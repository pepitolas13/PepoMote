"""Build-independent runner for isolated RetroArch/Switch/Cemu/Nunchuk/Wii IR checks.

Usage: python run_switch_preview.py <PepoMote.exe> [output-directory]
All settings/emulator directories and ports are isolated from the user's.
PEPOMOTE_E2E_SUITES=e2e_retroarch,e2e_eden picks the suites (default: all).
"""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time

sys.stdout.reconfigure(encoding="utf-8", errors="replace")

HERE = Path(__file__).resolve().parent
BIN = Path(sys.argv[1]).resolve()
ROOT = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else Path(tempfile.mkdtemp(prefix="pepomote-switch-e2e-"))


def free_ports():
    # Claim sockets during the check, then release immediately before launch.
    for port in range(27771, 28771, 10):
        held = []
        try:
            for typ, p in [(socket.SOCK_STREAM, port), (socket.SOCK_DGRAM, port),
                           (socket.SOCK_DGRAM, port+1), (socket.SOCK_DGRAM, port-1),
                           (socket.SOCK_DGRAM, port+2), (socket.SOCK_DGRAM, port+3),
                           (socket.SOCK_DGRAM, port+4), (socket.SOCK_DGRAM, port+5),
                           (socket.SOCK_DGRAM, port+7)]:
                s = socket.socket(socket.AF_INET, typ)
                held.append(s)
                s.bind(("127.0.0.1", p))
            return port
        except OSError:
            continue
        finally:
            for s in held:
                s.close()
    raise RuntimeError("No unused test ports")


def run_suite(name):
    base = ROOT / (name + "-" + str(time.time_ns()))
    port = free_ports()
    env = os.environ.copy()
    for key, path in {
        "APPDATA": base / "appdata",
        "PEPOMOTE_CONFIG_DIR": base / "config",
        "PEPOMOTE_DOLPHIN_DIR": base / "dolphin",
        "PEPOMOTE_CEMU_DIR": base / "appdata" / "Cemu",
        "PEPOMOTE_EDEN_DIR": base / "eden",
        "PEPOMOTE_RETROARCH_DIR": base / "retroarch",
    }.items():
        path.mkdir(parents=True, exist_ok=True)
        env[key] = str(path)
    env.update(PEPOMOTE_PORT=str(port), PEPOMOTE_DSU_PORT=str(port-1),
               PEPOMOTE_RETROARCH_PORT=str(port+2), PEPOMOTE_RETROARCH_CMD_PORT=str(port+7),
               PEPOMOTE_PAIR_CODE="1234", PEPOMOTE_ASSUME_EMULATOR_CLOSED="1",
               PEPOMOTE_NO_TRAY="1", PEPOMOTE_NO_UI="1", PYTHONIOENCODING="utf-8",
               # Windows: un receptor de prueba nunca instala el driver del
               # mando virtual (abriría un UAC); la CI lo instala antes con
               # `PepoMote.exe --install-driver`
               PEPOMOTE_NO_DRIVER_SETUP="1")
    (base / "config" / "settings.json").write_text(json.dumps({
        "sens_deg": 40, "abs_mode": True, "auto_mode": False,
        "auto_dolphin": True, "auto_cemu": True, "auto_eden": True, "auto_retroarch": True,
        "update_check": False, "lang": "es",
    }), encoding="utf-8")
    (base / "eden" / "qt-config.ini").write_bytes((HERE / "fixtures" / "qt-config.ini").read_bytes())
    creation = subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0
    with (base / "receiver.out").open("wb") as log:
        receiver = subprocess.Popen([str(BIN), "--minimized"], env=env,
                                    stdout=log, stderr=log, creationflags=creation)
        try:
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                if receiver.poll() is not None:
                    raise RuntimeError(f"Receiver exited: {receiver.returncode}")
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=0.1):
                        break
                except OSError:
                    time.sleep(0.05)
            else:
                raise TimeoutError("Receiver did not listen")
            command = [sys.executable, str(HERE / (name + ".py"))]
            if name == "e2e_cemu":
                command.append(env["APPDATA"])
            result = subprocess.run(command, env=env, capture_output=True,
                                    timeout=int(os.environ.get("PEPOMOTE_E2E_TIMEOUT", "180")),
                                    creationflags=creation)
            (base / "checks.log").write_bytes(result.stdout + result.stderr)
            print(result.stdout.decode("utf-8", errors="replace"), end="", flush=True)
            if result.returncode:
                print(result.stderr.decode("utf-8", errors="replace"), flush=True)
                raise RuntimeError(f"{name} failed: {result.returncode}; logs: {base}")
            print(f"PASS {name}; logs: {base}", flush=True)
        finally:
            if sys.exc_info()[0] is not None:
                status = receiver.poll()
                detail = "sigue vivo" if status is None else f"{status} (0x{status & 0xffffffff:08x})"
                print(f"Estado del receptor antes de limpiar la prueba: {detail}", flush=True)
            receiver.terminate()
            try:
                receiver.wait(timeout=8)
            except subprocess.TimeoutExpired:
                receiver.kill()
                try:
                    receiver.wait(timeout=30)
                except subprocess.TimeoutExpired:
                    # Un hilo atascado en el núcleo (una llamada al driver del
                    # mando virtual que nunca vuelve) deja el proceso «terminando»
                    # para siempre: no se espera más, se cuenta, y la siguiente
                    # prueba fallará al no poder abrir los puertos, con este aviso
                    # delante en vez de un runner colgado media hora.
                    print(f"El receptor (PID {receiver.pid}) no muere ni con kill: hilo atascado en el núcleo; "
                          "mira receiver.out y receptor.log", flush=True)
            if sys.exc_info()[0] is not None:
                # La CI debe conservar también lo que vio el receptor cuando
                # falla una comprobación, no solo el error del móvil simulado.
                for path in (base / "receiver.out", base / "config" / "receptor.log"):
                    if path.is_file():
                        print(f"\nDiagnóstico: {path.name}", flush=True)
                        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
                        print("\n".join(lines[-120:]), flush=True)


if not BIN.is_file():
    raise SystemExit(f"Executable does not exist: {BIN}")
# PEPOMOTE_E2E_SUITES=e2e_retroarch,e2e_eden limita las suites (la CI corre solo
# las que no dependen de rutas de Windows)
SUITES = [s for s in os.environ.get("PEPOMOTE_E2E_SUITES", "e2e_retroarch,e2e_eden,e2e_cemu,e2e_nunchuk,e2e_dolphin_ir").split(",") if s]
for suite in SUITES:
    run_suite(suite)
print("All isolated integration suites passed", flush=True)
