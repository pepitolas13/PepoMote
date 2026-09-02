#!/usr/bin/env bash
# Empaqueta el binario musl (postmarketOS / Alpine) como
# dist/PepoMote-Mobile-<variante>.tar.gz con lanzador, icono e install.sh.
# Uso: tarball.sh aarch64-musl
set -euo pipefail

VARIANT="${1:-aarch64-musl}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/mobile-linux/target/release/PepoMote-Mobile"
OUT="$ROOT/dist"
STAGE="$OUT/PepoMote-Mobile"
PKG="$ROOT/packaging/linux-mobile"

[[ -f "$BIN" ]] || { echo "Compila antes: cd mobile-linux && cargo build --release"; exit 1; }
# Tiene que ser un ejecutable DINÁMICO: el musl estático no puede dlopen
# (Wayland, EGL) y muere al arrancar sin abrir ventana.
if ! readelf -l "$BIN" | grep -q INTERP; then
    echo "ERROR: $BIN es estático (sin intérprete). Compila con RUSTFLAGS=\"-C target-feature=-crt-static\""
    exit 1
fi
echo "Enlazado dinámico OK: $(readelf -l "$BIN" | grep -o 'ld-musl[^]]*')"

rm -rf "$STAGE"
mkdir -p "$STAGE" "$OUT"
install -m 0755 "$BIN" "$STAGE/PepoMote-Mobile"
install -m 0644 "$PKG/dev.pepotech.PepoMote.desktop" "$STAGE/dev.pepotech.PepoMote.desktop"
install -m 0644 "$ROOT/packaging/linux/pepomote.png" "$STAGE/dev.pepotech.PepoMote.png"
install -m 0644 "$PKG/90-pepomote-iio.rules" "$STAGE/90-pepomote-iio.rules"
install -m 0755 "$PKG/install.sh" "$STAGE/install.sh"

tar -C "$OUT" -czf "$OUT/PepoMote-Mobile-$VARIANT.tar.gz" PepoMote-Mobile
echo "OK: $OUT/PepoMote-Mobile-$VARIANT.tar.gz"
