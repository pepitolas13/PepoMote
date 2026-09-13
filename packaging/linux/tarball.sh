#!/usr/bin/env bash
# Empaqueta desktop/target/release/PepoMote como dist/PepoMote-linux-x86_64.tar.gz:
# el binario, el lanzador, el icono, la regla udev de uinput e install.sh.
# Para quien prefiera un tar.gz al AppImage (o en distros donde FUSE da
# guerra): descomprimir y ./install.sh.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/desktop/target/release/PepoMote"
OUT="$ROOT/dist"
STAGE="$OUT/PepoMote"
PKG="$ROOT/packaging/linux"

[[ -f "$BIN" ]] || { echo "Compila antes: cd desktop && cargo build --release"; exit 1; }

rm -rf "$STAGE"
mkdir -p "$STAGE" "$OUT"
install -m 0755 "$BIN" "$STAGE/PepoMote"
install -m 0644 "$PKG/PepoMote.desktop" "$STAGE/PepoMote.desktop"
install -m 0644 "$PKG/pepomote.png" "$STAGE/pepomote.png"
install -m 0644 "$PKG/99-pepomote.rules" "$STAGE/99-pepomote.rules"
install -m 0755 "$PKG/install.sh" "$STAGE/install.sh"

tar -C "$OUT" -czf "$OUT/PepoMote-linux-x86_64.tar.gz" PepoMote
rm -rf "$STAGE"
echo "OK: $OUT/PepoMote-linux-x86_64.tar.gz"
