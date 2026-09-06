#!/usr/bin/env bash
# Builds SSCL and installs it for the current user.
#
#   ./install.sh            build in release mode and install
#   ./install.sh --uninstall remove what was installed
#
# Everything lands under ~/.local, so no root is required.

set -euo pipefail

BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/scalable/apps"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -f "${BIN_DIR}/sscl" "${BIN_DIR}/sscl-probe" \
          "${APP_DIR}/sscl.desktop" "${ICON_DIR}/sscl.svg"
    command -v update-desktop-database >/dev/null && \
        update-desktop-database "${APP_DIR}" 2>/dev/null || true
    echo "SSCL removed. Your connections in ~/.local/share/sscl were left alone."
    exit 0
fi

echo "==> Checking build dependencies"
missing=()
command -v cargo >/dev/null || missing+=("cargo (https://rustup.rs)")
pkg-config --exists xkbcommon 2>/dev/null || missing+=("libxkbcommon-dev")
pkg-config --exists gl 2>/dev/null || missing+=("libgl1-mesa-dev")
if ((${#missing[@]})); then
    echo "Missing: ${missing[*]}" >&2
    echo "On Debian/Ubuntu/Pop!_OS:" >&2
    echo "  sudo apt install build-essential pkg-config libxkbcommon-dev \\" >&2
    echo "       libwayland-dev libgl1-mesa-dev libx11-dev libxcursor-dev \\" >&2
    echo "       libxrandr-dev libxi-dev openssh-client" >&2
    exit 1
fi
command -v ssh >/dev/null || echo "warning: ssh not found — install openssh-client" >&2

echo "==> Building (release)"
cargo build --release --manifest-path "${HERE}/Cargo.toml"

echo "==> Installing"
install -Dm755 "${HERE}/target/release/sscl"       "${BIN_DIR}/sscl"
install -Dm755 "${HERE}/target/release/sscl-probe" "${BIN_DIR}/sscl-probe"
install -Dm644 "${HERE}/assets/sscl.desktop"       "${APP_DIR}/sscl.desktop"
install -Dm644 "${HERE}/assets/sscl.svg"           "${ICON_DIR}/sscl.svg"

command -v update-desktop-database >/dev/null && \
    update-desktop-database "${APP_DIR}" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && \
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true

echo
echo "Installed:"
echo "  ${BIN_DIR}/sscl"
echo "  ${BIN_DIR}/sscl-probe"
echo "  ${APP_DIR}/sscl.desktop"
case ":${PATH}:" in
    *":${BIN_DIR}:"*) ;;
    *) echo; echo "note: ${BIN_DIR} is not on your PATH." ;;
esac
