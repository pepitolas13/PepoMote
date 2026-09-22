"""e2e: la ventana del receptor se cuelga después de pintar, y se nota.

Hasta la 1.13.1 un cuelgue del hilo de la ventana posterior al primer
fotograma no dejaba rastro: el vigilante callaba en cuanto había fotograma,
`--diag` no podía preguntar nada al receptor abierto y el modo humo daba por
buena una ventana que ya no repintaba (la ventana negra de un usuario con la
1.13.0, con el QR a la vista un instante y luego nada).

Aquí un receptor AISLADO (puertos y configuración propios) arranca con
PEPOMOTE_FAKE_UI_HANG=1: su hilo de ventana se duerme en el fotograma 3, con
el QR ya pintado. Se comprueba que

- `PepoMote.exe --diag` (mismo PEPOMOTE_PORT) imprime «Receptor abierto:
  PepoMote <versión> · último fotograma hace N s · último paso: gancho de
  prueba»: el cerrojo contesta aunque la ventana esté colgada;
- receptor.log apunta «Ventana: sin repintar desde hace 10 s · último paso:
  gancho de prueba»;
- el modo humo (PEPOMOTE_SMOKE) sale con 4 en vez de quedarse colgado.

Y, sin el gancho, que el humo sale con 0 y el log no habla de cuelgues.

Uso: python e2e_ui_hang.py <PepoMote.exe> [directorio de trabajo]
Sale con 0 si pasa y con 1 si falla."""
import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path


def free_base_port() -> int:
    # TCP p y UDP p (móvil), UDP p-1 (DSU), UDP p+1 (instancia única)
    for port in range(28771, 29771, 10):
        held = []
        try:
            for typ, p in [(socket.SOCK_STREAM, port), (socket.SOCK_DGRAM, port),
                           (socket.SOCK_DGRAM, port - 1), (socket.SOCK_DGRAM, port + 1)]:
                s = socket.socket(socket.AF_INET, typ)
                held.append(s)
                s.bind(("0.0.0.0", p))
            return port
        except OSError:
            continue
        finally:
            for s in held:
                s.close()
    raise RuntimeError("no hay puertos libres para la prueba")


def receiver_env(port: int, cfg: Path, **extra: str) -> dict:
    env = os.environ.copy()
    env.update(
        PEPOMOTE_PORT=str(port), PEPOMOTE_DSU_PORT=str(port - 1), PEPOMOTE_CONFIG_DIR=str(cfg),
        PEPOMOTE_NO_TRAY="1", PEPOMOTE_NO_UPDATE_CHECK="1", PEPOMOTE_NO_DRIVER_SETUP="1",
        PEPOMOTE_ASSUME_EMULATOR_CLOSED="1", PEPOMOTE_PAIR_CODE="1234",
    )
    env.update(extra)
    return env


def read_log(cfg: Path) -> str:
    log = cfg / "receptor.log"
    return log.read_text(encoding="utf-8", errors="replace") if log.exists() else ""


def wait_for(cfg: Path, receiver: subprocess.Popen, needle: str, seconds: float) -> str:
    t0 = time.time()
    text = ""
    while time.time() - t0 < seconds:
        text = read_log(cfg)
        if needle in text or receiver.poll() is not None:
            break
        time.sleep(0.25)
    return text


def wait_exit(receiver: subprocess.Popen, seconds: float):
    try:
        return receiver.wait(timeout=seconds)
    except subprocess.TimeoutExpired:
        return None


class Checks:
    def __init__(self) -> None:
        self.fails = 0

    def __call__(self, cond: bool, msg: str) -> None:
        print(("OK   " if cond else "FAIL ") + msg, flush=True)
        if not cond:
            self.fails += 1


def case_hang(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "hang" / "config"
    cfg.mkdir(parents=True, exist_ok=True)
    port = free_base_port()
    env = receiver_env(port, cfg, PEPOMOTE_FAKE_UI_HANG="1", PEPOMOTE_SMOKE="3000")
    print(f"[cuelgue] receptor aislado en el puerto {port}", flush=True)
    receiver = subprocess.Popen([str(exe)], env=env)
    diag = ""
    try:
        text = wait_for(cfg, receiver, "PEPOMOTE_FAKE_UI_HANG", 20)
        check("primer fotograma pintado" in text, "la ventana llegó a pintar antes del gancho")
        check("PEPOMOTE_FAKE_UI_HANG" in text, "el gancho se disparó en el fotograma 3")
        # --diag desde otro proceso, con la ventana ya colgada: el cerrojo contesta
        time.sleep(2)
        out = subprocess.run([str(exe), "--diag"], env=env, capture_output=True, timeout=60)
        diag = out.stdout.decode("utf-8", errors="replace")
        print("---- --diag (receptor abierto) ----", flush=True)
        print("\n".join(l for l in diag.splitlines() if l.startswith("Receptor abierto")), flush=True)
        check("Receptor abierto: PepoMote " in diag, "--diag encontró al receptor abierto por su cerrojo")
        check("último paso: gancho de prueba" in diag, "--diag dice en qué paso se quedó la ventana")
        check("último fotograma hace" in diag, "--diag dice cuánto hace del último fotograma")
        text = wait_for(cfg, receiver, "sin repintar desde hace", 25)
        check("Ventana: sin repintar desde hace 10 s · último paso: gancho de prueba" in text,
              "el vigilante apuntó el cuelgue a los 10 s con su miga")
        code = wait_exit(receiver, 30)
        print(f"[cuelgue] el receptor salió con {code}", flush=True)
        check(code == 4, "el modo humo salió con 4 (ventana colgada tras pintar) en vez de quedarse")
    finally:
        if receiver.poll() is None:
            receiver.kill()
            receiver.wait(timeout=30)
    text = read_log(cfg)
    print("---- receptor.log (cuelgue) ----", flush=True)
    print(text, flush=True)
    check("PEPOMOTE_SMOKE: la ventana no repinta desde hace" in text, "el humo dejó dicho por qué salió")
    check("PÁNICO" not in text, "sin pánicos")


def case_normal(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "normal" / "config"
    cfg.mkdir(parents=True, exist_ok=True)
    port = free_base_port()
    env = receiver_env(port, cfg, PEPOMOTE_SMOKE="3000")
    print(f"[normal] receptor aislado en el puerto {port}", flush=True)
    receiver = subprocess.Popen([str(exe)], env=env)
    try:
        code = wait_exit(receiver, 40)
    finally:
        if receiver.poll() is None:
            receiver.kill()
            receiver.wait(timeout=30)
    text = read_log(cfg)
    print("---- receptor.log (normal) ----", flush=True)
    print(text, flush=True)
    check(code == 0, f"el humo salió con 0 (salió con {code})")
    check("primer fotograma pintado" in text, "la ventana pintó")
    check("sin repintar" not in text, "una ventana viva no dispara al vigilante")
    check("PÁNICO" not in text, "sin pánicos")


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    exe = Path(sys.argv[1]).resolve()
    root = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else Path(tempfile.mkdtemp(prefix="pepomote-ui-hang-"))
    check = Checks()
    case_hang(exe, root, check)
    case_normal(exe, root, check)
    print("RESULTADO:", "OK" if check.fails == 0 else f"{check.fails} fallo(s)")
    return 0 if check.fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
