"""e2e (solo Windows): el permiso de administrador del driver del mando
virtual se pide con la ventana de PepoMote a la vista y delante.

Informe (1.13.2): «de forma generalizada no les pide administrador de
primeras». El permiso existía, pero se pedía a los pocos ms de arrancar,
sin ventana, y Windows lo dejaba minimizado y parpadeando en la barra de
tareas (con `--minimized`, al iniciar sesión, ni eso). Aquí, con receptores
AISLADOS y `PEPOMOTE_FAKE_DRIVER_MISSING` (el sondeo dice «falta el driver» y
el instalador contesta «cancelado» sin lanzar nada: nunca sale un permiso de
verdad), se comprueba:

- ventana: el permiso se pide DESPUÉS del primer fotograma, con la ventana
  del receptor delante y como dueña (su HWND en el log); sin el foco, no.
- minimizado: con `--minimized` no se pide mientras no hay ventana; al abrirla
  (otra copia del exe), sí, con esa ventana.
- ya_intentado: con el intento apuntado en los ajustes no se pide y el log
  dice por qué.

Uso: python e2e_driver_prompt.py <PepoMote.exe> [directorio] [--cases a,b]
Sale con 0 si pasa y con 1 si falla."""
import ctypes
import os
import re
import subprocess
import sys
import tempfile
import time
from ctypes import wintypes
from pathlib import Path

from e2e_window_windows import (
    Checks, free_base_port, kernel32, read_log, read_settings, receiver_env, receiver_window,
    second_copy, seed_settings, start_receiver, stop, user32, wait_log, wait_until,
)

user32.AttachThreadInput.argtypes = [wintypes.DWORD, wintypes.DWORD, wintypes.BOOL]
user32.AttachThreadInput.restype = wintypes.BOOL
user32.SetForegroundWindow.argtypes = [wintypes.HWND]
user32.BringWindowToTop.argtypes = [wintypes.HWND]
user32.PeekMessageW.argtypes = [ctypes.c_void_p, wintypes.HWND, wintypes.UINT, wintypes.UINT, wintypes.UINT]
kernel32.GetCurrentThreadId.restype = wintypes.DWORD
SW_SHOWNOACTIVATE = 4

ASKED = "se pide el permiso de administrador para el driver"
PAINTED = "primer fotograma pintado"
WANTED = "se instalará el embebido"
SETUP_VERSION = "1.22.0"


def env_for(port: int, cfg: Path, **extra: str) -> dict:
    env = receiver_env(port, cfg, PEPOMOTE_SMOKE="60000", PEPOMOTE_FAKE_DRIVER_MISSING="1", **extra)
    # lo que se prueba es justo la instalación al arrancar
    env.pop("PEPOMOTE_NO_DRIVER_SETUP", None)
    return env


def foreground(h) -> bool:
    return user32.GetForegroundWindow() == h


def bring_to_front(h, seconds: float = 20.0) -> bool:
    """Pone delante la ventana, como lo haría un clic. El script corre en
    segundo plano: se engancha a la cola de entrada de quien tiene el foco.
    Mientras alguien escribe o hace clic en otra ventana, Windows no deja
    cambiar el foco (bloqueo de primer plano): se reintenta un rato, sin
    mandar teclas a nadie."""
    msg = ctypes.create_string_buffer(64)
    user32.PeekMessageW(msg, None, 0, 0, 0)  # que este hilo tenga cola de mensajes
    me = kernel32.GetCurrentThreadId()
    t0 = time.time()
    while time.time() - t0 < seconds:
        if foreground(h):
            return True
        fg = user32.GetForegroundWindow()
        fg_thread = user32.GetWindowThreadProcessId(fg, None) if fg else 0
        attached = bool(fg_thread) and fg_thread != me and bool(user32.AttachThreadInput(me, fg_thread, True))
        try:
            user32.BringWindowToTop(h)
            user32.SetForegroundWindow(h)
        finally:
            if attached:
                user32.AttachThreadInput(me, fg_thread, False)
        if wait_until(lambda: foreground(h), 0.5):
            return True
    return False


def before(text: str, first: str, second: str) -> bool:
    a, b = text.find(first), text.find(second)
    return a >= 0 and b >= 0 and a < b


def owner_of(text: str):
    m = re.search(ASKED + r" \(con la ventana 0x([0-9A-F]+) como dueña\)", text)
    return int(m.group(1), 16) if m else None


def asked_properly(tag: str, cfg: Path, receiver, hwnd, check: Checks) -> None:
    text = wait_log(cfg, "instalador terminado", 10, receiver)
    check(before(text, PAINTED, ASKED), f"[{tag}] el permiso se pidió después del primer fotograma")
    owner = owner_of(text)
    check(owner is not None and owner == hwnd, f"[{tag}] con la ventana del receptor como dueña (0x{(owner or 0):X} = 0x{(hwnd or 0):X})")
    check("instalador terminado: Declined" in text, f"[{tag}] el instalador falso contestó «cancelado»")
    check(text.count(ASKED) == 1, f"[{tag}] se pidió una sola vez ({text.count(ASKED)})")
    check(wait_until(lambda: read_settings(cfg).get("vigem_setup_version") == SETUP_VERSION, 5),
          f"[{tag}] el intento queda apuntado (no se vuelve a pedir solo)")


def case_ventana(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "ventana" / "config"
    seed_settings(cfg)
    port = free_base_port()
    print(f"[ventana] receptor aislado en el puerto {port}", flush=True)
    # Sin activarla (como si se hubiera hecho clic en otra cosa mientras
    # arrancaba): la primera ShowWindow de un proceso usa la de STARTUPINFO
    si = subprocess.STARTUPINFO()
    si.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    si.wShowWindow = SW_SHOWNOACTIVATE
    receiver = subprocess.Popen([str(exe)], env=env_for(port, cfg), startupinfo=si)
    try:
        text = wait_log(cfg, PAINTED, 30, receiver)
        check(PAINTED in text, "[ventana] la ventana pintó")
        check(WANTED in text, "[ventana] el hub dejó la instalación pedida, sin lanzarla")
        wait_until(lambda: receiver_window(receiver.pid) is not None, 10)
        hwnd = receiver_window(receiver.pid)
        check(hwnd is not None, "[ventana] ventana del receptor encontrada")
        if hwnd is None:
            return
        time.sleep(1.0)
        if not foreground(hwnd):
            # Nacida sin el foco (se usaba otra ventana): entonces espera
            check(ASKED not in read_log(cfg), "[ventana] sin el foco no se pide el permiso")
            if not bring_to_front(hwnd):
                check.inconclusive("[ventana] Windows no deja al script poner la ventana delante mientras se usa otra")
                return
        else:
            print("[ventana] la ventana nació con el foco", flush=True)
        asked_properly("ventana", cfg, receiver, hwnd, check)
    finally:
        stop(receiver)
        print("---- receptor.log (ventana) ----", flush=True)
        print(read_log(cfg), flush=True)


def case_minimizado(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "minimizado" / "config"
    seed_settings(cfg)
    port = free_base_port()
    env = env_for(port, cfg)
    print(f"[minimizado] receptor aislado en el puerto {port}", flush=True)
    receiver = subprocess.Popen([str(exe), "--minimized"], env=env)
    try:
        text = wait_log(cfg, WANTED, 20, receiver)
        check(WANTED in text, "[minimizado] el hub dejó la instalación pedida")
        time.sleep(3.0)
        text = read_log(cfg)
        check(ASKED not in text, "[minimizado] sin ventana no se pide el permiso (al iniciar sesión Windows lo bloquea)")
        check(receiver_window(receiver.pid) is None, "[minimizado] la ventana no existe")
        code = second_copy(exe, env)
        check(code == 0, f"[minimizado] otra copia pidió la ventana y salió (código {code})")
        text = wait_log(cfg, PAINTED, 30, receiver)
        check(PAINTED in text, "[minimizado] la ventana se creó y pintó")
        wait_until(lambda: receiver_window(receiver.pid) is not None, 10)
        hwnd = receiver_window(receiver.pid)
        check(hwnd is not None, "[minimizado] ventana del receptor encontrada")
        if hwnd is None:
            return
        time.sleep(1.0)
        if not foreground(hwnd):
            check(ASKED not in read_log(cfg), "[minimizado] sin el foco no se pide el permiso")
            if not bring_to_front(hwnd):
                check.inconclusive("[minimizado] Windows no deja al script poner la ventana delante mientras se usa otra")
                return
        asked_properly("minimizado", cfg, receiver, hwnd, check)
    finally:
        stop(receiver)
        print("---- receptor.log (minimizado) ----", flush=True)
        print(read_log(cfg), flush=True)


def case_ya_intentado(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "ya_intentado" / "config"
    seed_settings(cfg, vigem_setup_version=SETUP_VERSION)
    port = free_base_port()
    receiver = start_receiver(exe, env_for(port, cfg))
    try:
        text = wait_log(cfg, "no se instala solo", 30, receiver)
        check("no se instala solo: ya se intentó con este instalador" in text, "[ya_intentado] el log dice por qué no se instala")
        wait_log(cfg, PAINTED, 30, receiver)
        hwnd = receiver_window(receiver.pid)
        if hwnd is not None:
            bring_to_front(hwnd)
        time.sleep(2.0)
        check(ASKED not in read_log(cfg), "[ya_intentado] no se pide el permiso")
    finally:
        stop(receiver)
        print("---- receptor.log (ya_intentado) ----", flush=True)
        print(read_log(cfg), flush=True)


ALL = ["ventana", "minimizado", "ya_intentado"]


def main() -> int:
    args = list(sys.argv[1:])
    if not args or os.name != "nt":
        print(__doc__)
        return 2
    cases = ALL
    if "--cases" in args:
        cases = args[args.index("--cases") + 1].split(",")
        del args[args.index("--cases"):args.index("--cases") + 2]
    exe = Path(args[0]).resolve()
    root = Path(args[1]).resolve() if len(args) > 1 else Path(tempfile.mkdtemp(prefix="pepomote-driver-"))
    check = Checks()
    check.open = []

    def inconclusive(msg: str) -> None:
        print("SIN CONCLUIR " + msg, flush=True)
        check.open.append(msg)

    check.inconclusive = inconclusive
    for c in cases:
        print(f"==== {c} ====", flush=True)
        if c == "ventana":
            case_ventana(exe, root, check)
        elif c == "minimizado":
            case_minimizado(exe, root, check)
        elif c == "ya_intentado":
            case_ya_intentado(exe, root, check)
        else:
            check(False, f"caso desconocido: {c}")
    extra = f" ({len(check.open)} sin concluir: repetir sin usar el PC)" if check.open else ""
    print("RESULTADO:", ("OK" if check.fails == 0 else f"{check.fails} fallo(s)") + extra)
    return 0 if check.fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
