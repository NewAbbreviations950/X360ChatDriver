//! D-Bus names shared by the daemon and the tray/settings app (session bus).
//!
//! Methods: `Reload()` re-reads config.toml and re-detects the keyboard layout;
//! `Status() -> (layout: s, connected: ab)` gives the active layout and which of the four
//! controller slots are connected.
//! Signal: `ChatpadKey(slot: y, key: s)` when a chatpad key is pressed (`ChatpadKey` name).

pub const BUS_NAME: &str = "io.github.NewAbbreviations950.X360Chatpad";
pub const OBJECT_PATH: &str = "/io/github/NewAbbreviations950/X360Chatpad";
pub const INTERFACE: &str = "io.github.NewAbbreviations950.X360Chatpad1";
