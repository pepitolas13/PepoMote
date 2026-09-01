#!/usr/bin/env bash
# Empaqueta desktop/target/release/PepoMote como dist/PepoMote-x86_64.AppImage.
# Pensado para CI (ubuntu-22.04). Requiere: wget, file.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/desktop/target/release/PepoMote"
OUT="$ROOT/dist"
APPDIR="$OUT/PepoMote.AppDir"

[[ -f "$BIN" ]] || { echo "Compila antes: cargo build --release"; exit 1; }

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/256x256/apps" "$OUT"

install -m 0755 "$BIN" "$APPDIR/usr/bin/PepoMote"
install -m 0644 "$ROOT/packaging/linux/PepoMote.desktop" "$APPDIR/usr/share/applications/PepoMote.desktop"
cp "$ROOT/packaging/linux/PepoMote.desktop" "$APPDIR/PepoMote.desktop"
if [[ -f "$ROOT/packaging/linux/pepomote.png" ]]; then
    install -m 0644 "$ROOT/packaging/linux/pepomote.png" "$APPDIR/usr/share/icons/hicolor/256x256/apps/pepomote.png"
    cp "$ROOT/packaging/linux/pepomote.png" "$APPDIR/pepomote.png"
fi

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/PepoMote" "$@"
EOF
chmod +x "$APPDIR/AppRun"

TOOL="$OUT/appimagetool"
if [[ ! -x "$TOOL" ]]; then
    wget -q -O "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
    chmod +x "$TOOL"
fi

ARCH=x86_64 "$TOOL" "$APPDIR" "$OUT/PepoMote-x86_64.AppImage"
echo "OK: $OUT/PepoMote-x86_64.AppImage"
