#!/bin/bash
# e2e del receptor en un compositor wlroots REAL: sway sin cabeza (sin GPU
# ni seat) + wev (visor de eventos Wayland, a pantalla completa) + receptor
# aislado sin ventana + móvil simulado (e2e_wayland.py). El receptor tiene
# que elegir el backend Wayland (puntero/teclado virtuales) y mover el
# cursor, hacer clic, girar la rueda y teclear en wev. Lo corre la CI
# (desktop.yml); en un Linux con sway, wev y python3 vale igual.
# Todo queda en /tmp/pepomote-e2e (wev.log, sway.log, receptor.out,
# config/receptor.log, outputs.json).
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
E2E=/tmp/pepomote-e2e
RX="${PEPOMOTE_BIN:-$ROOT/desktop/target/release/PepoMote}"
PORT=26771

[ -x "$RX" ] || { echo "no encuentro el receptor en $RX (cargo build --release en desktop/, o PEPOMOTE_BIN=…)"; exit 1; }
rm -rf "$E2E"
mkdir -p "$E2E/config" "$E2E/dolphin" "$E2E/xdg"
chmod 700 "$E2E/xdg"
export XDG_RUNTIME_DIR="$E2E/xdg"
unset DISPLAY WAYLAND_DISPLAY PEPOMOTE_INJECT SWAYSOCK || true

RXPID=""; SWAYPID=""
cleanup() {
    [ -n "$RXPID" ] && kill "$RXPID" 2>/dev/null || true
    if [ -n "${SWAYSOCK:-}" ]; then swaymsg -s "$SWAYSOCK" exit 2>/dev/null || true; fi
    if [ -n "$SWAYPID" ]; then sleep 1; kill "$SWAYPID" 2>/dev/null || true; fi
}
trap cleanup EXIT

echo "== sway sin cabeza"
WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
    sway -c "$HERE/sway.conf" > "$E2E/sway.log" 2>&1 &
SWAYPID=$!
SOCK=""
for _ in $(seq 1 100); do
    SOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock 2>/dev/null | head -1 || true)
    if [ -n "$SOCK" ] && swaymsg -s "$SOCK" -t get_version >/dev/null 2>&1; then break; fi
    SOCK=""
    sleep 0.2
done
[ -n "$SOCK" ] || { echo "sway no arranca:"; cat "$E2E/sway.log"; exit 1; }
export SWAYSOCK="$SOCK"
# sway 1.7 no obedece WAYLAND_DISPLAY: crea wayland-N en el runtime dir (aquí solo hay uno)
export WAYLAND_DISPLAY="$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | head -1)"
echo "   WAYLAND_DISPLAY=$WAYLAND_DISPLAY"

for _ in $(seq 1 75); do
    swaymsg -t get_tree | grep -Eq '"app_id": ?"wev"' && break
    sleep 0.2
done
if ! swaymsg -t get_tree | grep -Eq '"app_id": ?"wev"'; then
    echo "wev no aparece en el árbol de sway:"
    swaymsg -t get_tree | grep -E '"(app_id|name|pid)"' | head -20
    cat "$E2E/sway.log" | tail -20
    exit 1
fi
swaymsg -t get_outputs > "$E2E/outputs.json"

echo "== receptor aislado (sin ventana)"
PEPOMOTE_CONFIG_DIR="$E2E/config" PEPOMOTE_DOLPHIN_DIR="$E2E/dolphin" \
PEPOMOTE_PORT=$PORT PEPOMOTE_DSU_PORT=26770 PEPOMOTE_PAIR_CODE=1234 \
PEPOMOTE_NO_AUTOFIX=1 PEPOMOTE_ASSUME_EMULATOR_CLOSED=1 PEPOMOTE_NO_UI=1 PEPOMOTE_DEBUG=1 \
XDG_SESSION_TYPE=wayland "$RX" > "$E2E/receptor.out" 2>&1 &
RXPID=$!

echo "== móvil simulado"
python3 "$HERE/e2e_wayland.py" --e2e-dir "$E2E" --port "$PORT" --outputs "$E2E/outputs.json"
