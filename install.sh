#!/usr/bin/env bash
# Install, update or remove x360chatpad for the current user.
#
#   curl -fsSL https://raw.githubusercontent.com/OWNER/X360ChatDriver/main/install.sh | bash
#   ... | bash -s -- --uninstall
#   ./install.sh --local        # from a checkout: build and install the working tree
#
# Everything goes in your home directory except one udev rule (needs sudo once), which lets
# your user access the Xbox 360 wireless receiver.
set -euo pipefail

REPO="${X360CHATPAD_REPO:-OWNER/X360ChatDriver}"
ASSET="x360chatpad-x86_64-linux.tar.gz"

BIN_DIR="$HOME/.local/bin"
UNIT_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
UNIT="x360chatpad.service"
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
    say "Done. x360chatpad is running and will start automatically when you log in."
    say "Logs: journalctl --user -u $UNIT -f"
}

stage_local() {
    local root
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    [ -f "$root/Cargo.toml" ] || die "--local must be run from a checkout of the repository"
    # rustup's default location, in case this shell's PATH doesn't include it yet.
    [ -x "$HOME/.cargo/bin/cargo" ] && PATH="$HOME/.cargo/bin:$PATH"
    command -v cargo >/dev/null || die "cargo not found; install Rust from https://rustup.rs"
    say "Building release binaries"
    (cd "$root" && cargo build --release --locked -p x360chatpad-daemon)
    STAGE="$(mktemp -d)"
    mkdir -p "$STAGE/bin" "$STAGE/share"
    cp "$root/target/release/x360chatpad-daemon" "$STAGE/bin/"
    cp "$root/packaging/$UNIT" "$root/packaging/70-x360chatpad.rules" "$STAGE/share/"
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
    rm -f "$UNIT_DIR/$UNIT" "$BIN_DIR/x360chatpad-daemon"
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
