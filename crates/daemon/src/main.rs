//! x360chatpad-daemon: takes an Xbox 360 Wireless Receiver over from xpad and exposes each
//! controller as a virtual Xbox 360 pad, plus a shared virtual keyboard for chatpads.

mod actions;
mod config;
mod devices;
mod indicators;
mod keyboard;
mod keymap;
mod layout;

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use nusb::transfer::{Buffer, In, Interrupt, Out, TransferError};
use nusb::{DeviceInfo, Endpoint, MaybeFuture};
use x360chatpad_proto::{ChatpadEvent, InPacket, out, parse};

use devices::{ChatpadState, Pad};
use keyboard::Keyboard;
use keymap::Keymap;
use layout::Layout;

const RECEIVER_IDS: &[(u16, u16)] = &[(0x045e, 0x0719), (0x045e, 0x0291), (0x045e, 0x02a9)];
const SLOTS: u8 = 4;
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(1);

struct Shared {
    running: AtomicBool,
    keymap: Keymap,
    keyboard: Mutex<Keyboard>,
    /// System Caps Lock state, as mirrored onto our keyboard by the compositor.
    caps_lock: AtomicBool,
}

impl Shared {
    fn type_keys(&self, events: &[(evdev::KeyCode, bool)]) -> std::io::Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        self.keyboard.lock().unwrap().send(events)
    }
}

fn main() -> Result<()> {
    let config = config::Config::load();
    let layout = Layout::load(config.layout_name());
    eprintln!("typing for keyboard layout {}", layout.name);
    let keymap = Keymap::new(layout);
    let (keyboard, mut leds) =
        Keyboard::new().context("create virtual keyboard (is /dev/uinput accessible?)")?;
    let shared = Arc::new(Shared {
        running: AtomicBool::new(true),
        keymap,
        keyboard: Mutex::new(keyboard),
        caps_lock: AtomicBool::new(false),
    });
    {
        let shared = shared.clone();
        thread::spawn(move || {
            while let Ok(on) = leds.next_caps_lock() {
                shared.caps_lock.store(on, Ordering::SeqCst);
            }
        });
    }
    {
        let shared = shared.clone();
        ctrlc::set_handler(move || shared.running.store(false, Ordering::SeqCst))?;
    }

    let mut waiting_logged = false;
    while shared.running.load(Ordering::SeqCst) {
        match find_receiver() {
            Ok(Some(info)) => {
                waiting_logged = false;
                eprintln!(
                    "receiver {:04x}:{:04x} at {}",
                    info.vendor_id(),
                    info.product_id(),
                    info.sysfs_path().display()
                );
                if let Err(e) = run_receiver(&info, &shared) {
                    eprintln!("receiver stopped: {e:#}");
                }
            }
            Ok(None) if !waiting_logged => {
                eprintln!("waiting for an Xbox 360 wireless receiver…");
                waiting_logged = true;
            }
            Ok(None) => {}
            Err(e) => eprintln!("listing USB devices failed: {e}"),
        }
        sleep_while_running(&shared, Duration::from_secs(2));
    }
    eprintln!("stopped");
    Ok(())
}

fn find_receiver() -> Result<Option<DeviceInfo>> {
    Ok(nusb::list_devices()
        .wait()?
        .find(|d| RECEIVER_IDS.contains(&(d.vendor_id(), d.product_id()))))
}

fn run_receiver(info: &DeviceInfo, shared: &Arc<Shared>) -> Result<()> {
    let device = info
        .open()
        .wait()
        .context("open receiver (is the udev rule installed?)")?;

    // Claim every slot before starting any, so a failure never leaves half a receiver running.
    let claimed = (0..SLOTS)
        .map(|slot| {
            let iface = device.detach_and_claim_interface(slot * 2).wait();
            iface.with_context(|| format!("claim interface {}", slot * 2))
        })
        .collect::<Result<Vec<_>>>();
    let claimed = match claimed {
        Ok(claimed) => claimed,
        Err(e) => {
            restore_xpad(&device, info);
            return Err(e);
        }
    };

    let threads: Vec<_> = claimed
        .into_iter()
        .zip(0..SLOTS)
        .map(|(iface, slot)| {
            let shared = shared.clone();
            thread::spawn(move || {
                let result = run_slot(slot, &iface, &shared);
                if let Err(e) = &result {
                    eprintln!("slot {slot}: {e:#}");
                }
                // Dropping `iface` releases it and lets xpad have it back.
                result
            })
        })
        .collect();
    let mut result = Ok(());
    for t in threads {
        if let Ok(Err(e)) = t.join() {
            result = Err(e);
        }
    }
    restore_xpad(&device, info);
    result
}

fn run_slot(slot: u8, iface: &nusb::Interface, shared: &Shared) -> Result<()> {
    let mut ep_in = iface.endpoint::<Interrupt, In>(0x81 + slot * 2)?;
    let mut ep_out = iface.endpoint::<Interrupt, Out>(0x01 + slot * 2)?;
    while ep_in.pending() < 4 {
        ep_in.submit(ep_in.allocate(32));
    }
    send(&mut ep_out, &out::presence_inquiry())?;

    let mut pad: Option<Pad> = None;
    let mut chatpad = ChatpadState::default();
    let mut indicators = indicators::IndicatorSync::default();
    let mut keepalive_phase = false;
    let mut next_keepalive = Instant::now();

    let result = (|| -> Result<()> {
        while shared.running.load(Ordering::SeqCst) {
            // Poll faster while a controller is connected so rumble requests go out promptly.
            let timeout = if pad.is_some() {
                Duration::from_millis(8)
            } else {
                Duration::from_millis(100)
            };
            if let Some(c) = ep_in.wait_next_complete(timeout) {
                match c.status {
                    Ok(()) => match parse(&c.buffer[..c.actual_len]) {
                        InPacket::Presence { connected: true } if pad.is_none() => {
                            eprintln!("slot {slot}: controller connected");
                            pad = Some(Pad::new().context("create virtual pad")?);
                            send(&mut ep_out, &out::led(2 + slot))?;
                            indicators.reset();
                            next_keepalive = Instant::now();
                        }
                        InPacket::Presence { connected: false } if pad.is_some() => {
                            eprintln!("slot {slot}: controller disconnected");
                            pad = None;
                            shared.type_keys(&chatpad.release_all(&shared.keymap).keys)?;
                        }
                        InPacket::Gamepad(state) => {
                            if let Some(pad) = pad.as_mut() {
                                pad.emit(&state)?;
                            }
                        }
                        InPacket::Chatpad(ChatpadEvent::WakeRequest) => {
                            send(&mut ep_out, &out::chatpad_wake_ack())?
                        }
                        InPacket::Chatpad(ChatpadEvent::Active { leds }) => {
                            indicators.on_status(leds)
                        }
                        InPacket::Chatpad(ChatpadEvent::Keys(keys)) => {
                            let out = chatpad.update(keys, &shared.keymap);
                            shared.type_keys(&out.keys)?;
                            if out.open_chat {
                                actions::open_steam_chat();
                            }
                            if out.caps_toggled {
                                // Show it right away; the compositor's LED update confirms it.
                                shared.caps_lock.fetch_xor(true, Ordering::SeqCst);
                            }
                            if let (Some(on), Some(pad)) = (out.quick_menu, pad.as_mut()) {
                                pad.quick_menu(on, Instant::now())?;
                            }
                        }
                        _ => {}
                    },
                    Err(TransferError::Disconnected) => anyhow::bail!("receiver unplugged"),
                    Err(e) => {
                        eprintln!("slot {slot}: read error {e:?}");
                        thread::sleep(Duration::from_millis(100));
                    }
                }
                let mut buf = c.buffer;
                buf.clear();
                ep_in.submit(buf);
            }

            if let Some(pad) = pad.as_mut() {
                pad.tick(Instant::now())?;
                if let Some((strong, weak)) = pad.poll_ff()? {
                    send(&mut ep_out, &out::rumble(strong, weak))?;
                }
                for (bit, on) in indicators.poll(
                    chatpad.indicators(shared.caps_lock.load(Ordering::SeqCst)),
                    Instant::now(),
                ) {
                    send(&mut ep_out, &out::chatpad_led(bit, on))?;
                }
                if Instant::now() >= next_keepalive {
                    send(&mut ep_out, &out::chatpad_keepalive(keepalive_phase))?;
                    keepalive_phase = !keepalive_phase;
                    next_keepalive = Instant::now() + KEEPALIVE_INTERVAL;
                }
            }
        }
        Ok(())
    })();

    let _ = shared.type_keys(&chatpad.release_all(&shared.keymap).keys);
    if pad.is_some() {
        let _ = send(&mut ep_out, &out::rumble(0, 0));
    }
    ep_in.cancel_all();
    result
}

fn send(ep: &mut Endpoint<Interrupt, Out>, packet: &out::Packet) -> Result<()> {
    let mut buf = Buffer::new(packet.len());
    buf.extend_from_slice(packet);
    match ep.transfer_blocking(buf, Duration::from_millis(200)).status {
        Ok(()) => Ok(()),
        Err(TransferError::Disconnected) => anyhow::bail!("receiver unplugged"),
        // A dropped output packet (e.g. controller out of range) isn't fatal.
        Err(e) => {
            eprintln!("send {packet:02x?} failed: {e:?}");
            Ok(())
        }
    }
}

fn info_still_present(info: &DeviceInfo) -> bool {
    info.sysfs_path().exists()
}

/// Hand every controller interface back to xpad. xpad's probe occasionally fails right after
/// a release, so check each interface actually got a driver and retry a few times.
fn restore_xpad(device: &nusb::Device, info: &DeviceInfo) {
    let sysfs = info.sysfs_path();
    let name = sysfs
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    for slot in 0..SLOTS {
        let iface = slot * 2;
        let driver_link = sysfs.join(format!("{name}:1.{iface}/driver"));
        for attempt in 0..5 {
            if has_driver(&driver_link) {
                break;
            }
            if attempt > 0 {
                thread::sleep(Duration::from_millis(200));
            }
            let _ = device.attach_kernel_driver(iface);
        }
        if !has_driver(&driver_link) && info_still_present(info) {
            eprintln!("warning: could not give interface {iface} back to xpad");
        }
    }
}

fn has_driver(link: &Path) -> bool {
    link.exists()
}

fn sleep_while_running(shared: &Shared, total: Duration) {
    let end = Instant::now() + total;
    while shared.running.load(Ordering::SeqCst) && Instant::now() < end {
        thread::sleep(Duration::from_millis(100));
    }
}
