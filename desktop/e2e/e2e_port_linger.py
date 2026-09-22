"""e2e: el puerto del móvil en manos de un receptor que acaba de morir.

Reproduce lo que pasó al actualizar en caliente a la 1.13.0: el receptor nuevo
arranca menos de un segundo después de morir el viejo y el UDP del móvil sigue
un instante en la tabla del sistema a nombre de un PID que ya no existe. Hasta
la 1.13.0 el receptor se rendía a la primera: sin «Inyección: SendInput» en el
log y sin puntero hasta reabrirlo (el pie de la ventana decía «Inyección:
ninguna»).

Aquí un proceso padre abre el UDP del receptor de pruebas con el handle
heredable, lanza un hijo que lo hereda y muere al instante: el puerto queda «de
un muerto» hasta que el hijo lo suelta (2 s). El receptor AISLADO (puertos
propios, configuración propia, PEPOMOTE_SMOKE) tiene que esperar, cogerlo y
llegar a crear el inyector; se comprueba en su receptor.log.

Uso: python e2e_port_linger.py <PepoMote.exe> [directorio de trabajo]
En Linux sin ventana añade PEPOMOTE_NO_UI=1 al entorno (el receptor no sale
solo y se cierra desde aquí). Sale con 0 si pasa y con 1 si falla."""
import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HOLD_SECONDS = 2.0


def parent(port: int, hold: float) -> None:
    """Abre el UDP del receptor, se lo deja en herencia a un hijo y muere."""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.bind(("0.0.0.0", port))
    s.set_inheritable(True)
    child = subprocess.Popen([sys.executable, __file__, "--child", str(hold)], close_fds=False)
    print(f"padre {os.getpid()} tiene UDP {port}; hijo {child.pid} lo hereda y lo suelta a los {hold} s", flush=True)
    # sin cerrar el socket: el handle sigue vivo en el hijo y el sistema sigue
    # apuntando el puerto a este PID, que ya no existe
    os._exit(0)


def child(hold: float) -> None:
    time.sleep(hold)
    os._exit(0)


def free_base_port() -> int:
    # TCP p y UDP p (móvil), UDP p-1 (DSU), UDP p+1 (instancia única)
    for port in range(27771, 28771, 10):
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


def owner_line(port: int) -> str:
    if os.name != "nt":
        return "(sin consulta del dueño fuera de Windows)"
    out = subprocess.run(
        ["powershell", "-NoProfile", "-Command",
         f"Get-NetUDPEndpoint -LocalPort {port} -ErrorAction SilentlyContinue | "
         "ForEach-Object { '{0}:{1} dueño PID {2}' -f $_.LocalAddress, $_.LocalPort, $_.OwningProcess }"],
        capture_output=True, text=True,
    )
    return out.stdout.strip() or "(libre)"


def main() -> int:
    if len(sys.argv) >= 3 and sys.argv[1] == "--parent":
        parent(int(sys.argv[2]), float(sys.argv[3]))
        return 0
    if len(sys.argv) >= 3 and sys.argv[1] == "--child":
        child(float(sys.argv[2]))
        return 0
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    exe = Path(sys.argv[1]).resolve()
    root = Path(sys.argv[2]).resolve() if len(sys.argv) > 2 else Path(tempfile.mkdtemp(prefix="pepomote-linger-"))
    cfg = root / "config"
    cfg.mkdir(parents=True, exist_ok=True)
    log = cfg / "receptor.log"
    if log.exists():
        log.unlink()

    port = free_base_port()
    # sin capturar la salida: el hijo heredaría la tubería y este run()
    # esperaría a que el hijo muriese (y soltase el puerto) antes de seguir
    subprocess.run([sys.executable, __file__, "--parent", str(port), str(HOLD_SECONDS)], check=True)
    time.sleep(0.3)
    print("antes de arrancar el receptor:", owner_line(port), flush=True)

    env = os.environ.copy()
    env.update(
        PEPOMOTE_PORT=str(port), PEPOMOTE_DSU_PORT=str(port - 1), PEPOMOTE_CONFIG_DIR=str(cfg),
        PEPOMOTE_NO_TRAY="1", PEPOMOTE_NO_UPDATE_CHECK="1", PEPOMOTE_NO_DRIVER_SETUP="1",
        PEPOMOTE_ASSUME_EMULATOR_CLOSED="1", PEPOMOTE_PAIR_CODE="1234", PEPOMOTE_SMOKE="20000",
    )
    t0 = time.time()
    receiver = subprocess.Popen([str(exe)], env=env)
    text = ""
    try:
        # el receptor tiene que cogerlo dentro de su espera (3 s): se le dan 15
        while time.time() - t0 < 15:
            if log.exists():
                text = log.read_text(encoding="utf-8", errors="replace")
                if "Inyección" in text:
                    break
            if receiver.poll() is not None:
                break
            time.sleep(0.2)
    finally:
        receiver.kill()
        receiver.wait(timeout=30)
    if log.exists():
        text = log.read_text(encoding="utf-8", errors="replace")
    print("después:", owner_line(port))
    print("---- receptor.log ----")
    print(text)

    fails = 0

    def check(cond: bool, msg: str) -> None:
        nonlocal fails
        print(("OK   " if cond else "FAIL ") + msg)
        if not cond:
            fails += 1

    check("ocupado al arrancar" in text and "quedó libre solo" in text,
          "el receptor esperó a que el puerto del muerto quedase libre y lo dejó en el log")
    check("Inyección" in text, "la telemetría llegó a crear el inyector (o a decir por qué no) en vez de morir")
    check("PÁNICO" not in text, "sin pánicos")
    print("RESULTADO:", "OK" if fails == 0 else f"{fails} fallo(s)")
    return 0 if fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
