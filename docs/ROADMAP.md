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
- **Tray app + remap GUI** (milestone 5 of the plan):
  - Separate `x360chatpad-tray` binary using `ksni` (StatusNotifierItem) and `eframe/egui`.
  - Falls back to a normal window when no tray exists (plain GNOME without the
    AppIndicator extension).
  - Talks to the daemon over D-Bus (`zbus`): status, reload config, live key events.
  - Remapping is saved to `config.toml`, which needs a keymap section in the config schema.
  - Add it to `install.sh`, plus autostart `.desktop` files and an icon.

## Later
- **Steam controller list at startup:** if the pad already exists when Steam starts, Steam
  uses it but may not list it in Settings → Controller. It does list it once it's hotplugged
  while Steam runs. Check this again, especially when booting into Deck game mode.
- **Use for the People light:** it's currently always off.
- **Battery status:** not identified in the protocol yet. Capture with `x360chatpad-dump`
  while the battery drains, or compare different battery levels. Show it in the tray, and
  notify when low.
- **Layout changes at runtime:** re-detect when the desktop layout changes. Options: watch
  `kxkbrc`/gsettings, or have the tray send a D-Bus reload. Right now the layout is only
  read at startup.
- **Headset audio:** the odd-numbered interfaces (protocol 0x82, interrupt endpoints) carry
  the headset. The format is undocumented. Research it, then expose a PipeWire sink and
  source.
- **Unknown packets:** `00 f8 0N` (seen while the chatpad wasn't handshaked) and the
  `00 0f` link-info packet.
- **Non-ASCII chatpad legends** (€, š, ¨, «, »): these work only if the layout can type
  them. A compose/Unicode fallback could cover the rest.
- **Packaging extras:** aarch64 builds, AUR/OBS packages, maybe a Flatpak for the GUI only.
- **Power off controller** from the tray (`00 00 08 c0`).

## Architecture decisions (from the initial plan)
- Rust. The daemon runs as a systemd **user** service, so it works in Steam Deck game
  mode. `nusb` handles USB (pure Rust, no libusb) and the `evdev` crate handles uinput.
- No Flatpak for the daemon: it needs a host udev rule and must autostart outside a
  sandbox.
- The only root step is the udev rule (`TAG+="uaccess"`). `/dev/uinput` access comes from
  Steam's own udev rules on most systems.
- Release builds run on ubuntu-22.04 to keep the glibc requirement low. The daemon isn't
  fully static because libxkbcommon is loaded at runtime.
