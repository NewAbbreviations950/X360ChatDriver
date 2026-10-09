//! The uinput devices we expose: one Xbox 360 pad per connected controller, and one keyboard
//! shared by all chatpads.

use std::collections::HashMap;
use std::io;
use std::os::fd::AsRawFd;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use evdev::uinput::VirtualDevice;
use evdev::{
    AbsInfo, AbsoluteAxisCode, AttributeSet, BusType, EventSummary, EventType, FFEffectCode, FFEffectKind,
    InputEvent, InputId, KeyCode, UInputCode, UinputAbsSetup,
};
use x360chatpad_proto::{button, modifier, ChatpadKey, ChatpadKeys, GamepadState};

use crate::keymap::{Combo, Keymap, Layer};

/// Identify as a wired Xbox 360 pad (what xpad reports for 045e:028e), which SDL and Steam
/// recognise out of the box.
const PAD_ID: (u16, u16, u16) = (0x045e, 0x028e, 0x0114);
const PAD_NAME: &str = "Microsoft X-Box 360 pad";
const FF_EFFECTS_MAX: u16 = 16;

const BUTTONS: &[(u16, KeyCode)] = &[
    (button::A, KeyCode::BTN_SOUTH),
    (button::B, KeyCode::BTN_EAST),
    (button::X, KeyCode::BTN_NORTH),
    (button::Y, KeyCode::BTN_WEST),
    (button::LB, KeyCode::BTN_TL),
    (button::RB, KeyCode::BTN_TR),
    (button::BACK, KeyCode::BTN_SELECT),
    (button::START, KeyCode::BTN_START),
    (button::GUIDE, KeyCode::BTN_MODE),
    (button::LS, KeyCode::BTN_THUMBL),
    (button::RS, KeyCode::BTN_THUMBR),
];

pub struct Pad {
    dev: VirtualDevice,
    /// Uploaded rumble effects: id → (strong, weak).
    effects: HashMap<i16, (u16, u16)>,
    /// When the currently playing effect ends on its own.
    stop_at: Option<Instant>,
    rumble: (u8, u8),
}

impl Pad {
    pub fn new() -> io::Result<Self> {
        let keys: AttributeSet<KeyCode> = BUTTONS.iter().map(|&(_, k)| k).collect();
        let stick = AbsInfo::new(0, -32768, 32767, 16, 128, 0);
        let trigger = AbsInfo::new(0, 0, 255, 0, 0, 0);
        let hat = AbsInfo::new(0, -1, 1, 0, 0, 0);
        let mut builder = VirtualDevice::builder()?
            .name(PAD_NAME)
            .input_id(InputId::new(BusType::BUS_USB, PAD_ID.0, PAD_ID.1, PAD_ID.2))
            .with_keys(&keys)?
            .with_ff(&AttributeSet::from_iter([FFEffectCode::FF_RUMBLE]))?
            .with_ff_effects_max(FF_EFFECTS_MAX as u32);
        for (axis, info) in [
            (AbsoluteAxisCode::ABS_X, stick),
            (AbsoluteAxisCode::ABS_Y, stick),
            (AbsoluteAxisCode::ABS_RX, stick),
            (AbsoluteAxisCode::ABS_RY, stick),
            (AbsoluteAxisCode::ABS_Z, trigger),
            (AbsoluteAxisCode::ABS_RZ, trigger),
            (AbsoluteAxisCode::ABS_HAT0X, hat),
            (AbsoluteAxisCode::ABS_HAT0Y, hat),
        ] {
            builder = builder.with_absolute_axis(&UinputAbsSetup::new(axis, info))?;
        }
        let dev = builder.build()?;
        set_nonblocking(&dev)?;
        Ok(Pad { dev, effects: HashMap::new(), stop_at: None, rumble: (0, 0) })
    }

    pub fn emit(&mut self, s: &GamepadState) -> io::Result<()> {
        let mut events: Vec<InputEvent> = BUTTONS
            .iter()
            .map(|&(bit, key)| InputEvent::new(EventType::KEY.0, key.0, (s.buttons & bit != 0) as i32))
            .collect();
        let dir = |neg: u16, pos: u16| (s.buttons & pos != 0) as i32 - (s.buttons & neg != 0) as i32;
        // Like xpad: evdev Y axes point down, so flip the controller's up-positive values.
        for (axis, value) in [
            (AbsoluteAxisCode::ABS_X, s.lx as i32),
            (AbsoluteAxisCode::ABS_Y, !s.ly as i32),
            (AbsoluteAxisCode::ABS_RX, s.rx as i32),
            (AbsoluteAxisCode::ABS_RY, !s.ry as i32),
            (AbsoluteAxisCode::ABS_Z, s.lt as i32),
            (AbsoluteAxisCode::ABS_RZ, s.rt as i32),
            (AbsoluteAxisCode::ABS_HAT0X, dir(button::DPAD_LEFT, button::DPAD_RIGHT)),
            (AbsoluteAxisCode::ABS_HAT0Y, dir(button::DPAD_UP, button::DPAD_DOWN)),
        ] {
            events.push(InputEvent::new(EventType::ABSOLUTE.0, axis.0, value));
        }
        self.dev.emit(&events)
    }

    /// Handle pending force-feedback requests from games. Returns the new rumble strength
    /// `(strong, weak)` if it changed and should be sent to the controller.
    pub fn poll_ff(&mut self) -> io::Result<Option<(u8, u8)>> {
        let mut target = None;
        let events: Vec<InputEvent> = match self.dev.fetch_events() {
            Ok(events) => events.collect(),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Vec::new(),
            Err(e) => return Err(e),
        };
        for event in events {
            match event.destructure() {
                EventSummary::UInput(event, UInputCode::UI_FF_UPLOAD, ..) => {
                    let mut upload = self.dev.process_ff_upload(event)?;
                    let id = upload.effect_id();
                    let id = if (0..FF_EFFECTS_MAX as i16).contains(&id) {
                        id
                    } else {
                        // New effect: pick the lowest free id.
                        match (0..FF_EFFECTS_MAX as i16).find(|i| !self.effects.contains_key(i)) {
                            Some(free) => free,
                            None => {
                                upload.set_retval(-libc::ENOSPC);
                                continue;
                            }
                        }
                    };
                    let magnitudes = match upload.effect().kind {
                        FFEffectKind::Rumble { strong_magnitude, weak_magnitude } => (strong_magnitude, weak_magnitude),
                        _ => (0, 0),
                    };
                    self.effects.insert(id, magnitudes);
                    upload.set_effect_id(id);
                    upload.set_retval(0);
                }
                EventSummary::UInput(event, UInputCode::UI_FF_ERASE, ..) => {
                    let mut erase = self.dev.process_ff_erase(event)?;
                    self.effects.remove(&(erase.effect_id() as i16));
                    erase.set_retval(0);
                }
                EventSummary::ForceFeedback(_, code, value) => {
                    let id = code.0 as i16;
                    if value > 0 {
                        let (strong, weak) = self.effects.get(&id).copied().unwrap_or((0, 0));
                        target = Some(((strong >> 8) as u8, (weak >> 8) as u8));
                        // Games mostly stop effects explicitly; cap runaway rumble just in case.
                        self.stop_at = Some(Instant::now() + Duration::from_secs(10));
                    } else {
                        target = Some((0, 0));
                        self.stop_at = None;
                    }
                }
                _ => {}
            }
        }
        if target.is_none() && self.stop_at.is_some_and(|t| Instant::now() >= t) {
            target = Some((0, 0));
            self.stop_at = None;
        }
        Ok(match target {
            Some(r) if r != self.rumble => {
                self.rumble = r;
                Some(r)
            }
            _ => None,
        })
    }
}

/// The virtual keyboard all chatpads type into.
pub struct Keyboard {
    dev: VirtualDevice,
}

impl Keyboard {
    pub fn new(keymap: &Keymap) -> io::Result<Self> {
        let keys: AttributeSet<KeyCode> = keymap.all_codes().collect();
        let dev = VirtualDevice::builder()?
            .name("X360 Chatpad")
            .input_id(InputId::new(BusType::BUS_VIRTUAL, 0x045e, 0x0719, 1))
            .with_keys(&keys)?
            .build()?;
        Ok(Keyboard { dev })
    }

    fn send(&mut self, key: KeyCode, down: bool) -> io::Result<()> {
        self.dev.emit(&[InputEvent::new(EventType::KEY.0, key.0, down as i32)])
    }
}

/// Turns one chatpad's key reports into key presses on the shared keyboard.
#[derive(Default)]
pub struct ChatpadState {
    modifiers: u8,
    /// Raw codes currently held, with the combo pressed for each (released in reverse).
    held: Vec<(u8, Combo)>,
}

impl ChatpadState {
    pub fn update(&mut self, report: ChatpadKeys, keymap: &Keymap, keyboard: &Mutex<Keyboard>) -> io::Result<()> {
        let mut kb = keyboard.lock().unwrap();

        for (bit, key) in [(modifier::SHIFT, keymap.shift), (modifier::PEOPLE, keymap.people)] {
            let (was, is) = (self.modifiers & bit != 0, report.modifiers & bit != 0);
            if was != is {
                kb.send(key, is)?;
            }
        }
        self.modifiers = report.modifiers;

        // Release keys no longer reported.
        let mut i = 0;
        while i < self.held.len() {
            if report.keys.contains(&self.held[i].0) {
                i += 1;
            } else {
                let (_, combo) = self.held.remove(i);
                for &k in combo.iter().rev() {
                    kb.send(k, false)?;
                }
            }
        }

        // Press newly reported keys, using the layer that's active right now.
        let layer = if report.modifiers & modifier::GREEN != 0 {
            Layer::Green
        } else if report.modifiers & modifier::ORANGE != 0 {
            Layer::Orange
        } else {
            Layer::Normal
        };
        for &code in report.keys.iter().filter(|&&c| c != 0) {
            if self.held.iter().any(|(c, _)| *c == code) {
                continue;
            }
            let combo = match ChatpadKey::from_code(code).and_then(|k| keymap.lookup(layer, k)) {
                Some(combo) => combo.clone(),
                None => {
                    eprintln!("unknown chatpad key code {code:#04x}");
                    Vec::new()
                }
            };
            for &k in &combo {
                kb.send(k, true)?;
            }
            self.held.push((code, combo));
        }
        Ok(())
    }

    /// Release everything (controller disconnected or daemon stopping).
    pub fn release_all(&mut self, keymap: &Keymap, keyboard: &Mutex<Keyboard>) -> io::Result<()> {
        self.update(ChatpadKeys::default(), keymap, keyboard)
    }
}

fn set_nonblocking(dev: &VirtualDevice) -> io::Result<()> {
    let fd = dev.as_raw_fd();
    // SAFETY: plain fcntl on a file descriptor we own for the duration of the call.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
