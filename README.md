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

## Configuration

`~/.config/x360chatpad/config.toml` (optional):

```toml
# Keyboard layout to type for. Detected from KDE, GNOME or the system default if unset.
# Steam Deck game mode uses a US layout regardless of desktop settings, so set this there
# if your desktop layout differs.
layout = "no"
variant = ""
```

Restart after changing it: `systemctl --user restart x360chatpad`.

## Troubleshooting

- Logs: `journalctl --user -u x360chatpad -f`
- "open receiver" errors: the udev rule is missing. Rerun the installer, or replug the receiver.
- Stopping the service (`systemctl --user stop x360chatpad`) hands the receiver back to
  `xpad`, so the controller keeps working without the chatpad.

## Development

- `cargo test --workspace` runs packet-parsing, keymap and layout tests.
- `x360chatpad-dump` (built with the daemon) hex-dumps receiver traffic and sends packets
  typed on stdin. It's useful for protocol work; stop the service first.
- Protocol notes: [docs/protocol.md](docs/protocol.md)
