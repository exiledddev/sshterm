#!/usr/bin/env bash
# Builds ACLI and installs it for the current user.
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
    rm -f "${BIN_DIR}/acli" "${APP_DIR}/acli.desktop" "${ICON_DIR}/acli.svg"
    command -v update-desktop-database >/dev/null && \
        update-desktop-database "${APP_DIR}" 2>/dev/null || true
    echo "ACLI removed. Your settings in ~/.config/acli were left alone."
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
    echo "       libxrandr-dev libxi-dev" >&2
    exit 1
fi

echo "==> Building (release)"
cargo build --release --manifest-path "${HERE}/Cargo.toml"

echo "==> Installing"
install -Dm755 "${HERE}/target/release/acli"  "${BIN_DIR}/acli"
install -Dm644 "${HERE}/assets/acli.desktop" "${APP_DIR}/acli.desktop"
install -Dm644 "${HERE}/assets/acli.svg"     "${ICON_DIR}/acli.svg"

command -v update-desktop-database >/dev/null && \
    update-desktop-database "${APP_DIR}" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && \
    gtk-update-icon-cache -f -t "${HOME}/.local/share/icons/hicolor" 2>/dev/null || true
# KDE keeps its own icon and service caches, which the two above do not touch.
rm -f "${HOME}/.cache/icon-cache.kcache"
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
command -v kbuildsycoca5 >/dev/null && kbuildsycoca5 >/dev/null 2>&1 || true

echo
echo "Installed:"
echo "  ${BIN_DIR}/acli"
echo "  ${APP_DIR}/acli.desktop"
case ":${PATH}:" in
    *":${BIN_DIR}:"*) ;;
    *) echo; echo "note: ${BIN_DIR} is not on your PATH." ;;
esac
