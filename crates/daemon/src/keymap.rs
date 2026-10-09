//! What each chatpad key types. Layers are defined by the characters printed on the chatpad
//! and turned into key presses through the user's keyboard layout (see `layout.rs`).
//! Green and Orange select alternate layers; a key with no entry in the active layer falls
//! back to its normal-layer output.

use std::collections::HashMap;

use evdev::KeyCode;
use x360chatpad_proto::ChatpadKey;

use crate::layout::{Combo, Layout};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    Normal,
    Green,
    Orange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Output {
    /// Type this character, whatever keys that takes in the current layout.
    Char(char),
    /// Press this key as-is (Enter, arrows, …).
    Key(KeyCode),
}

pub struct Keymap {
    layers: HashMap<(Layer, ChatpadKey), Output>,
    layout: Layout,
    pub people: KeyCode,
}

impl Keymap {
    pub fn new(layout: Layout) -> Self {
        let mut layers = HashMap::new();
        for (layer, entries) in [
            (Layer::Normal, NORMAL),
            (Layer::Green, GREEN),
            (Layer::Orange, ORANGE),
        ] {
            for &(key, out) in entries {
                layers.insert((layer, key), out);
            }
        }
        Keymap {
            layers,
            layout,
            people: KeyCode::KEY_LEFTMETA,
        }
    }

    pub fn output(&self, layer: Layer, key: ChatpadKey) -> Option<Output> {
        self.layers
            .get(&(layer, key))
            .or_else(|| self.layers.get(&(Layer::Normal, key)))
            .copied()
    }

    /// Key presses for `key` on `layer`. `shift` adds Shift on the normal layer (so letters
    /// become capitals); alternate-layer characters are typed exactly as printed.
    pub fn combo(&self, layer: Layer, key: ChatpadKey, shift: bool) -> Option<Combo> {
        let mut combo = match self.output(layer, key)? {
            Output::Char(c) => match self.layout.resolve(c) {
                Some(combo) => combo.clone(),
                None => {
                    eprintln!(
                        "{c:?} can't be typed with keyboard layout {}",
                        self.layout.name
                    );
                    return None;
                }
            },
            Output::Key(k) => Combo::key(k),
        };
        let uses_layer = layer != Layer::Normal && self.layers.contains_key(&(layer, key));
        if shift && !uses_layer && !combo.hold.contains(&KeyCode::KEY_LEFTSHIFT) {
            combo.hold.insert(0, KeyCode::KEY_LEFTSHIFT);
        }
        Some(combo)
    }
}

use ChatpadKey as C;
use Output::{Char as Ch, Key};

const NORMAL: &[(ChatpadKey, Output)] = &[
    (C::D1, Ch('1')),
    (C::D2, Ch('2')),
    (C::D3, Ch('3')),
    (C::D4, Ch('4')),
    (C::D5, Ch('5')),
    (C::D6, Ch('6')),
    (C::D7, Ch('7')),
    (C::D8, Ch('8')),
    (C::D9, Ch('9')),
    (C::D0, Ch('0')),
    (C::Q, Ch('q')),
    (C::W, Ch('w')),
    (C::E, Ch('e')),
    (C::R, Ch('r')),
    (C::T, Ch('t')),
    (C::Y, Ch('y')),
    (C::U, Ch('u')),
    (C::I, Ch('i')),
    (C::O, Ch('o')),
    (C::P, Ch('p')),
    (C::A, Ch('a')),
    (C::S, Ch('s')),
    (C::D, Ch('d')),
    (C::F, Ch('f')),
    (C::G, Ch('g')),
    (C::H, Ch('h')),
    (C::J, Ch('j')),
    (C::K, Ch('k')),
    (C::L, Ch('l')),
    (C::Comma, Ch(',')),
    (C::Z, Ch('z')),
    (C::X, Ch('x')),
    (C::C, Ch('c')),
    (C::V, Ch('v')),
    (C::B, Ch('b')),
    (C::N, Ch('n')),
    (C::M, Ch('m')),
    (C::Period, Ch('.')),
    (C::Enter, Key(KeyCode::KEY_ENTER)),
    (C::Backspace, Key(KeyCode::KEY_BACKSPACE)),
    (C::Space, Key(KeyCode::KEY_SPACE)),
    (C::Left, Key(KeyCode::KEY_LEFT)),
    (C::Right, Key(KeyCode::KEY_RIGHT)),
];

// Legends as printed on a US/English 360 chatpad.
const GREEN: &[(ChatpadKey, Output)] = &[
    (C::Q, Ch('!')),
    (C::W, Ch('@')),
    (C::E, Ch('€')),
    (C::R, Ch('#')),
    (C::T, Ch('%')),
    (C::Y, Ch('^')),
    (C::U, Ch('&')),
    (C::I, Ch('*')),
    (C::O, Ch('(')),
    (C::P, Ch(')')),
    (C::A, Ch('~')),
    (C::S, Ch('š')),
    (C::D, Ch('{')),
    (C::F, Ch('}')),
    (C::G, Ch('¨')),
    (C::H, Ch('/')),
    (C::J, Ch('\'')),
    (C::K, Ch('[')),
    (C::L, Ch(']')),
    (C::Comma, Ch(':')),
    (C::Z, Ch('`')),
    (C::X, Ch('«')),
    (C::C, Ch('»')),
    (C::V, Ch('-')),
    (C::B, Ch('|')),
    (C::N, Ch('<')),
    (C::M, Ch('>')),
    (C::Period, Ch('?')),
];

const ORANGE: &[(ChatpadKey, Output)] = &[
    (C::Q, Ch('¡')),
    (C::W, Ch('å')),
    (C::E, Ch('è')),
    (C::R, Ch('$')),
    (C::T, Ch('þ')),
    (C::Y, Ch('ý')),
    (C::U, Ch('ú')),
    (C::I, Ch('ì')),
    (C::O, Ch('ò')),
    (C::P, Ch('=')),
    (C::A, Ch('á')),
    (C::S, Ch('ß')),
    (C::D, Ch('ð')),
    (C::F, Ch('£')),
    (C::G, Ch('¥')),
    (C::H, Ch('\\')),
    (C::J, Ch('"')),
    (C::L, Ch('ø')),
    (C::Comma, Ch(';')),
    (C::Z, Ch('æ')),
    (C::X, Ch('œ')),
    (C::C, Ch('ç')),
    (C::V, Ch('_')),
    (C::B, Ch('+')),
    (C::N, Ch('ñ')),
    (C::M, Ch('µ')),
    (C::Period, Ch('¿')),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_types_something_on_us() {
        let map = Keymap::new(Layout::us());
        for &k in ChatpadKey::ALL {
            assert!(map.combo(Layer::Normal, k, false).is_some(), "{k:?}");
        }
    }

    #[test]
    fn shift_capitalises_but_not_layer_symbols() {
        let map = Keymap::new(Layout::us());
        let shifted_a = map.combo(Layer::Normal, ChatpadKey::A, true).unwrap();
        assert_eq!(shifted_a.hold, vec![KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_A]);
        // Green+H is '/', which needs no shift; chatpad Shift must not turn it into '?'.
        let slash = map.combo(Layer::Green, ChatpadKey::H, true).unwrap();
        assert_eq!(slash.hold, vec![KeyCode::KEY_SLASH]);
    }

    #[test]
    fn unmapped_layer_key_falls_back_to_normal() {
        let map = Keymap::new(Layout::us());
        assert_eq!(
            map.output(Layer::Green, ChatpadKey::Space),
            Some(Key(KeyCode::KEY_SPACE))
        );
    }
}
