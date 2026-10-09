#!/usr/bin/env bash
# Install, update or remove x360chatpad for the current user.
#
#   curl -fsSL https://raw.githubusercontent.com/NewAbbreviations950/X360ChatDriver/main/install.sh | bash
#   ... | bash -s -- --uninstall
#   ./install.sh --local        # from a checkout: build and install the working tree
#
# Everything goes in your home directory except one udev rule (needs sudo once), which lets
# your user access the Xbox 360 wireless receiver.
set -euo pipefail

REPO="${X360CHATPAD_REPO:-NewAbbreviations950/X360ChatDriver}"
ASSET="x360chatpad-x86_64-linux.tar.gz"

BIN_DIR="$HOME/.local/bin"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNIT="x360chatpad.service"
APPS_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
ICON_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
RULE_DEST="/etc/udev/rules.d/70-x360chatpad.rules"

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

is_steamos() { grep -qs '^ID=steamos' /etc/os-release; }

# Run a command as root, explaining why first.
as_root() {
    if [ "$(id -u)" -eq 0 ]; then "$@"; return; fi
    command -v sudo >/dev/null || die "sudo is needed to install the udev rule"
    if ! sudo -n true 2>/dev/null; then
        say "sudo is needed once to install the receiver's udev rule (/etc/udev/rules.d)."
        if is_steamos; then
            say "On Steam Deck: if you've never set a password, open Konsole and run 'passwd' first."
        fi
    fi
    sudo "$@"
}

reload_udev() {
    as_root udevadm control --reload
    # Re-apply to already plugged-in receivers so no replug is needed.
    as_root udevadm trigger --subsystem-match=usb --attr-match=idVendor=045e
}

install_files() {
    local src="$1"   # directory containing bin/ and share/
    say "Installing binaries to $BIN_DIR"
    install -Dm755 "$src/bin/x360chatpad-daemon" "$BIN_DIR/x360chatpad-daemon"
    install -Dm755 "$src/bin/x360chatpad-tray" "$BIN_DIR/x360chatpad-tray"

    say "Adding \"X360 Chatpad Settings\" to the app menu and the tray icon to autostart"
    install_desktop "$src/share/x360chatpad-settings.desktop" "$APPS_DIR/x360chatpad-settings.desktop"
    install_desktop "$src/share/x360chatpad-tray.desktop" "$AUTOSTART_DIR/x360chatpad-tray.desktop"
    for icon in "$src"/share/icons/*.svg; do
        install -Dm644 "$icon" "$ICON_DIR/$(basename "$icon")"
    done
    touch "${ICON_DIR%/scalable/apps}" 2>/dev/null || true   # nudge icon caches

    say "Installing systemd user service"
    install -Dm644 "$src/share/$UNIT" "$UNIT_DIR/$UNIT"
    systemctl --user daemon-reload

    if [ -f "$RULE_DEST" ] && cmp -s "$src/share/70-x360chatpad.rules" "$RULE_DEST"; then
        say "udev rule already up to date"
    else
        say "Installing udev rule to $RULE_DEST"
        as_root install -Dm644 "$src/share/70-x360chatpad.rules" "$RULE_DEST"
        reload_udev
    fi

    systemctl --user enable "$UNIT" >/dev/null
    # restart (not start) so updates take effect immediately.
    systemctl --user restart "$UNIT"
    start_tray
    say "Done. x360chatpad is running and will start automatically when you log in."
    say "Logs: journalctl --user -u $UNIT -f"
}

# Install a .desktop file with the binary directory filled in (launchers don't see ~/.local/bin
# on every desktop).
install_desktop() {
    mkdir -p "$(dirname "$2")"
    sed "s|@BIN_DIR@|$BIN_DIR|g" "$1" > "$2"
    chmod 644 "$2"
}

has_tray_host() {
    command -v busctl >/dev/null && busctl --user status org.kde.StatusNotifierWatcher >/dev/null 2>&1
}

# (Re)start the tray icon now if we're in a graphical session, so updates show immediately.
start_tray() {
    pkill -x x360chatpad-tra 2>/dev/null || true
    [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ] || return 0
    if has_tray_host; then
        setsid "$BIN_DIR/x360chatpad-tray" >/dev/null 2>&1 < /dev/null &
    elif [[ "${XDG_CURRENT_DESKTOP:-}" == *GNOME* ]]; then
        say "GNOME has no tray by default. Open \"X360 Chatpad Settings\" from the app menu, or install"
        say "the \"AppIndicator and KStatusNotifierItem Support\" extension for a tray icon."
    fi
}

stage_local() {
    local root
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    [ -f "$root/Cargo.toml" ] || die "--local must be run from a checkout of the repository"
    # rustup's default location, in case this shell's PATH doesn't include it yet.
    [ -x "$HOME/.cargo/bin/cargo" ] && PATH="$HOME/.cargo/bin:$PATH"
    command -v cargo >/dev/null || die "cargo not found; install Rust from https://rustup.rs"
    say "Building release binaries"
    (cd "$root" && cargo build --release --locked -p x360chatpad-daemon -p x360chatpad-tray)
    STAGE="$(mktemp -d)"
    mkdir -p "$STAGE/bin" "$STAGE/share"
    cp "$root/target/release/x360chatpad-daemon" "$root/target/release/x360chatpad-tray" "$STAGE/bin/"
    cp -r "$root"/packaging/* "$STAGE/share/"
}

stage_release() {
    command -v curl >/dev/null || die "curl is required"
    [ "$(uname -m)" = "x86_64" ] || die "only x86_64 builds are published so far"
    local base="https://github.com/$REPO/releases/latest/download"
    STAGE="$(mktemp -d)"
    say "Downloading latest release from github.com/$REPO"
    curl -fsSL "$base/$ASSET" -o "$STAGE/$ASSET"
    curl -fsSL "$base/SHA256SUMS" -o "$STAGE/SHA256SUMS"
    (cd "$STAGE" && grep " $ASSET\$" SHA256SUMS | sha256sum -c --quiet -) || die "checksum mismatch; aborting"
    tar -xzf "$STAGE/$ASSET" -C "$STAGE" --strip-components=1
}

uninstall() {
    say "Stopping and removing x360chatpad"
    systemctl --user disable --now "$UNIT" 2>/dev/null || true
    pkill -x x360chatpad-tra 2>/dev/null || true
    rm -f "$UNIT_DIR/$UNIT" "$BIN_DIR/x360chatpad-daemon" "$BIN_DIR/x360chatpad-tray" \
        "$APPS_DIR/x360chatpad-settings.desktop" "$AUTOSTART_DIR/x360chatpad-tray.desktop" \
        "$ICON_DIR/x360chatpad.svg" "$ICON_DIR/x360chatpad-tray-symbolic.svg"
    systemctl --user daemon-reload
    if [ -f "$RULE_DEST" ]; then
        as_root rm -f "$RULE_DEST"
        reload_udev
    fi
    say "Removed. Your settings in ~/.config/x360chatpad were kept."
}

main() {
    [ "$(id -u)" -ne 0 ] || die "run this as your normal user, not root (it asks for sudo when needed)"
    STAGE=""
    trap '[ -z "$STAGE" ] || rm -rf "$STAGE"' EXIT
    case "${1:-}" in
        --uninstall) uninstall; return ;;
        --local) stage_local ;;
        "") stage_release ;;
        *) die "unknown option: $1 (expected --local or --uninstall)" ;;
    esac
    install_files "$STAGE"
}

main "$@"
