"""e2e (solo Windows): la ventana del receptor aparece SIEMPRE.

Dos usuarios (1.13.2, Brasil y Perú, con el exe en una carpeta llamada
«PepoMote») veían «se abre un milisegundo y se cierra; solo está en el
Administrador de tareas / en la bandeja». Aquí se comprueba cada causa con
receptores AISLADOS (puertos y configuración propios; nunca los de defecto):

- carpeta: ventana pintada, se cierra con la X (se esconde en la bandeja),
  encima hay otra ventana titulada «PepoMote» (lo que es el Explorador de
  Windows 10 abierto en esa carpeta), y se vuelve a abrir el exe: la ventana
  tiene que volver y la otra ventana no se toca (antes la desmaximizaba y la
  nuestra no volvía nunca). `carpeta_min`: lo mismo con la ventana minimizada.
- aviso: la primera X enseña el aviso de la bandeja (la ventana sigue a la
  vista), la segunda esconde y queda apuntado; sin bandeja la X minimiza.
- normal: la sonda de OpenGL dice que va bien y pinta OpenGL; el segundo
  arranque ya no lanza sonda (lo recuerda por la huella de la gráfica).
- sin_opengl: el OpenGL 1.1 de Microsoft de verdad (PEPOMOTE_FAKE_NO_OPENGL,
  lo que tiene un PC sin driver): la ventana nace con Direct3D 12.
- sonda_colgada: la sonda se cuelga en OpenGL; a los 3 s se la mata y pinta
  Direct3D, sin sondas vivas al final.
- reintento: sin sonda y sin OpenGL: OpenGL falla y Direct3D pinta en el
  mismo proceso.
- marca: el arranque anterior no llegó a pintar con OpenGL: directo a Direct3D.
- nada: ni OpenGL ni Direct3D: sale el mensaje de error (se cierra desde
  aquí) y el receptor termina con 2.
- colgada: la ventana de verdad se cuelga antes de pintar; al abrir otra
  copia, la colgada le deja el sitio y la nueva pinta con Direct3D.
- En todos: ninguna ventana visible de la sonda en ningún momento.

Uso: python e2e_window_windows.py <PepoMote.exe> [directorio] [--cases a,b] [--old]
`--old` = el exe es anterior al arreglo (solo tiene sentido `carpeta`: ahí
tiene que FALLAR). Sale con 0 si pasa y con 1 si falla."""
import ctypes
import json
import os
import socket
import subprocess
import sys
import tempfile
import threading
import time
from ctypes import wintypes
from pathlib import Path

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

WM_CLOSE = 0x0010
SW_MINIMIZE = 6
PROCESS_QUERY_LIMITED_INFORMATION = 0x1000

EnumWindowsProc = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
user32.EnumWindows.argtypes = [EnumWindowsProc, wintypes.LPARAM]
user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowThreadProcessId.restype = wintypes.DWORD
user32.GetWindowTextW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetClassNameW.argtypes = [wintypes.HWND, wintypes.LPWSTR, ctypes.c_int]
for name in ("IsWindowVisible", "IsIconic", "IsZoomed", "IsWindow"):
    getattr(user32, name).argtypes = [wintypes.HWND]
    getattr(user32, name).restype = wintypes.BOOL
user32.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
user32.ShowWindow.argtypes = [wintypes.HWND, ctypes.c_int]
user32.GetForegroundWindow.restype = wintypes.HWND
user32.GetWindowRect.argtypes = [wintypes.HWND, ctypes.POINTER(wintypes.RECT)]
user32.FindWindowExW.argtypes = [wintypes.HWND, wintypes.HWND, wintypes.LPCWSTR, wintypes.LPCWSTR]
user32.FindWindowExW.restype = wintypes.HWND
BM_CLICK = 0x00F5
# Ventana auxiliar de winit para sus mensajes: «visible» para Windows, pero
# de 0×0, transparente y sin botón; no se ve nunca
WINIT_HELPER = "Winit Thread Event Target"
kernel32.OpenProcess.argtypes = [wintypes.DWORD, wintypes.BOOL, wintypes.DWORD]
kernel32.OpenProcess.restype = wintypes.HANDLE
kernel32.QueryFullProcessImageNameW.argtypes = [wintypes.HANDLE, wintypes.DWORD, wintypes.LPWSTR, ctypes.POINTER(wintypes.DWORD)]
kernel32.CloseHandle.argtypes = [wintypes.HANDLE]


def top_windows() -> list:
    """(hwnd, pid, clase, título, visible) de todas las ventanas de primer nivel."""
    out = []

    @EnumWindowsProc
    def cb(h, _l):
        pid = wintypes.DWORD(0)
        user32.GetWindowThreadProcessId(h, ctypes.byref(pid))
        title = ctypes.create_unicode_buffer(256)
        user32.GetWindowTextW(h, title, 256)
        cls = ctypes.create_unicode_buffer(256)
        user32.GetClassNameW(h, cls, 256)
        out.append((h, pid.value, cls.value, title.value, bool(user32.IsWindowVisible(h))))
        return True

    user32.EnumWindows(cb, 0)
    return out


def has_area(h) -> bool:
    r = wintypes.RECT()
    return bool(user32.GetWindowRect(h, ctypes.byref(r))) and r.right > r.left and r.bottom > r.top


def image_of(pid: int) -> str:
    h = kernel32.OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, False, pid)
    if not h:
        return ""
    try:
        buf = ctypes.create_unicode_buffer(1024)
        size = wintypes.DWORD(1024)
        if kernel32.QueryFullProcessImageNameW(h, 0, buf, ctypes.byref(size)):
            return buf.value
        return ""
    finally:
        kernel32.CloseHandle(h)


def receiver_window(pid: int):
    """La ventana principal del receptor (clase de winit, título «PepoMote»)."""
    for h, p, cls, title, _vis in top_windows():
        if p == pid and title == "PepoMote" and cls == "Window Class":
            return h
    return None


def wait_until(pred, seconds: float, step: float = 0.05) -> bool:
    t0 = time.time()
    while time.time() - t0 < seconds:
        if pred():
            return True
        time.sleep(step)
    return bool(pred())


def free_base_port() -> int:
    # TCP p y UDP p (móvil), UDP p-1 (DSU), UDP p+1 (instancia única)
    for port in range(29771, 30771, 10):
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


def receiver_env(port: int, cfg: Path, tray: bool = False, **extra: str) -> dict:
    env = os.environ.copy()
    for k in list(env):
        if k.startswith("PEPOMOTE_"):
            del env[k]
    env.update(
        PEPOMOTE_PORT=str(port), PEPOMOTE_DSU_PORT=str(port - 1), PEPOMOTE_CONFIG_DIR=str(cfg),
        PEPOMOTE_NO_UPDATE_CHECK="1", PEPOMOTE_NO_DRIVER_SETUP="1",
        PEPOMOTE_ASSUME_EMULATOR_CLOSED="1", PEPOMOTE_PAIR_CODE="1234",
    )
    if not tray:
        env["PEPOMOTE_NO_TRAY"] = "1"
    env.update(extra)
    return env


def seed_settings(cfg: Path, **fields) -> None:
    """settings.json mínimo válido (sens_deg y abs_mode no tienen valor por
    defecto: sin ellos el receptor tiraría todo el archivo)."""
    cfg.mkdir(parents=True, exist_ok=True)
    data = {"sens_deg": 40.0, "abs_mode": True, "update_check": False}
    data.update(fields)
    (cfg / "settings.json").write_text(json.dumps(data), encoding="utf-8")


def read_settings(cfg: Path) -> dict:
    try:
        return json.loads((cfg / "settings.json").read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


def read_log(cfg: Path) -> str:
    log = cfg / "receptor.log"
    return log.read_text(encoding="utf-8", errors="replace") if log.exists() else ""


def wait_log(cfg: Path, needle: str, seconds: float, proc=None) -> str:
    t0 = time.time()
    text = ""
    while time.time() - t0 < seconds:
        text = read_log(cfg)
        if needle in text or (proc is not None and proc.poll() is not None):
            break
        time.sleep(0.1)
    return read_log(cfg)


def stop(proc, grace: float = 0.0) -> None:
    if proc is None:
        return
    if grace > 0:
        try:
            proc.wait(timeout=grace)
        except subprocess.TimeoutExpired:
            pass
    if proc.poll() is None:
        proc.kill()
        try:
            proc.wait(timeout=30)
        except subprocess.TimeoutExpired:
            pass


class Checks:
    def __init__(self) -> None:
        self.fails = 0

    def __call__(self, cond: bool, msg: str) -> None:
        print(("OK   " if cond else "FAIL ") + msg, flush=True)
        if not cond:
            self.fails += 1


class ProbeWatch:
    """Vigila, mientras dura un caso, que ninguna ventana VISIBLE sea de otro
    proceso con el mismo exe que el receptor (la sonda no se deja ver nunca).
    Apunta también los PID de esas copias para comprobar que no queda
    ninguna viva al final."""

    def __init__(self, exe: Path, receiver_pids) -> None:
        self.exe = str(exe).lower()
        self.receiver_pids = receiver_pids
        self.visible = []
        self.pids = set()
        self._stop = threading.Event()
        self._t = threading.Thread(target=self._run, daemon=True)
        self._t.start()

    def _run(self) -> None:
        images = {}
        while not self._stop.is_set():
            for h, pid, cls, title, vis in top_windows():
                if pid in self.receiver_pids():
                    continue
                if pid not in images:
                    images[pid] = image_of(pid).lower()
                if images[pid] == self.exe:
                    self.pids.add(pid)
                    if vis and cls != WINIT_HELPER and has_area(h):
                        self.visible.append((pid, cls, title))
            time.sleep(0.02)

    def finish(self):
        self._stop.set()
        self._t.join(timeout=5)
        alive = [p for p in self.pids if image_of(p).lower() == self.exe]
        return self.visible, alive


DECOY = r'''
import sys, tkinter as tk
r = tk.Tk()
r.title("PepoMote")
r.geometry("600x400+80+80")
r.update()
r.state("zoomed")
r.lift()
r.focus_force()
r.after(int(sys.argv[1]), r.destroy)
r.mainloop()
'''


def open_decoy(seconds: int = 60):
    """Una ventana ajena titulada «PepoMote», maximizada y encima de todo lo
    demás: lo que es el Explorador de Windows 10 en una carpeta con ese nombre."""
    proc = subprocess.Popen([sys.executable, "-c", DECOY, str(seconds * 1000)])
    found = []

    def find():
        found.clear()
        found.extend(h for h, pid, _c, title, vis in top_windows() if pid == proc.pid and title == "PepoMote" and vis)
        return bool(found) and bool(user32.IsZoomed(found[0]))

    ok = wait_until(find, 10)
    return proc, (found[0] if ok else None)


def start_receiver(exe: Path, env: dict):
    return subprocess.Popen([str(exe)], env=env)


def second_copy(exe: Path, env: dict):
    try:
        out = subprocess.run([str(exe)], env=env, timeout=30)
        return out.returncode
    except subprocess.TimeoutExpired:
        return None


# ---------------------------------------------------------------- casos


def case_carpeta(exe: Path, root: Path, check: Checks, old: bool, minimized: bool) -> None:
    tag = "carpeta_min" if minimized else "carpeta"
    cfg = root / tag / "config"
    seed_settings(cfg, tray_notice_seen=True)
    port = free_base_port()
    env = receiver_env(port, cfg, tray=True, PEPOMOTE_SMOKE="30000")
    print(f"[{tag}] receptor aislado en el puerto {port}", flush=True)
    receiver = start_receiver(exe, env)
    decoy = None
    try:
        text = wait_log(cfg, "primer fotograma pintado", 30, receiver)
        check("primer fotograma pintado" in text, f"[{tag}] la ventana pintó")
        hwnd = None
        wait_until(lambda: receiver_window(receiver.pid) is not None and user32.IsWindowVisible(receiver_window(receiver.pid)), 10)
        hwnd = receiver_window(receiver.pid)
        check(hwnd is not None, f"[{tag}] ventana del receptor encontrada")
        if hwnd is None:
            return
        if minimized:
            user32.ShowWindow(hwnd, SW_MINIMIZE)
            check(wait_until(lambda: user32.IsIconic(hwnd), 5), f"[{tag}] minimizada")
        else:
            user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)
            check(wait_until(lambda: not user32.IsWindowVisible(hwnd), 5), f"[{tag}] la X la escondió en la bandeja")
        decoy, decoy_hwnd = open_decoy()
        check(decoy_hwnd is not None, f"[{tag}] ventana ajena «PepoMote» maximizada encima")
        code = second_copy(exe, env)
        check(code == 0, f"[{tag}] la segunda copia entregó el testigo y salió (código {code})")
        shown = wait_until(lambda: user32.IsWindowVisible(hwnd) and not user32.IsIconic(hwnd), 3)
        decoy_ok = decoy_hwnd is not None and bool(user32.IsZoomed(decoy_hwnd))
        if old:
            # Minimizada, la 1.13.2 sí volvía (eframe sigue repintando una
            # ventana minimizada y aplica sus órdenes); escondida, no
            if not minimized:
                check(not shown, f"[{tag}] (exe anterior) la ventana NO vuelve: el fallo de los usuarios, reproducido")
            check(not decoy_ok, f"[{tag}] (exe anterior) la ventana ajena salió desmaximizada")
        else:
            check(shown, f"[{tag}] la ventana volvió a la vista en menos de 3 s")
            check(decoy_ok, f"[{tag}] la ventana ajena «PepoMote» sigue maximizada (no se tocó)")
        text = read_log(cfg)
        check("Otra copia de PepoMote pide mostrar la ventana" in text, f"[{tag}] la primera copia recibió la petición")
    finally:
        stop(decoy)
        stop(receiver, grace=0 if old else 35)
        print(f"---- receptor.log ({tag}) ----", flush=True)
        print(read_log(cfg), flush=True)


def case_aviso(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "aviso" / "config"
    seed_settings(cfg)
    port = free_base_port()
    env = receiver_env(port, cfg, tray=True, PEPOMOTE_SMOKE="30000")
    print(f"[aviso] receptor aislado en el puerto {port}", flush=True)
    receiver = start_receiver(exe, env)
    try:
        wait_log(cfg, "primer fotograma pintado", 30, receiver)
        wait_until(lambda: receiver_window(receiver.pid) is not None, 10)
        hwnd = receiver_window(receiver.pid)
        check(hwnd is not None, "[aviso] ventana del receptor encontrada")
        if hwnd is None:
            return
        user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)
        text = wait_log(cfg, "aviso de la bandeja", 5, receiver)
        check("aviso de la bandeja" in text, "[aviso] la primera X enseñó el aviso (log)")
        time.sleep(1.0)
        check(bool(user32.IsWindowVisible(hwnd)), "[aviso] con el aviso, la ventana sigue a la vista")
        user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)
        check(wait_until(lambda: not user32.IsWindowVisible(hwnd), 5), "[aviso] la segunda X la escondió")
        check(wait_until(lambda: read_settings(cfg).get("tray_notice_seen") is True, 5),
              "[aviso] settings.json apunta que el aviso ya se vio")
    finally:
        stop(receiver)
    # Sin bandeja: la X minimiza, nunca esconde donde no se puede volver
    cfg = root / "aviso_sin_bandeja" / "config"
    seed_settings(cfg, tray_notice_seen=True)
    port = free_base_port()
    env = receiver_env(port, cfg, tray=False, PEPOMOTE_SMOKE="30000")
    receiver = start_receiver(exe, env)
    try:
        wait_log(cfg, "primer fotograma pintado", 30, receiver)
        wait_until(lambda: receiver_window(receiver.pid) is not None, 10)
        hwnd = receiver_window(receiver.pid)
        if hwnd is not None:
            user32.PostMessageW(hwnd, WM_CLOSE, 0, 0)
            check(wait_until(lambda: user32.IsIconic(hwnd), 5), "[aviso] sin bandeja la X minimiza")
        else:
            check(False, "[aviso] ventana del receptor (sin bandeja) encontrada")
    finally:
        stop(receiver)


def run_smoke(exe: Path, cfg: Path, tag: str, check: Checks, seconds: float = 40, **extra: str):
    """Un receptor en modo humo; devuelve (código, log, ventanas de sonda
    visibles, sondas vivas, segundos hasta el primer fotograma)."""
    port = free_base_port()
    env = receiver_env(port, cfg, PEPOMOTE_SMOKE="1500", **extra)
    receiver = start_receiver(exe, env)
    watch = ProbeWatch(exe, lambda: {receiver.pid})
    t0 = time.time()
    first = None
    try:
        while time.time() - t0 < seconds and receiver.poll() is None:
            if first is None and "primer fotograma pintado" in read_log(cfg):
                first = time.time() - t0
            time.sleep(0.05)
        code = receiver.poll()
    finally:
        stop(receiver)
    visible, alive = watch.finish()
    text = read_log(cfg)
    print(f"---- receptor.log ({tag}) ----", flush=True)
    print(text, flush=True)
    check(not visible, f"[{tag}] la sonda no enseñó ninguna ventana ({visible[:3]})")
    check(not alive, f"[{tag}] ninguna sonda sigue viva ({alive})")
    check("PÁNICO" not in text or tag in ("nada",), f"[{tag}] sin pánicos")
    return code, text, first


def painted_with(text: str, what: str) -> bool:
    return any("primer fotograma pintado" in l and what in l for l in text.splitlines())


def remembered(cfg: Path):
    """Lo recordado para la última gráfica (`window_renderer`: lista de
    {fingerprint, renderer}, la más reciente al final)."""
    entries = read_settings(cfg).get("window_renderer") or []
    return entries[-1] if entries else None


def case_normal(exe: Path, root: Path, check: Checks) -> None:
    """Sin trucos: lo que decida la sonda en ESTE equipo. Con GPU, OpenGL;
    en un Windows sin GPU (el runner de GitHub, un PC sin driver), el OpenGL
    1.1 de Microsoft no sirve y tiene que pintar Direct3D 12 por WARP."""
    cfg = root / "normal" / "config"
    seed_settings(cfg)
    code, text, first = run_smoke(exe, cfg, "normal", check)
    check(code == 0, f"[normal] el humo salió con 0 (salió con {code})")
    gl_ok = "sonda de OpenGL bien" in text
    check(gl_ok or "sonda de OpenGL mal" in text or "no contesta" in text, "[normal] la sonda de OpenGL contestó")
    want, key = ("OpenGL", "opengl") if gl_ok else ("Direct3D 12", "direct3d")
    line = next((l for l in text.splitlines() if "primer fotograma pintado" in l), "")
    print(f"[normal] en este equipo: {line.split('pintado', 1)[-1].strip()}", flush=True)
    check(painted_with(text, want), f"[normal] pintó con {want}")
    saved = remembered(cfg)
    check(bool(saved) and saved.get("renderer") == key, f"[normal] recuerda {want} para esta gráfica ({saved})")
    # Segundo arranque, misma gráfica: sin sonda
    (cfg / "receptor.log").unlink(missing_ok=True)
    code, text, first = run_smoke(exe, cfg, "normal-2", check)
    check(code == 0, f"[normal-2] el humo salió con 0 (salió con {code})")
    check("recordado para esta gráfica" in text and "sonda de OpenGL" not in text, "[normal-2] sin sonda: lo recordaba")
    check(painted_with(text, want), f"[normal-2] pintó con {want}")


def case_sin_opengl(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "sin_opengl" / "config"
    seed_settings(cfg)
    code, text, first = run_smoke(exe, cfg, "sin_opengl", check, PEPOMOTE_FAKE_NO_OPENGL="1")
    check(code == 0, f"[sin_opengl] el humo salió con 0 (salió con {code})")
    check("sonda de OpenGL mal" in text and "opengl 2.0" in text.lower(),
          "[sin_opengl] la sonda trajo el fallo real del OpenGL 1.1 de Windows")
    check(painted_with(text, "Direct3D 12"), "[sin_opengl] pintó con Direct3D 12")
    check("falló antes de pintar" not in text, "[sin_opengl] sin intento OpenGL en el receptor")
    saved = remembered(cfg)
    check(bool(saved) and saved.get("renderer") == "direct3d", f"[sin_opengl] recuerda Direct3D ({saved})")


def case_sonda_colgada(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "sonda_colgada" / "config"
    seed_settings(cfg)
    code, text, first = run_smoke(exe, cfg, "sonda_colgada", check, PEPOMOTE_FAKE_GL_HANG="1")
    check(code == 0, f"[sonda_colgada] el humo salió con 0 (salió con {code})")
    check("no contesta" in text, "[sonda_colgada] la sonda no contestó y se apuntó")
    check(painted_with(text, "Direct3D 12"), "[sonda_colgada] pintó con Direct3D 12")
    check(first is not None and first <= 6.0, f"[sonda_colgada] primer fotograma en {first} s (≤ 6 s)")


def case_reintento(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "reintento" / "config"
    seed_settings(cfg)
    code, text, first = run_smoke(exe, cfg, "reintento", check,
                                  PEPOMOTE_NO_WINDOW_PROBE="1", PEPOMOTE_FAKE_NO_OPENGL="1")
    check(code == 0, f"[reintento] el humo salió con 0 (salió con {code})")
    check("falló antes de pintar" in text and "opengl 2.0" in text.lower(), "[reintento] OpenGL falló en el receptor (fallo real)")
    check(painted_with(text, "intento 2") and painted_with(text, "Direct3D 12"), "[reintento] Direct3D pintó en el intento 2")


def case_marca(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "marca" / "config"
    seed_settings(cfg)
    # Primero un arranque normal para saber la huella de este PC
    code, text, _ = run_smoke(exe, cfg, "marca-previo", check)
    saved = remembered(cfg) or {}
    fp = saved.get("fingerprint", "")
    check(bool(fp), "[marca] el arranque previo dejó la huella de la gráfica")
    data = read_settings(cfg)
    # Lo que queda si el arranque anterior se colgó (o lo cerraron a la
    # fuerza) intentando pintar con OpenGL
    data["window_pending"] = {"fingerprint": fp, "renderer": "opengl"}
    (cfg / "settings.json").write_text(json.dumps(data), encoding="utf-8")
    (cfg / "receptor.log").unlink(missing_ok=True)
    code, text, _ = run_smoke(exe, cfg, "marca", check)
    check(code == 0, f"[marca] el humo salió con 0 (salió con {code})")
    check("no llegó a pintar" in text, "[marca] apuntó que el arranque anterior no pintó")
    check(painted_with(text, "Direct3D 12"), "[marca] pintó con Direct3D 12")


def find_error_box(pid: int):
    for h, p, cls, title, vis in top_windows():
        if p == pid and cls == "#32770" and vis:
            return h, title
    return None


def case_nada(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "nada" / "config"
    seed_settings(cfg)
    port = free_base_port()
    env = receiver_env(port, cfg, PEPOMOTE_FAKE_NO_OPENGL="1", PEPOMOTE_FAKE_NO_DIRECT3D="1")
    receiver = start_receiver(exe, env)
    box = None
    try:
        wait_until(lambda: find_error_box(receiver.pid) is not None or receiver.poll() is not None, 30)
        box = find_error_box(receiver.pid)
        check(box is not None, f"[nada] salió el mensaje de error ({box[1] if box else '-'})")
        if box is not None:
            check("PepoMote" in box[1], "[nada] el mensaje lleva el título de PepoMote")
            # Se cierra como lo haría una persona: pulsando su botón
            button = user32.FindWindowExW(box[0], None, "Button", None)
            user32.PostMessageW(button, BM_CLICK, 0, 0)
        try:
            code = receiver.wait(timeout=10)
        except subprocess.TimeoutExpired:
            code = None
        check(code == 2, f"[nada] el receptor terminó con 2 (terminó con {code})")
    finally:
        stop(receiver)
    text = read_log(cfg)
    print("---- receptor.log (nada) ----", flush=True)
    print(text, flush=True)
    check("Direct3D 12" in text and "falló" in text, "[nada] el log dice qué falló")


def case_colgada(exe: Path, root: Path, check: Checks) -> None:
    cfg = root / "colgada" / "config"
    seed_settings(cfg)
    port = free_base_port()
    env_hang = receiver_env(port, cfg, PEPOMOTE_FAKE_GL_HANG="ventana", PEPOMOTE_NO_WINDOW_PROBE="1")
    env_ok = receiver_env(port, cfg, PEPOMOTE_SMOKE="1500")
    first = start_receiver(exe, env_hang)
    second = None
    try:
        wait_log(cfg, "PEPOMOTE_FAKE_GL_HANG", 20, first)
        # La colgada solo deja el sitio pasados 5 s desde su intento de
        # ventana (una ventana lenta pero sana no se sacrifica)
        time.sleep(6.0)
        second = start_receiver(exe, env_ok)
        try:
            code = second.wait(timeout=40)
        except subprocess.TimeoutExpired:
            code = None
        check(code == 0, f"[colgada] la copia nueva pintó y salió con 0 (salió con {code})")
        check(wait_until(lambda: first.poll() is not None, 10), "[colgada] la copia colgada desapareció")
    finally:
        stop(second)
        stop(first)
    text = read_log(cfg)
    print("---- receptor.log (colgada) ----", flush=True)
    print(text, flush=True)
    check("le dejo el sitio" in text, "[colgada] la colgada apuntó que dejaba el sitio")
    check(painted_with(text, "Direct3D 12"), "[colgada] la nueva pintó con Direct3D 12")


ALL = ["carpeta", "carpeta_min", "aviso", "normal", "sin_opengl", "sonda_colgada", "reintento", "marca", "nada", "colgada"]


def main() -> int:
    args = [a for a in sys.argv[1:]]
    if not args or os.name != "nt":
        print(__doc__)
        return 2
    old = "--old" in args
    cases = ALL
    if "--cases" in args:
        cases = args[args.index("--cases") + 1].split(",")
        del args[args.index("--cases"):args.index("--cases") + 2]
    args = [a for a in args if a != "--old"]
    exe = Path(args[0]).resolve()
    root = Path(args[1]).resolve() if len(args) > 1 else Path(tempfile.mkdtemp(prefix="pepomote-window-"))
    check = Checks()
    for c in cases:
        print(f"==== {c} ====", flush=True)
        if c == "carpeta":
            case_carpeta(exe, root, check, old, minimized=False)
        elif c == "carpeta_min":
            case_carpeta(exe, root, check, old, minimized=True)
        elif c == "aviso":
            case_aviso(exe, root, check)
        elif c == "normal":
            case_normal(exe, root, check)
        elif c == "sin_opengl":
            case_sin_opengl(exe, root, check)
        elif c == "sonda_colgada":
            case_sonda_colgada(exe, root, check)
        elif c == "reintento":
            case_reintento(exe, root, check)
        elif c == "marca":
            case_marca(exe, root, check)
        elif c == "nada":
            case_nada(exe, root, check)
        elif c == "colgada":
            case_colgada(exe, root, check)
        else:
            check(False, f"caso desconocido: {c}")
    print("RESULTADO:", "OK" if check.fails == 0 else f"{check.fails} fallo(s)")
    return 0 if check.fails == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
