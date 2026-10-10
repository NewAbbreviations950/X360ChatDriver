# Roadmap

Status and planned work. The original design plan (Rust daemon + tray app, installer
script, no Flatpak) is summarised under "Architecture decisions" at the end.

## Done
- Protocol capture and notes (`docs/protocol.md`), including the chatpad wake handshake.
- Daemon: all 4 slots → virtual Xbox 360 pads (buttons, axes, rumble, player LED) plus a
  shared chatpad keyboard. Receiver is handed back to `xpad` on exit.
- Layout-aware typing via libxkbcommon (dlopen'd, with a US fallback). Layout comes from
  KDE/GNOME/localed or `config.toml`.
- One-shot Shift/Green/Orange, Shift+Orange = Caps Lock, Green+People = Steam Quick Access
  (Guide+A), People = Meta.
- `install.sh` (release download / `--local` / `--uninstall`), systemd user unit, udev
  rule, and CI + release workflows.
- People opens Steam friends & chat (`steam://open/friends`).
- Quick Access sends Guide, then A 80 ms later, so Steam doesn't also register a stray A.
- Caps light synced with the system Caps Lock via an `LED_CAPSL` on the virtual keyboard.
- Tray app and settings popup (`x360chatpad-tray`):
  - **Tray icon:** a StatusNotifierItem via `ksni`, sent as pixels rendered from our SVG
    in the panel's text colour. It shows status, connected players and the layout.
  - **Settings popup:** a borderless `egui` window (`--settings`) that closes when it loses
    focus. On KDE Wayland a temporary KWin script places it above the tray icon; on X11 it
    places itself.
  - **Look:** colours come from the KDE colour scheme or the desktop portal, and the font
    is the system UI font.
  - **Keymap editor:** keycaps show all three layers. Keys are selected by clicking or by
    pressing them on the chatpad, and changes save and apply instantly over D-Bus.
- **Daemon D-Bus service** `io.github.NewAbbreviations950.X360Chatpad`: `Reload`, `Status`,
  and a `ChatpadKey` signal. `crates/common` holds the shared config and keymap.
- **Key remapping** in `config.toml` under `[keys.normal|green|orange]`.
- **Layout changes at runtime:** "Reload" in the tray, or saving in the popup, re-detects
  the layout.
- **Icons:** a 360-style controller icon in full colour for the app menu and in monochrome
  for the tray (`packaging/icons`).
- Chatpad indicator lights:
  - Caps shows Caps Lock or a latched Shift; Green and Orange show latched or held.
  - Re-synced from the status byte whenever the chatpad wakes (see `docs/protocol.md`).


## Next
- **Publish:** create the GitHub repo `NewAbbreviations950/X360ChatDriver`, push, and tag
  `v0.1.0` so the one-line installer works.
- **Test on Steam Deck** (desktop + game mode) and **Kubuntu**:
  - Steam sees exactly one pad per controller.
  - Chatpad types in game mode.
  - Check the layout in game mode: gamescope uses US unless configured.
  - Check that the udev rule in `/etc` survives a SteamOS update.
- **Test the tray and popup on other desktops:** GNOME, both with and without the
  AppIndicator extension (without it the popup should open from the app menu), plus
  Kubuntu, Deck desktop mode and X11. Check that the popup is placed next to the tray.
- **Release build size:** the tray binary is about 12 MB, and could be trimmed if that matters.

## Later
- **Steam controller list at startup:** if the pad already exists when Steam starts, Steam
  uses it but may not list it in Settings → Controller. It does list it once it's hotplugged
  while Steam runs. Check this again, especially when booting into Deck game mode.
- **Use for the People light:** it's currently always off.
- **Battery status:** not identified in the protocol yet. Capture with `x360chatpad-dump`
  while the battery drains, or compare different battery levels. Show it in the tray and
  the popup header, and notify when low. Add it to the D-Bus `Status`.
- **Automatic layout change detection:** right now it needs a manual Reload from the tray.
  The daemon could watch `kxkbrc`/gsettings instead.
- **Headset audio:** the odd-numbered interfaces (protocol 0x82, interrupt endpoints) carry
  the headset. The format is undocumented. Research it, then expose a PipeWire sink and
  source.
- **Unknown packets:** `00 f8 0N` (seen while the chatpad wasn't handshaked) and the
  `00 0f` link-info packet.
- **Non-ASCII chatpad legends** (€, š, ¨, «, »): these work only if the layout can type
  them. A compose/Unicode fallback could cover the rest.
- **Packaging extras:** aarch64 builds, AUR/OBS packages, maybe a Flatpak for the GUI only.
- **Power off controller** from the tray or popup (`00 00 08 c0`); needs a D-Bus method.

## Architecture decisions (from the initial plan)
- Rust. The daemon runs as a systemd **user** service, so it works in Steam Deck game
  mode. `nusb` handles USB (pure Rust, no libusb) and the `evdev` crate handles uinput.
- No Flatpak for the daemon: it needs a host udev rule and must autostart outside a
  sandbox.
- The only root step is the udev rule (`TAG+="uaccess"`). `/dev/uinput` access comes from
  Steam's own udev rules on most systems.
- Release builds run on ubuntu-22.04 to keep the glibc requirement low. The daemon isn't
  fully static because libxkbcommon is loaded at runtime.
