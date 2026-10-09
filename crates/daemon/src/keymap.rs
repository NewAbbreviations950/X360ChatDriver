//! What each chatpad key types. Green and Orange select alternate layers; a key with no
//! entry in the active layer falls back to its normal-layer output.
//!
//! Layer contents follow the legends printed on a US chatpad, limited to characters a US
//! keyboard layout can produce. Non-ASCII legends (€, ñ, …) are left unmapped for now.

use std::collections::HashMap;

use evdev::KeyCode;
use x360chatpad_proto::ChatpadKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    Normal,
    Green,
    Orange,
}

/// Keys pressed together (in order) for one chatpad key, released in reverse.
pub type Combo = Vec<KeyCode>;

pub struct Keymap {
    layers: HashMap<(Layer, ChatpadKey), Combo>,
    pub shift: KeyCode,
    pub people: KeyCode,
}

impl Keymap {
    pub fn lookup(&self, layer: Layer, key: ChatpadKey) -> Option<&Combo> {
        self.layers.get(&(layer, key)).or_else(|| self.layers.get(&(Layer::Normal, key)))
    }

    /// Every key code this map can emit, for declaring the virtual keyboard's capabilities.
    pub fn all_codes(&self) -> impl Iterator<Item = KeyCode> + '_ {
        self.layers.values().flatten().copied().chain([self.shift, self.people])
    }
}

impl Default for Keymap {
    fn default() -> Self {
        use ChatpadKey as C;
        use KeyCode as K;
        const SHIFT: K = K::KEY_LEFTSHIFT;

        let normal: &[(C, &[K])] = &[
            (C::D1, &[K::KEY_1]), (C::D2, &[K::KEY_2]), (C::D3, &[K::KEY_3]), (C::D4, &[K::KEY_4]),
            (C::D5, &[K::KEY_5]), (C::D6, &[K::KEY_6]), (C::D7, &[K::KEY_7]), (C::D8, &[K::KEY_8]),
            (C::D9, &[K::KEY_9]), (C::D0, &[K::KEY_0]),
            (C::Q, &[K::KEY_Q]), (C::W, &[K::KEY_W]), (C::E, &[K::KEY_E]), (C::R, &[K::KEY_R]),
            (C::T, &[K::KEY_T]), (C::Y, &[K::KEY_Y]), (C::U, &[K::KEY_U]), (C::I, &[K::KEY_I]),
            (C::O, &[K::KEY_O]), (C::P, &[K::KEY_P]),
            (C::A, &[K::KEY_A]), (C::S, &[K::KEY_S]), (C::D, &[K::KEY_D]), (C::F, &[K::KEY_F]),
            (C::G, &[K::KEY_G]), (C::H, &[K::KEY_H]), (C::J, &[K::KEY_J]), (C::K, &[K::KEY_K]),
            (C::L, &[K::KEY_L]), (C::Comma, &[K::KEY_COMMA]),
            (C::Z, &[K::KEY_Z]), (C::X, &[K::KEY_X]), (C::C, &[K::KEY_C]), (C::V, &[K::KEY_V]),
            (C::B, &[K::KEY_B]), (C::N, &[K::KEY_N]), (C::M, &[K::KEY_M]), (C::Period, &[K::KEY_DOT]),
            (C::Enter, &[K::KEY_ENTER]), (C::Backspace, &[K::KEY_BACKSPACE]), (C::Space, &[K::KEY_SPACE]),
            (C::Left, &[K::KEY_LEFT]), (C::Right, &[K::KEY_RIGHT]),
        ];
        let green: &[(C, &[K])] = &[
            (C::Q, &[SHIFT, K::KEY_1]),          // !
            (C::W, &[SHIFT, K::KEY_2]),          // @
            (C::R, &[SHIFT, K::KEY_3]),          // #
            (C::T, &[SHIFT, K::KEY_5]),          // %
            (C::Y, &[SHIFT, K::KEY_6]),          // ^
            (C::U, &[SHIFT, K::KEY_7]),          // &
            (C::I, &[SHIFT, K::KEY_8]),          // *
            (C::O, &[SHIFT, K::KEY_9]),          // (
            (C::P, &[SHIFT, K::KEY_0]),          // )
            (C::A, &[SHIFT, K::KEY_GRAVE]),      // ~
            (C::D, &[SHIFT, K::KEY_LEFTBRACE]),  // {
            (C::F, &[SHIFT, K::KEY_RIGHTBRACE]), // }
            (C::H, &[K::KEY_SLASH]),             // /
            (C::J, &[K::KEY_APOSTROPHE]),        // '
            (C::K, &[K::KEY_LEFTBRACE]),         // [
            (C::L, &[K::KEY_RIGHTBRACE]),        // ]
            (C::Comma, &[SHIFT, K::KEY_SEMICOLON]), // :
            (C::Z, &[K::KEY_GRAVE]),             // `
            (C::V, &[K::KEY_MINUS]),             // -
            (C::B, &[SHIFT, K::KEY_BACKSLASH]),  // |
            (C::N, &[SHIFT, K::KEY_COMMA]),      // <
            (C::M, &[SHIFT, K::KEY_DOT]),        // >
            (C::Period, &[SHIFT, K::KEY_SLASH]), // ?
        ];
        let orange: &[(C, &[K])] = &[
            (C::R, &[SHIFT, K::KEY_4]),          // $
            (C::P, &[K::KEY_EQUAL]),             // =
            (C::H, &[K::KEY_BACKSLASH]),         // \
            (C::J, &[SHIFT, K::KEY_APOSTROPHE]), // "
            (C::Comma, &[K::KEY_SEMICOLON]),     // ;
            (C::V, &[SHIFT, K::KEY_MINUS]),      // _
            (C::B, &[SHIFT, K::KEY_EQUAL]),      // +
        ];

        let mut layers = HashMap::new();
        for (layer, entries) in [(Layer::Normal, normal), (Layer::Green, green), (Layer::Orange, orange)] {
            for (key, combo) in entries {
                layers.insert((layer, *key), combo.to_vec());
            }
        }
        Keymap { layers, shift: K::KEY_LEFTSHIFT, people: K::KEY_LEFTMETA }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_a_normal_mapping() {
        let map = Keymap::default();
        for &k in ChatpadKey::ALL {
            assert!(map.lookup(Layer::Normal, k).is_some(), "{k:?} unmapped");
        }
    }

    #[test]
    fn layers_fall_back_to_normal() {
        let map = Keymap::default();
        assert_eq!(map.lookup(Layer::Green, ChatpadKey::Q), Some(&vec![KeyCode::KEY_LEFTSHIFT, KeyCode::KEY_1]));
        assert_eq!(map.lookup(Layer::Green, ChatpadKey::Space), Some(&vec![KeyCode::KEY_SPACE]));
    }
}
