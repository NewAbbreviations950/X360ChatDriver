//! User settings from `~/.config/x360chatpad/config.toml`. Every field is optional:
//!
//! ```toml
//! # Keyboard layout the chatpad should type for. Detected from the desktop if unset.
//! # Useful on Steam Deck game mode, which uses its own (US) layout.
//! layout = "no"
//! variant = ""
//! ```

use std::path::PathBuf;

use serde::Deserialize;

use crate::layout::{LayoutName, dirs_config};

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub layout: Option<String>,
    pub variant: Option<String>,
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        Some(dirs_config()?.join("x360chatpad/config.toml"))
    }

    /// Load the config, or the defaults if it's missing or broken (with a warning).
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(e) => {
                eprintln!("reading {}: {e}; using defaults", path.display());
                return Self::default();
            }
        };
        toml::from_str(&text).unwrap_or_else(|e| {
            eprintln!("{}: {e}; using defaults", path.display());
            Self::default()
        })
    }

    pub fn layout_name(&self) -> Option<LayoutName> {
        let layout = self.layout.clone().filter(|l| !l.is_empty())?;
        Some(LayoutName {
            layout,
            variant: self.variant.clone().unwrap_or_default(),
        })
    }
}
