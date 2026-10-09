//! The uinput devices we expose: one Xbox 360 pad per connected controller, and one keyboard
//! shared by all chatpads.

use std::collections::HashMap;
use std::io;
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use evdev::uinput::VirtualDevice;
use evdev::{
    AbsInfo, AbsoluteAxisCode, AttributeSet, BusType, EventSummary, EventType, FFEffectCode,
    FFEffectKind, InputEvent, InputId, KeyCode, UInputCode, UinputAbsSetup,
};
use x360chatpad_proto::{ChatpadKey, ChatpadKeys, GamepadState, button, modifier};

use crate::keymap::{Keymap, Layer};

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
    /// Last state from the controller, and buttons we hold down on top of it (chatpad shortcuts).
    last: GamepadState,
    overlay: u16,
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
        Ok(Pad {
            dev,
            effects: HashMap::new(),
            stop_at: None,
            rumble: (0, 0),
            last: GamepadState::default(),
            overlay: 0,
        })
    }

    pub fn emit(&mut self, state: &GamepadState) -> io::Result<()> {
        self.last = *state;
        let s = &GamepadState {
            buttons: state.buttons | self.overlay,
            ..*state
        };
        let mut events: Vec<InputEvent> = BUTTONS
            .iter()
            .map(|&(bit, key)| {
                InputEvent::new(EventType::KEY.0, key.0, (s.buttons & bit != 0) as i32)
            })
            .collect();
        let dir =
            |neg: u16, pos: u16| (s.buttons & pos != 0) as i32 - (s.buttons & neg != 0) as i32;
        // Like xpad: evdev Y axes point down, so flip the controller's up-positive values.
        for (axis, value) in [
            (AbsoluteAxisCode::ABS_X, s.lx as i32),
            (AbsoluteAxisCode::ABS_Y, !s.ly as i32),
            (AbsoluteAxisCode::ABS_RX, s.rx as i32),
            (AbsoluteAxisCode::ABS_RY, !s.ry as i32),
            (AbsoluteAxisCode::ABS_Z, s.lt as i32),
            (AbsoluteAxisCode::ABS_RZ, s.rt as i32),
            (
                AbsoluteAxisCode::ABS_HAT0X,
                dir(button::DPAD_LEFT, button::DPAD_RIGHT),
            ),
            (
                AbsoluteAxisCode::ABS_HAT0Y,
                dir(button::DPAD_UP, button::DPAD_DOWN),
            ),
        ] {
            events.push(InputEvent::new(EventType::ABSOLUTE.0, axis.0, value));
        }
        self.dev.emit(&events)
    }

    /// Hold `buttons` down in addition to whatever the controller reports.
    pub fn set_overlay(&mut self, buttons: u16) -> io::Result<()> {
        self.overlay = buttons;
        let last = self.last;
        self.emit(&last)
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
                        FFEffectKind::Rumble {
                            strong_magnitude,
                            weak_magnitude,
                        } => (strong_magnitude, weak_magnitude),
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
    pub fn new() -> io::Result<Self> {
        // Declare every regular key: which ones we press depends on the user's layout.
        let keys: AttributeSet<KeyCode> = (1..=KeyCode::KEY_MICMUTE.0).map(KeyCode).collect();
        let dev = VirtualDevice::builder()?
            .name("X360 Chatpad")
            .input_id(InputId::new(BusType::BUS_VIRTUAL, 0x045e, 0x0719, 1))
            .with_keys(&keys)?
            .build()?;
        Ok(Keyboard { dev })
    }

    pub fn send(&mut self, events: &[(KeyCode, bool)]) -> io::Result<()> {
        // One SYN per key event, so compositors see presses in order and never merged.
        for &(key, down) in events {
            self.dev
                .emit(&[InputEvent::new(EventType::KEY.0, key.0, down as i32)])?;
        }
        Ok(())
    }
}

/// Shift, Green and Orange are one-shot like on the console: tap one, then the key. Holding
/// the modifier while pressing the key works too.
const STICKY: [u8; 3] = [modifier::SHIFT, modifier::GREEN, modifier::ORANGE];

/// What a chatpad report should do.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ChatpadOutput {
    /// Key events for the shared keyboard.
    pub keys: Vec<(KeyCode, bool)>,
    /// Press (`true`) or release the Steam Quick Access shortcut (Guide+A) on this controller's pad.
    pub quick_menu: Option<bool>,
}

/// Steam opens its Quick Access menu on Guide+A.
pub const QUICK_MENU_BUTTONS: u16 = button::GUIDE | button::A;

/// Turns one chatpad's key reports into key events for the shared keyboard.
///
/// Special combinations (either key may be held or tapped first):
/// - Shift + Orange toggles Caps Lock.
/// - Green + People opens the Steam Quick Access menu; People alone is the Meta key.
#[derive(Default)]
pub struct ChatpadState {
    modifiers: u8,
    /// One-shot modifiers waiting for the next key.
    latched: u8,
    /// Held modifiers that were used for something (so releasing them doesn't latch).
    used: u8,
    /// Whether the held People key is acting as the Quick Access shortcut rather than Meta.
    people_quick_menu: bool,
    /// Raw codes currently held, with the chord held for each (released in reverse).
    held: Vec<(u8, Vec<KeyCode>)>,
}

impl ChatpadState {
    pub fn update(&mut self, report: ChatpadKeys, keymap: &Keymap) -> ChatpadOutput {
        let mut out = ChatpadOutput::default();
        let pressed = report.modifiers & !self.modifiers;
        let released = self.modifiers & !report.modifiers;
        self.used &= !pressed;

        if pressed & modifier::PEOPLE != 0 {
            let green = (report.modifiers | self.latched) & modifier::GREEN;
            self.people_quick_menu = green != 0;
            if self.people_quick_menu {
                self.latched &= !modifier::GREEN;
                self.used |= green & report.modifiers;
                out.quick_menu = Some(true);
            } else {
                out.keys.push((keymap.people, true));
            }
        }
        if released & modifier::PEOPLE != 0 {
            if self.people_quick_menu {
                out.quick_menu = Some(false);
            } else {
                out.keys.push((keymap.people, false));
            }
        }

        for bit in STICKY {
            if released & bit == 0 || self.used & bit != 0 {
                continue;
            }
            let caps_partner = match bit {
                modifier::SHIFT => modifier::ORANGE,
                modifier::ORANGE => modifier::SHIFT,
                _ => 0,
            };
            if (report.modifiers | self.latched) & caps_partner != 0 {
                out.keys.extend([
                    (KeyCode::KEY_CAPSLOCK, true),
                    (KeyCode::KEY_CAPSLOCK, false),
                ]);
                self.latched &= !(bit | caps_partner);
                self.used |= caps_partner & report.modifiers;
                continue;
            }
            self.latched ^= bit;
            // Green and Orange are alternatives; latching one cancels the other.
            if bit != modifier::SHIFT && self.latched & bit != 0 {
                self.latched &= !(modifier::GREEN | modifier::ORANGE) | bit;
            }
        }
        self.modifiers = report.modifiers;

        // Release keys no longer reported.
        self.held.retain(|(code, chord)| {
            if report.keys.contains(code) {
                return true;
            }
            out.keys.extend(chord.iter().rev().map(|&k| (k, false)));
            false
        });

        for &code in report.keys.iter().filter(|&&c| c != 0) {
            if self.held.iter().any(|(c, _)| *c == code) {
                continue;
            }
            let active = report.modifiers | self.latched;
            let layer = if active & modifier::GREEN != 0 {
                Layer::Green
            } else if active & modifier::ORANGE != 0 {
                Layer::Orange
            } else {
                Layer::Normal
            };
            let combo = match ChatpadKey::from_code(code) {
                Some(key) => keymap.combo(layer, key, active & modifier::SHIFT != 0),
                None => {
                    eprintln!("unknown chatpad key code {code:#04x}");
                    None
                }
            };
            let chord = match combo {
                Some(combo) => {
                    for tap in &combo.taps {
                        out.keys.extend(tap.iter().map(|&k| (k, true)));
                        out.keys.extend(tap.iter().rev().map(|&k| (k, false)));
                    }
                    out.keys.extend(combo.hold.iter().map(|&k| (k, true)));
                    combo.hold
                }
                None => Vec::new(),
            };
            self.held.push((code, chord));
            self.latched = 0;
            self.used |= report.modifiers;
        }
        out
    }

    /// Release everything (controller disconnected or daemon stopping).
    pub fn release_all(&mut self, keymap: &Keymap) -> ChatpadOutput {
        let out = self.update(ChatpadKeys::default(), keymap);
        self.latched = 0;
        out
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Layout;
    use KeyCode as K;

    fn report(modifiers: u8, keys: &[u8]) -> ChatpadKeys {
        let mut r = ChatpadKeys {
            modifiers,
            keys: [0; 2],
        };
        r.keys[..keys.len()].copy_from_slice(keys);
        r
    }

    const A: u8 = 0x37;
    const Q: u8 = 0x27;

    #[test]
    fn plain_key() {
        let map = Keymap::new(Layout::us());
        let mut s = ChatpadState::default();
        assert_eq!(s.update(report(0, &[A]), &map).keys, vec![(K::KEY_A, true)]);
        assert_eq!(s.update(report(0, &[]), &map).keys, vec![(K::KEY_A, false)]);
    }

    #[test]
    fn held_shift() {
        let map = Keymap::new(Layout::us());
        let mut s = ChatpadState::default();
        assert!(s.update(report(modifier::SHIFT, &[]), &map).keys.is_empty());
        assert_eq!(
            s.update(report(modifier::SHIFT, &[A]), &map).keys,
            vec![(K::KEY_LEFTSHIFT, true), (K::KEY_A, true)]
        );
        s.update(report(modifier::SHIFT, &[]), &map);
        s.update(report(0, &[]), &map);
        // Shift was used while held, so it must not latch for the next key.
        assert_eq!(s.update(report(0, &[A]), &map).keys, vec![(K::KEY_A, true)]);
    }

    #[test]
    fn tapped_green_applies_once() {
        let map = Keymap::new(Layout::us());
        let mut s = ChatpadState::default();
        s.update(report(modifier::GREEN, &[]), &map);
        s.update(report(0, &[]), &map);
        // Green+Q = '!'
        assert_eq!(
            s.update(report(0, &[Q]), &map).keys,
            vec![(K::KEY_LEFTSHIFT, true), (K::KEY_1, true)]
        );
        assert_eq!(
            s.update(report(0, &[]), &map).keys,
            vec![(K::KEY_1, false), (K::KEY_LEFTSHIFT, false)]
        );
        assert_eq!(s.update(report(0, &[Q]), &map).keys, vec![(K::KEY_Q, true)]);
    }

    #[test]
    fn tapping_green_twice_cancels() {
        let map = Keymap::new(Layout::us());
        let mut s = ChatpadState::default();
        for _ in 0..2 {
            s.update(report(modifier::GREEN, &[]), &map);
            s.update(report(0, &[]), &map);
        }
        assert_eq!(s.update(report(0, &[Q]), &map).keys, vec![(K::KEY_Q, true)]);
    }

    #[test]
    fn orange_replaces_latched_green() {
        let map = Keymap::new(Layout::us());
        let mut s = ChatpadState::default();
        for m in [modifier::GREEN, modifier::ORANGE] {
            s.update(report(m, &[]), &map);
            s.update(report(0, &[]), &map);
        }
        // Orange+R = '$'
        assert_eq!(
            s.update(report(0, &[0x24]), &map).keys,
            vec![(K::KEY_LEFTSHIFT, true), (K::KEY_4, true)]
        );
    }

    const CAPS: [(KeyCode, bool); 2] = [(K::KEY_CAPSLOCK, true), (K::KEY_CAPSLOCK, false)];

    #[test]
    fn shift_plus_orange_toggles_caps_lock() {
        let map = Keymap::new(Layout::us());
        // Hold Shift, tap Orange.
        let mut s = ChatpadState::default();
        s.update(report(modifier::SHIFT, &[]), &map);
        s.update(report(modifier::SHIFT | modifier::ORANGE, &[]), &map);
        assert_eq!(s.update(report(modifier::SHIFT, &[]), &map).keys, CAPS);
        s.update(report(0, &[]), &map);
        // Nothing left latched: the next key is plain.
        assert_eq!(s.update(report(0, &[A]), &map).keys, vec![(K::KEY_A, true)]);

        // Tap Orange, then tap Shift.
        let mut s = ChatpadState::default();
        s.update(report(modifier::ORANGE, &[]), &map);
        s.update(report(0, &[]), &map);
        s.update(report(modifier::SHIFT, &[]), &map);
        assert_eq!(s.update(report(0, &[]), &map).keys, CAPS);
        assert_eq!(s.update(report(0, &[A]), &map).keys, vec![(K::KEY_A, true)]);
    }

    #[test]
    fn green_plus_people_is_quick_menu() {
        let map = Keymap::new(Layout::us());
        let mut s = ChatpadState::default();
        s.update(report(modifier::GREEN, &[]), &map);
        let down = s.update(report(modifier::GREEN | modifier::PEOPLE, &[]), &map);
        assert_eq!(
            down,
            ChatpadOutput {
                keys: vec![],
                quick_menu: Some(true)
            }
        );
        let up = s.update(report(modifier::GREEN, &[]), &map);
        assert_eq!(
            up,
            ChatpadOutput {
                keys: vec![],
                quick_menu: Some(false)
            }
        );
        // Green was used, so releasing it doesn't latch.
        s.update(report(0, &[]), &map);
        assert_eq!(s.update(report(0, &[Q]), &map).keys, vec![(K::KEY_Q, true)]);
    }

    #[test]
    fn tapped_green_then_people_is_quick_menu() {
        let map = Keymap::new(Layout::us());
        let mut s = ChatpadState::default();
        s.update(report(modifier::GREEN, &[]), &map);
        s.update(report(0, &[]), &map);
        assert_eq!(
            s.update(report(modifier::PEOPLE, &[]), &map).quick_menu,
            Some(true)
        );
        assert_eq!(s.update(report(0, &[]), &map).quick_menu, Some(false));
    }

    #[test]
    fn people_alone_is_meta() {
        let map = Keymap::new(Layout::us());
        let mut s = ChatpadState::default();
        assert_eq!(
            s.update(report(modifier::PEOPLE, &[]), &map).keys,
            vec![(K::KEY_LEFTMETA, true)]
        );
        assert_eq!(
            s.update(report(0, &[]), &map).keys,
            vec![(K::KEY_LEFTMETA, false)]
        );
    }
}
