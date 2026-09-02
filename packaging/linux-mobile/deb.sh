#!/usr/bin/env bash
# Paquete Debian del emisor para Mobian (y cualquier Debian/Ubuntu ARM):
# se descarga desde el navegador del móvil, se toca y "Instalar". Instala el
# binario, el lanzador con icono y la regla udev del IMU (con root, sin
# preguntar). Uso: deb.sh [arm64|amd64]   (por defecto, la arquitectura actual)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/mobile-linux/target/release/PepoMote-Mobile"
PKG="$ROOT/packaging/linux-mobile"
OUT="$ROOT/dist"
ARCH="${1:-$(dpkg --print-architecture)}"
VER="$(grep -m1 '^version' "$ROOT/mobile-linux/Cargo.toml" | cut -d'"' -f2)"
STAGE="$OUT/deb/pepomote-mobile_${VER}_${ARCH}"

[[ -f "$BIN" ]] || { echo "Compila antes: cd mobile-linux && cargo build --release"; exit 1; }

rm -rf "$STAGE"
mkdir -p "$STAGE/DEBIAN" "$STAGE/usr/bin" "$STAGE/usr/share/applications" \
    "$STAGE/usr/share/icons/hicolor/256x256/apps" "$STAGE/usr/lib/udev/rules.d" \
    "$STAGE/usr/share/doc/pepomote-mobile" "$OUT"

install -m 0755 "$BIN" "$STAGE/usr/bin/PepoMote-Mobile"
install -m 0644 "$PKG/dev.pepotech.PepoMote.desktop" "$STAGE/usr/share/applications/dev.pepotech.PepoMote.desktop"
install -m 0644 "$ROOT/packaging/linux/pepomote.png" "$STAGE/usr/share/icons/hicolor/256x256/apps/dev.pepotech.PepoMote.png"
install -m 0644 "$PKG/90-pepomote-iio.rules" "$STAGE/usr/lib/udev/rules.d/90-pepomote-iio.rules"
install -m 0644 "$ROOT/docs/MOBILE-LINUX.md" "$STAGE/usr/share/doc/pepomote-mobile/README.md"

SIZE_KB="$(du -sk "$STAGE/usr" | cut -f1)"
cat > "$STAGE/DEBIAN/control" <<CONTROL
Package: pepomote-mobile
Version: $VER
Section: games
Priority: optional
Architecture: $ARCH
Installed-Size: $SIZE_KB
Depends: libc6, libwayland-client0, libxkbcommon0, libegl1, libgl1
Maintainer: PepoTech <pepitolas13@users.noreply.github.com>
Homepage: https://github.com/pepitolas13/PepoMote
Description: PepoMote sender for Linux phones (Mobian, Debian...)
 Your phone as a Wii-style motion pointer and controller for your PC.
 Pairs with the PepoMote receiver using the 4-digit code shown under its QR;
 works as a Wiimote for the Dolphin emulator too.
CONTROL

cat > "$STAGE/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
set -e
if command -v udevadm >/dev/null 2>&1; then
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger --subsystem-match=iio 2>/dev/null || true
fi
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database -q /usr/share/applications 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q /usr/share/icons/hicolor 2>/dev/null || true
exit 0
POSTINST
chmod 0755 "$STAGE/DEBIAN/postinst"

DEB="$OUT/pepomote-mobile_${VER}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "$STAGE" "$DEB"
# Copia con nombre estable: el enlace .../releases/latest/download/pepomote-mobile_arm64.deb
# no cambia entre versiones (vale para un QR impreso)
cp "$DEB" "$OUT/pepomote-mobile_${ARCH}.deb"
echo "OK: $DEB (+ pepomote-mobile_${ARCH}.deb)"
