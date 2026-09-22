#!/bin/bash
# Prueba de humo de la VENTANA del receptor: la abre de verdad bajo un
# compositor sin cabeza (sway = Wayland) o un servidor X virtual (Xvfb = X11),
# espera el primer fotograma y sale sola (PEPOMOTE_SMOKE). Con «fallback», el
# primer intento falla a propósito (PEPOMOTE_SMOKE_FAIL_FIRST) y se exige que
# el receptor se relance (exec de sí mismo con render por software). Con
# «hang», el sondeo del driver del mando virtual se cuelga a propósito
# (PEPOMOTE_FAKE_DRIVER_HANG) y se exige que la ventana pinte igual y que el
# log diga «no contesta»: la regresión de la ventana negra de la 1.12. Vale con
# el binario o con el AppImage (PEPOMOTE_BIN=dist/PepoMote-x86_64.AppImage y
# APPIMAGE_EXTRACT_AND_RUN=1 donde no hay FUSE). Lo corre la CI (desktop.yml y
# release.yml); en un Linux con sway o Xvfb va igual. Todo queda en
# /tmp/pepomote-smoke/<modo>: receptor.out, config/receptor.log, sway.log o
# xvfb.log.
#
#   uso: smoke_gui.sh wayland|x11 [fallback|hang]
set -euo pipefail
MODE="${1:-wayland}"
FALLBACK="${2:-}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../.." && pwd)"
RX="${PEPOMOTE_BIN:-$ROOT/desktop/target/release/PepoMote}"
OUT="/tmp/pepomote-smoke/$MODE${FALLBACK:+-$FALLBACK}"
PORT=26781

[ -x "$RX" ] || { echo "no encuentro el receptor en $RX (cargo build --release en desktop/, o PEPOMOTE_BIN=…)"; exit 1; }
case "$MODE" in wayland | x11) ;; *) echo "uso: $0 wayland|x11 [fallback|hang]"; exit 2 ;; esac
case "$FALLBACK" in "" | fallback | hang) ;; *) echo "uso: $0 wayland|x11 [fallback|hang]"; exit 2 ;; esac
rm -rf "$OUT"
mkdir -p "$OUT/config" "$OUT/xdg"
chmod 700 "$OUT/xdg"
export XDG_RUNTIME_DIR="$OUT/xdg"
unset DISPLAY WAYLAND_DISPLAY SWAYSOCK PEPOMOTE_INJECT || true

RXPID=""; SWAYPID=""; XPID=""
cleanup() {
    [ -n "$RXPID" ] && kill "$RXPID" 2>/dev/null || true
    if [ -n "${SWAYSOCK:-}" ]; then swaymsg -s "$SWAYSOCK" exit 2>/dev/null || true; fi
    if [ -n "$SWAYPID" ]; then sleep 1; kill "$SWAYPID" 2>/dev/null || true; fi
    [ -n "$XPID" ] && kill "$XPID" 2>/dev/null || true
}
trap cleanup EXIT

if [ "$MODE" = wayland ]; then
    echo "== sway sin cabeza"
    WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
        sway -c "$HERE/sway-smoke.conf" > "$OUT/sway.log" 2>&1 &
    SWAYPID=$!
    SOCK=""
    for _ in $(seq 1 100); do
        SOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock 2>/dev/null | head -1 || true)
        if [ -n "$SOCK" ] && swaymsg -s "$SOCK" -t get_version >/dev/null 2>&1; then break; fi
        SOCK=""
        sleep 0.2
    done
    [ -n "$SOCK" ] || { echo "sway no arranca:"; cat "$OUT/sway.log"; exit 1; }
    export SWAYSOCK="$SOCK"
    # sway 1.7 no obedece WAYLAND_DISPLAY: crea wayland-N en el runtime dir (aquí solo hay uno)
    export WAYLAND_DISPLAY="$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | head -1)"
    echo "   WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
    seen_window() { swaymsg -t get_tree 2>/dev/null | grep -Eq '"app_id": ?"PepoMote"'; }
else
    echo "== Xvfb"
    Xvfb :77 -screen 0 1280x720x24 -nolisten tcp > "$OUT/xvfb.log" 2>&1 &
    XPID=$!
    export DISPLAY=:77
    for _ in $(seq 1 50); do
        xdpyinfo >/dev/null 2>&1 && break
        sleep 0.2
    done
    xdpyinfo >/dev/null 2>&1 || { echo "Xvfb no arranca:"; cat "$OUT/xvfb.log"; exit 1; }
    seen_window() { xwininfo -root -tree 2>/dev/null | grep -qi pepomote; }
fi

echo "== receptor ($MODE${FALLBACK:+, $FALLBACK}) · $RX"
ENV=(PEPOMOTE_CONFIG_DIR="$OUT/config" PEPOMOTE_PORT=$PORT PEPOMOTE_DSU_PORT=26780
     PEPOMOTE_NO_AUTOFIX=1 PEPOMOTE_NO_UPDATE_CHECK=1 PEPOMOTE_ASSUME_EMULATOR_CLOSED=1
     PEPOMOTE_DEBUG=1)
# con «hang» se espera más: el aviso de «no contesta» salta a los 3 s
SMOKE=2500
[ "$FALLBACK" = hang ] && SMOKE=4500
ENV+=(PEPOMOTE_SMOKE=$SMOKE)
[ "$FALLBACK" = fallback ] && ENV+=(PEPOMOTE_SMOKE_FAIL_FIRST=1)
[ "$FALLBACK" = hang ] && ENV+=(PEPOMOTE_FAKE_DRIVER_HANG=1)
set +e
env "${ENV[@]}" timeout 90 "$RX" > "$OUT/receptor.out" 2>&1 &
RXPID=$!
SEEN=no
while kill -0 "$RXPID" 2>/dev/null; do
    if seen_window; then SEEN=yes; fi
    sleep 0.3
done
wait "$RXPID"
CODE=$?
RXPID=""
set -e

LOG="$OUT/config/receptor.log"
echo "== receptor terminado con $CODE · ventana vista en el árbol: $SEEN"
echo "-- receptor.out:"; tail -15 "$OUT/receptor.out" 2>/dev/null || true
echo "-- receptor.log:"; grep -E "Ventana|PEPOMOTE_SMOKE|PEPOMOTE_FAKE|Mando virtual|PÁNICO|\[eframe|\[winit|\[glutin|\[egui_glow" "$LOG" 2>/dev/null || true
[ "$CODE" -eq 0 ] || { echo "FALLO: el receptor salió con $CODE"; exit 1; }
grep -q "Ventana: primer fotograma pintado" "$LOG" || { echo "FALLO: sin «primer fotograma pintado» en el log"; exit 1; }
[ "$SEEN" = yes ] || { echo "FALLO: la ventana PepoMote no apareció en el árbol de ventanas"; exit 1; }
if [ "$FALLBACK" = fallback ]; then
    grep -q "Ventana: relanzo (intento 2" "$LOG" || { echo "FALLO: el intento 1 falló pero no hubo relanzamiento"; exit 1; }
    grep -q "primer fotograma pintado (intento 2" "$LOG" || { echo "FALLO: el intento 2 no pintó"; exit 1; }
fi
if [ "$FALLBACK" = hang ]; then
    grep -q "PEPOMOTE_FAKE_DRIVER_HANG: el sondeo" "$LOG" || { echo "FALLO: el gancho de cuelgue no entró"; exit 1; }
    grep -q "Mando virtual: el sondeo del driver no contesta a los" "$LOG" || { echo "FALLO: el hub no avisó de que el driver no contesta"; exit 1; }
fi
echo "OK: humo $MODE${FALLBACK:+ $FALLBACK}"
