//! Protocol exploration tool: takes the controller interfaces of an Xbox 360 wireless
//! receiver away from xpad, hex-dumps everything they send, and lets you type packets
//! to send back. xpad is reattached on exit (Ctrl-C or `q`).
//!
//! stdin commands:
//!   <slot> <hex bytes>   send a packet to slot 0-3, e.g. `0 00 00 0c 1b`
//!   k <slot> on|off      toggle chatpad keepalive + wake handshake for a slot
//!   q                    quit

use std::io::BufRead;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use nusb::MaybeFuture;
use nusb::transfer::{Buffer, In, Interrupt, Out};

const RECEIVER_IDS: &[(u16, u16)] = &[(0x045e, 0x0719), (0x045e, 0x0291), (0x045e, 0x02a9)];
const SLOTS: u8 = 4;

enum Cmd {
    Send(Vec<u8>),
    Keepalive(bool),
}

fn main() -> Result<()> {
    let info = nusb::list_devices()
        .wait()?
        .find(|d| RECEIVER_IDS.contains(&(d.vendor_id(), d.product_id())))
        .context("no Xbox 360 wireless receiver found")?;
    println!(
        "found receiver {:04x}:{:04x} on bus {} addr {}",
        info.vendor_id(),
        info.product_id(),
        info.busnum(),
        info.device_address()
    );
    let device = info
        .open()
        .wait()
        .context("open receiver (need the udev rule or root)")?;

    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        ctrlc::set_handler(move || running.store(false, Ordering::SeqCst))?;
    }

    let result = run(&device, &running);
    // Give every controller interface back to xpad, even ones a crashed earlier run left orphaned.
    for slot in 0..SLOTS {
        let _ = device.attach_kernel_driver(slot * 2);
    }
    println!("released interfaces; xpad reattached");
    result
}

fn run(device: &nusb::Device, running: &Arc<AtomicBool>) -> Result<()> {
    // Claim everything before starting threads so a failure leaves nothing half-running.
    let mut claimed = Vec::new();
    for slot in 0..SLOTS {
        let iface = device
            .detach_and_claim_interface(slot * 2)
            .wait()
            .with_context(|| format!("claim interface {}", slot * 2))?;
        // Slot N uses interface 2N with endpoints 0x81+2N (in) / 0x01+2N (out).
        let ep_in = iface.endpoint::<Interrupt, In>(0x81 + slot * 2)?;
        let ep_out = iface.endpoint::<Interrupt, Out>(0x01 + slot * 2)?;
        claimed.push((slot, iface, ep_in, ep_out));
    }

    let start = Instant::now();
    let mut senders = Vec::new();
    let mut threads = Vec::new();
    for (slot, iface, mut ep_in, mut ep_out) in claimed {
        let (tx, rx) = mpsc::channel::<Cmd>();
        senders.push(tx);
        let running = running.clone();
        threads.push(thread::spawn(move || {
            let _iface = iface; // keep claimed (and reattach xpad on drop)
            let mut keepalive = false;
            let mut last_keepalive = Instant::now();
            let mut toggle = false;
            while ep_in.pending() < 4 {
                ep_in.submit(ep_in.allocate(64));
            }
            while running.load(Ordering::SeqCst) {
                while let Ok(cmd) = rx.try_recv() {
                    match cmd {
                        Cmd::Send(bytes) => send(&mut ep_out, slot, &bytes),
                        Cmd::Keepalive(on) => keepalive = on,
                    }
                }
                if keepalive && last_keepalive.elapsed() >= Duration::from_secs(1) {
                    send(
                        &mut ep_out,
                        slot,
                        &chatpad_cmd(if toggle { 0x1f } else { 0x1e }),
                    );
                    toggle = !toggle;
                    last_keepalive = Instant::now();
                }
                if let Some(c) = ep_in.wait_next_complete(Duration::from_millis(50)) {
                    let t = start.elapsed().as_secs_f64();
                    match c.status {
                        Ok(()) => {
                            let d = &c.buffer[..c.actual_len];
                            println!("{t:9.3} slot{slot} IN  {}", hex(d));
                            // Chatpad wake request: the host must answer with 0x1b each time.
                            if keepalive
                                && d.len() >= 26
                                && d[..4] == [0x00, 0x02, 0x00, 0xf0]
                                && d[24] == 0xf0
                                && d[25] == 0x03
                            {
                                send(&mut ep_out, slot, &chatpad_cmd(0x1b));
                            }
                        }
                        Err(e) => {
                            println!("{t:9.3} slot{slot} IN  error: {e:?}");
                            thread::sleep(Duration::from_millis(200));
                        }
                    }
                    let mut buf = c.buffer;
                    buf.clear();
                    ep_in.submit(buf);
                }
            }
            ep_in.cancel_all();
        }));
    }
    println!("claimed all {SLOTS} controller interfaces; xpad detached. Ctrl-C or `q` to quit.");

    // stdin runs on its own thread so Ctrl-C can still end the program while it blocks.
    {
        let running = running.clone();
        thread::spawn(move || {
            for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                match parse_cmd(&line) {
                    Ok(None) => {
                        running.store(false, Ordering::SeqCst);
                        break;
                    }
                    Ok(Some((slot, cmd))) => {
                        let _ = senders[slot].send(cmd);
                    }
                    Err(e) => eprintln!("? {e}"),
                }
            }
        });
    }

    for t in threads {
        let _ = t.join();
    }
    Ok(())
}

fn send(ep: &mut nusb::Endpoint<Interrupt, Out>, slot: u8, bytes: &[u8]) {
    let mut buf = Buffer::new(bytes.len());
    buf.extend_from_slice(bytes);
    let c = ep.transfer_blocking(buf, Duration::from_millis(500));
    println!("          slot{slot} OUT {} -> {:?}", hex(bytes), c.status);
}

fn chatpad_cmd(cmd: u8) -> [u8; 12] {
    [0x00, 0x00, 0x0c, cmd, 0, 0, 0, 0, 0, 0, 0, 0]
}

/// `Ok(None)` means quit.
fn parse_cmd(line: &str) -> Result<Option<(usize, Cmd)>> {
    let mut words = line.split_whitespace();
    let first = match words.next() {
        Some(w) => w,
        None => bail!("empty line"),
    };
    if first == "q" {
        return Ok(None);
    }
    if first == "k" {
        let slot = parse_slot(words.next())?;
        let on = matches!(words.next(), Some("on"));
        return Ok(Some((slot, Cmd::Keepalive(on))));
    }
    let slot = parse_slot(Some(first))?;
    let bytes = words
        .flat_map(|w| {
            w.as_bytes()
                .chunks(2)
                .map(|c| std::str::from_utf8(c).unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .map(|b| u8::from_str_radix(&b, 16).with_context(|| format!("bad hex byte {b:?}")))
        .collect::<Result<Vec<u8>>>()?;
    if bytes.is_empty() {
        bail!("no bytes to send");
    }
    Ok(Some((slot, Cmd::Send(bytes))))
}

fn parse_slot(w: Option<&str>) -> Result<usize> {
    let slot: usize = w
        .context("missing slot")?
        .parse()
        .context("slot must be 0-3")?;
    if slot >= SLOTS as usize {
        bail!("slot must be 0-3");
    }
    Ok(slot)
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
