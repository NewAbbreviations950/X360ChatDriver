# X360 Chatpad Driver

Use an Xbox 360 wireless controller **with its chatpad** on Linux through the official
Xbox 360 Wireless Receiver. It works on any desktop distro and on the Steam Deck, and it
plays nicely with Steam Input.

The kernel's `xpad` driver supports the controller but ignores the chatpad. This driver
takes the receiver over from `xpad` (and gives it back when stopped). It exposes:

- **one virtual Xbox 360 pad per controller**, with buttons, sticks, triggers, rumble and
  the player LED. Steam and games see a normal 360 controller.
- **one virtual keyboard** that all chatpads type into, following your keyboard layout.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/NewAbbreviations950/X360ChatDriver/main/install.sh | bash
```

This installs to `~/.local/bin` and runs as a systemd user service that starts when you log
in, including Steam Deck game mode. The installer asks for `sudo` **once**, to add a udev
rule that gives your user access to the receiver. On Steam Deck, set a password first with
`passwd` in Konsole if you've never done so.

To update, run the same command again. To remove:

```sh
curl -fsSL https://raw.githubusercontent.com/NewAbbreviations950/X360ChatDriver/main/install.sh | bash -s -- --uninstall
```

From a checkout, `./install.sh --local` builds and installs the working tree (needs Rust).

## Using the chatpad

| Keys | Result |
|---|---|
| Shift, Green, Orange | one-shot: tap it, then press a key (holding it works too); tap again to cancel |
| Green / Orange + key | the green / orange character printed on the key |
| Shift + Orange | Caps Lock on/off |
| People | open Steam friends & chat |
| Green + People | Steam Quick Access menu (Guide + A) |

Characters are typed through your desktop's keyboard layout, so symbols come out right
on non-US layouts too. Characters your layout can't type are skipped.

## Tray icon and settings

A controller icon in the system tray shows the driver status and the connected players.
Clicking it opens a small settings popup:
- See which players are connected.
- Remap any chatpad key on each layer: click a key, or press it on the chatpad, then type
  a character or pick a special key. Changes apply immediately.
- Choose the keyboard layout: automatic, or a fixed one.

The popup is also in your app menu as **X360 Chatpad Settings**. That's the way in on GNOME
unless you install the "AppIndicator and KStatusNotifierItem Support" extension.

## Configuration

`~/.config/x360chatpad/config.toml` (optional):

```toml
# Keyboard layout to type for. Detected from KDE, GNOME or the system default if unset.
# Steam Deck game mode uses a US layout regardless of desktop settings, so set this there
# if your desktop layout differs.
layout = "no"
variant = ""

# Remapped keys per layer (normal, green, orange). Use a single character, an evdev key
# name such as "KEY_F5", or "" to disable the key. Key names: D1-D0, Q-P, A-L, Comma,
# Z-M, Period, Enter, Left, Space, Right, Backspace.
[keys.green]
E = "KEY_F5"
```

The settings popup edits this file for you. After editing it by hand, choose "Reload" in
the tray menu, or run `systemctl --user restart x360chatpad`.

## Troubleshooting

- Logs: `journalctl --user -u x360chatpad -f`
- "open receiver" errors: the udev rule is missing. Rerun the installer, or replug the receiver.
- Stopping the service (`systemctl --user stop x360chatpad`) hands the receiver back to
  `xpad`, so the controller keeps working without the chatpad.

## Development

- `cargo test --workspace` runs packet-parsing, keymap and layout tests.
- Crates: `proto` (USB packets), `common` (config, keymap, D-Bus names), `daemon`, `tray`.
- `x360chatpad-dump` (built with the daemon) hex-dumps receiver traffic and sends packets
  typed on stdin. It's useful for protocol work; stop the service first.
- Protocol notes: [docs/protocol.md](docs/protocol.md). Status and plans: [docs/ROADMAP.md](docs/ROADMAP.md)
