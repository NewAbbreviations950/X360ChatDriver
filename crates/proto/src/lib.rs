//! Xbox 360 Wireless Receiver packet parsing and construction.
//! See `docs/protocol.md` for the format and how it was verified.

/// A packet received from one controller slot of the receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InPacket {
    /// Controller connected to / disconnected from this slot.
    Presence {
        connected: bool,
    },
    Gamepad(GamepadState),
    Chatpad(ChatpadEvent),
    /// Anything we don't understand (yet).
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GamepadState {
    /// Bitmask of `button::*`.
    pub buttons: u16,
    pub lt: u8,
    pub rt: u8,
    pub lx: i16,
    /// Up is positive, as the controller reports it.
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

/// Gamepad button bits in [`GamepadState::buttons`] (report byte 6 is the low byte, byte 7 the high byte).
pub mod button {
    pub const DPAD_UP: u16 = 0x0001;
    pub const DPAD_DOWN: u16 = 0x0002;
    pub const DPAD_LEFT: u16 = 0x0004;
    pub const DPAD_RIGHT: u16 = 0x0008;
    pub const START: u16 = 0x0010;
    pub const BACK: u16 = 0x0020;
    pub const LS: u16 = 0x0040;
    pub const RS: u16 = 0x0080;
    pub const LB: u16 = 0x0100;
    pub const RB: u16 = 0x0200;
    pub const GUIDE: u16 = 0x0400;
    pub const A: u16 = 0x1000;
    pub const B: u16 = 0x2000;
    pub const X: u16 = 0x4000;
    pub const Y: u16 = 0x8000;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatpadEvent {
    /// The chatpad wants to wake up; the host must answer with [`out::chatpad_wake_ack`].
    WakeRequest,
    /// Chatpad is awake and reporting keys.
    Active,
    Keys(ChatpadKeys),
}

/// Full chatpad state: held modifiers plus up to two held keys (raw codes, 0 = none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatpadKeys {
    /// Bitmask of `modifier::*`.
    pub modifiers: u8,
    pub keys: [u8; 2],
}

pub mod modifier {
    pub const SHIFT: u8 = 0x01;
    pub const GREEN: u8 = 0x02;
    pub const ORANGE: u8 = 0x04;
    pub const PEOPLE: u8 = 0x08;
}

pub fn parse(data: &[u8]) -> InPacket {
    match data {
        [0x08, flags, ..] => InPacket::Presence {
            connected: flags & 0x80 != 0,
        },
        [0x00, 0x01, 0x00, 0xf0, ..] if data.len() >= 18 => {
            let i16_at = |i: usize| i16::from_le_bytes([data[i], data[i + 1]]);
            InPacket::Gamepad(GamepadState {
                buttons: u16::from_le_bytes([data[6], data[7]]),
                lt: data[8],
                rt: data[9],
                lx: i16_at(10),
                ly: i16_at(12),
                rx: i16_at(14),
                ry: i16_at(16),
            })
        }
        [0x00, 0x02, 0x00, 0xf0, ..] if data.len() >= 28 => match (data[24], data[25]) {
            (0xf0, 0x03) => InPacket::Chatpad(ChatpadEvent::WakeRequest),
            (0xf0, 0x04) => InPacket::Chatpad(ChatpadEvent::Active),
            (0x00, modifiers) => InPacket::Chatpad(ChatpadEvent::Keys(ChatpadKeys {
                modifiers,
                keys: [data[26], data[27]],
            })),
            _ => InPacket::Other,
        },
        _ => InPacket::Other,
    }
}

/// Packets sent to a controller slot.
pub mod out {
    pub type Packet = [u8; 12];

    const fn packet(b0: u8, b1: u8, b2: u8, b3: u8) -> Packet {
        [b0, b1, b2, b3, 0, 0, 0, 0, 0, 0, 0, 0]
    }

    /// Ask whether a controller is connected; answered with a presence packet.
    pub const fn presence_inquiry() -> Packet {
        packet(0x08, 0x00, 0x0f, 0xc0)
    }

    /// Ring-of-light animation, using xpad's LED command numbers (2–5 = flash then player 1–4).
    pub const fn led(command: u8) -> Packet {
        packet(0x00, 0x00, 0x08, 0x40 | (command % 0x0e))
    }

    pub const fn power_off() -> Packet {
        packet(0x00, 0x00, 0x08, 0xc0)
    }

    pub const fn rumble(strong: u8, weak: u8) -> Packet {
        [0x00, 0x01, 0x0f, 0xc0, 0x00, strong, weak, 0, 0, 0, 0, 0]
    }

    /// Reply to every [`super::ChatpadEvent::WakeRequest`].
    pub const fn chatpad_wake_ack() -> Packet {
        packet(0x00, 0x00, 0x0c, 0x1b)
    }

    /// Keepalive, sent about once a second; `phase` alternates the two variants.
    pub const fn chatpad_keepalive(phase: bool) -> Packet {
        packet(0x00, 0x00, 0x0c, if phase { 0x1f } else { 0x1e })
    }
}

macro_rules! chatpad_keys {
    ($($name:ident = $code:literal),* $(,)?) => {
        /// A physical chatpad key (excluding the modifier keys, which are reported as bits).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum ChatpadKey { $($name),* }

        impl ChatpadKey {
            pub const ALL: &'static [ChatpadKey] = &[$(ChatpadKey::$name),*];

            pub fn from_code(code: u8) -> Option<Self> {
                match code {
                    $($code => Some(ChatpadKey::$name),)*
                    _ => None,
                }
            }

            pub fn code(self) -> u8 {
                match self { $(ChatpadKey::$name => $code),* }
            }

            pub fn name(self) -> &'static str {
                match self { $(ChatpadKey::$name => stringify!($name)),* }
            }

            pub fn from_name(name: &str) -> Option<Self> {
                match name {
                    $(stringify!($name) => Some(ChatpadKey::$name),)*
                    _ => None,
                }
            }
        }
    };
}

chatpad_keys! {
    D1 = 0x17, D2 = 0x16, D3 = 0x15, D4 = 0x14, D5 = 0x13,
    D6 = 0x12, D7 = 0x11, D8 = 0x67, D9 = 0x66, D0 = 0x65,
    Q = 0x27, W = 0x26, E = 0x25, R = 0x24, T = 0x23,
    Y = 0x22, U = 0x21, I = 0x76, O = 0x75, P = 0x64,
    A = 0x37, S = 0x36, D = 0x35, F = 0x34, G = 0x33,
    H = 0x32, J = 0x31, K = 0x77, L = 0x72, Comma = 0x62,
    Z = 0x46, X = 0x45, C = 0x44, V = 0x43, B = 0x42,
    N = 0x41, M = 0x52, Period = 0x53, Enter = 0x63,
    Left = 0x55, Space = 0x54, Right = 0x51, Backspace = 0x71,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace()
            .map(|b| u8::from_str_radix(b, 16).unwrap())
            .collect()
    }

    // Packets below are copied from docs/captures/chatpad-keys.log.

    #[test]
    fn presence() {
        assert_eq!(parse(&hex("08 80")), InPacket::Presence { connected: true });
        assert_eq!(
            parse(&hex("08 00")),
            InPacket::Presence { connected: false }
        );
    }

    #[test]
    fn gamepad_idle_with_stick_drift() {
        let p = parse(&hex(
            "00 01 00 f0 00 13 00 00 00 00 41 fa 72 ea 5a 07 3b 1a 00 00 00 00 00 00 00 00 00 00 00",
        ));
        assert_eq!(
            p,
            InPacket::Gamepad(GamepadState {
                buttons: 0,
                lt: 0,
                rt: 0,
                lx: -1471,
                ly: -5518,
                rx: 1882,
                ry: 6715
            })
        );
    }

    #[test]
    fn chatpad_status() {
        let wake = hex(
            "00 02 00 f0 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 f0 03 00 0d 0d",
        );
        let active = hex(
            "00 02 00 f0 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 f0 04 00 0e 0e",
        );
        assert_eq!(parse(&wake), InPacket::Chatpad(ChatpadEvent::WakeRequest));
        assert_eq!(parse(&active), InPacket::Chatpad(ChatpadEvent::Active));
    }

    #[test]
    fn chatpad_keys() {
        let shift_a = hex(
            "00 02 00 f0 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 01 37 00 00",
        );
        assert_eq!(
            parse(&shift_a),
            InPacket::Chatpad(ChatpadEvent::Keys(ChatpadKeys {
                modifiers: modifier::SHIFT,
                keys: [0x37, 0]
            }))
        );
        let q_w = hex(
            "00 02 00 f0 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 26 27 00",
        );
        assert_eq!(
            parse(&q_w),
            InPacket::Chatpad(ChatpadEvent::Keys(ChatpadKeys {
                modifiers: 0,
                keys: [0x26, 0x27]
            }))
        );
    }

    #[test]
    fn verified_key_codes() {
        for (code, key) in [
            (0x27, ChatpadKey::Q),
            (0x26, ChatpadKey::W),
            (0x25, ChatpadKey::E),
            (0x17, ChatpadKey::D1),
            (0x37, ChatpadKey::A),
            (0x63, ChatpadKey::Enter),
            (0x71, ChatpadKey::Backspace),
            (0x54, ChatpadKey::Space),
        ] {
            assert_eq!(ChatpadKey::from_code(code), Some(key));
        }
    }

    #[test]
    fn key_table_roundtrips() {
        for &k in ChatpadKey::ALL {
            assert_eq!(ChatpadKey::from_code(k.code()), Some(k));
            assert_eq!(ChatpadKey::from_name(k.name()), Some(k));
        }
        assert_eq!(ChatpadKey::from_code(0), None);
    }
}
