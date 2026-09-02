#!/usr/bin/env bash
# Empaqueta el binario ya compilado como dist/PepoMote-Mobile-<arch>.flatpak
# (bundle de un solo archivo: se instala tocándolo en GNOME Software / Discover).
# Pensado para CI (ubuntu-22.04-arm). Uso: flatpak.sh [aarch64|x86_64]
set -euo pipefail

ARCH="${1:-$(uname -m)}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/mobile-linux/target/release/PepoMote-Mobile"
OUT="$ROOT/dist"
MANIFEST="$ROOT/packaging/linux-mobile/flatpak/dev.pepotech.PepoMote.yml"
FLATHUB="https://flathub.org/repo/flathub.flatpakrepo"

[[ -f "$BIN" ]] || { echo "Compila antes: cd mobile-linux && cargo build --release"; exit 1; }
mkdir -p "$OUT"

flatpak remote-add --user --if-not-exists flathub "$FLATHUB"
flatpak install --user -y --noninteractive flathub org.freedesktop.Platform//24.08 org.freedesktop.Sdk//24.08

flatpak-builder --user --force-clean --disable-rofiles-fuse \
    --repo="$OUT/flatpak-repo" "$OUT/flatpak-build" "$MANIFEST"
# --runtime-repo: si el móvil no tiene el runtime, la tienda lo baja de Flathub sola
flatpak build-bundle "$OUT/flatpak-repo" "$OUT/PepoMote-Mobile-$ARCH.flatpak" dev.pepotech.PepoMote \
    --runtime-repo="$FLATHUB"
echo "OK: $OUT/PepoMote-Mobile-$ARCH.flatpak"
