#!/usr/bin/env bash
# Empaqueta mobile-linux/target/release/PepoMote-Mobile como
# dist/PepoMote-Mobile-<arch>.AppImage. Pensado para CI (ubuntu-22.04-arm).
# Uso: appimage.sh [aarch64|x86_64]   (por defecto, la arquitectura actual)
set -euo pipefail

ARCH="${1:-$(uname -m)}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/mobile-linux/target/release/PepoMote-Mobile"
OUT="$ROOT/dist"
APPDIR="$OUT/PepoMote-Mobile.AppDir"
PKG="$ROOT/packaging/linux-mobile"

[[ -f "$BIN" ]] || { echo "Compila antes: cd mobile-linux && cargo build --release"; exit 1; }

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$OUT"

install -m 0755 "$BIN" "$APPDIR/usr/bin/PepoMote-Mobile"
install -m 0644 "$PKG/dev.pepotech.PepoMote.desktop" "$APPDIR/usr/share/applications/dev.pepotech.PepoMote.desktop"
cp "$PKG/dev.pepotech.PepoMote.desktop" "$APPDIR/dev.pepotech.PepoMote.desktop"
install -m 0644 "$ROOT/packaging/linux/pepomote.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/dev.pepotech.PepoMote.png"
cp "$ROOT/packaging/linux/pepomote.png" "$APPDIR/dev.pepotech.PepoMote.png"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/PepoMote-Mobile" "$@"
EOF
chmod +x "$APPDIR/AppRun"

TOOL="$OUT/appimagetool-$ARCH"
if [[ ! -x "$TOOL" ]]; then
    wget -q -O "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"
    chmod +x "$TOOL"
fi

# --appimage-extract-and-run: no depende de FUSE en el runner
ARCH="$ARCH" "$TOOL" --appimage-extract-and-run "$APPDIR" "$OUT/PepoMote-Mobile-$ARCH.AppImage"
echo "OK: $OUT/PepoMote-Mobile-$ARCH.AppImage"
