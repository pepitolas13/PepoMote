"""Receptor contra el Cemu REAL de este PC: qué le llega al móvil cuando Cemu
está abierto como administrador (y cuando se reabre normal).

La wiki de Cemu recomienda «Ejecutar como administrador»; con Cemu así y
PepoMote sin elevar, Windows no deja a PepoMote capturar la ventana GamePad
View ni escribirle. Este script arranca un receptor AISLADO (puerto de
prueba, configuración propia y `PEPOMOTE_CEMU_DIR` propio: la configuración
de Cemu de verdad no se toca), conecta un móvil simulado en modo Wii U con el
canal de pantalla y apunta qué le llega a lo largo del tiempo: estados
(texto), imágenes y avisos. A los `--text-at` segundos manda texto del
teclado del móvil. No abre ni cierra Cemu: eso lo hace quien prueba (abrirlo
como administrador pide el permiso de Windows).

Uso: python real_cemu_admin.py <PepoMote.exe> <carpeta> [--seconds 20]
     [--no-wgc] [--text-at 6] [--expect admin|frames|none]
Imprime la línea de tiempo y un resumen; con --expect sale con 1 si no se
cumple."""
import json
import os
import socket
import struct
import sys
import threading
import time
from pathlib import Path

from e2e_window_windows import free_base_port, read_log, receiver_env, seed_settings, start_receiver, stop, wait_log

MAGIC = b"PMPS"


def arg(name: str, default=None):
    if name in sys.argv:
        return sys.argv[sys.argv.index(name) + 1]
    return default


class Phone:
    """Móvil simulado: sesión de mando (latido y avisos) y canal de pantalla."""

    def __init__(self, port: int, t0: float) -> None:
        self.port = port
        self.t0 = t0
        self.events = []  # (segundo, tipo, detalle)
        self.lock = threading.Lock()
        s = socket.create_connection(("127.0.0.1", port), timeout=5)
        s.sendall((json.dumps({"m": "hello", "pv": 1, "name": "CemuAdmin", "model": "e2e", "code": "1234"}) + "\n").encode())
        self.ctl = s
        self.f = s.makefile("r", encoding="utf-8")
        self.ok = json.loads(self.f.readline())
        self.sid = self.ok.get("session_id")
        threading.Thread(target=self._beat, daemon=True).start()
        threading.Thread(target=self._read_ctl, daemon=True).start()
        s.sendall(b'{"m":"mode","mode":"cemu"}\n')

    def note(self, kind: str, detail: str) -> None:
        with self.lock:
            self.events.append((round(time.time() - self.t0, 1), kind, detail))

    def _beat(self) -> None:
        while True:
            time.sleep(0.8)
            try:
                self.ctl.sendall(b'{"m":"ping","t":1}\n')
            except OSError:
                return

    def _read_ctl(self) -> None:
        while True:
            try:
                line = self.f.readline()
            except (OSError, ValueError):
                return
            if not line:
                return
            try:
                msg = json.loads(line)
            except ValueError:
                continue
            if msg.get("m") == "notice":
                self.note("aviso", msg.get("text", ""))
            elif msg.get("m") == "mode":
                self.note("modo", str(msg.get("mode")))

    def type_text(self, text: str) -> None:
        self.note("teclado", text)
        self.ctl.sendall((json.dumps({"m": "text", "text": text}) + "\n").encode())

    def screen(self, seconds: float, text_at: float) -> None:
        sc = socket.create_connection(("127.0.0.1", self.port), timeout=5)
        sc.sendall((json.dumps({"m": "screen", "session_id": self.sid, "w": 854, "h": 480, "q": 70}) + "\n").encode())
        buf = b""
        while b"\n" not in buf:
            chunk = sc.recv(4096)
            if not chunk:
                break
            buf += chunk
        line, _, buf = buf.partition(b"\n")
        self.note("canal", line.decode("utf-8", "replace"))
        sc.settimeout(1.0)
        end = time.time() + seconds
        last_status = None
        frames_this_second = 0
        second = None
        typed = False
        while time.time() < end:
            if not typed and text_at >= 0 and time.time() - self.t0 >= text_at:
                typed = True
                self.type_text("e2e")
            try:
                chunk = sc.recv(65536)
            except socket.timeout:
                chunk = b""
            except OSError:
                break
            buf += chunk
            while len(buf) >= 9:
                if buf[:4] != MAGIC:
                    self.note("error", f"magic {buf[:4]!r}")
                    return
                kind = buf[4]
                length = struct.unpack_from("<I", buf, 5)[0]
                if len(buf) < 9 + length:
                    break
                payload = buf[9:9 + length]
                buf = buf[9 + length:]
                if kind == 1:
                    now = int(time.time() - self.t0)
                    if second != now:
                        if second is not None and frames_this_second:
                            self.note("imágenes", f"{frames_this_second} en el segundo {second}")
                        second, frames_this_second = now, 0
                    frames_this_second += 1
                    last_status = None
                    sc.sendall(b"\x01")
                elif kind == 2:
                    text = payload.decode("utf-8", "replace")
                    if text != last_status:
                        self.note("estado", text)
                        last_status = text
        if frames_this_second:
            self.note("imágenes", f"{frames_this_second} en el segundo {second}")
        sc.close()


def main() -> int:
    if len(sys.argv) < 3 or os.name != "nt":
        print(__doc__)
        return 2
    exe = Path(sys.argv[1]).resolve()
    root = Path(sys.argv[2]).resolve()
    seconds = float(arg("--seconds", "20"))
    text_at = float(arg("--text-at", "6"))
    expect = arg("--expect", "none")
    cfg = root / "config"
    seed_settings(cfg, tray_notice_seen=True)
    (root / "cemu").mkdir(parents=True, exist_ok=True)
    port = free_base_port()
    extra = {"PEPOMOTE_CEMU_DIR": str(root / "cemu")}
    if "--no-wgc" in sys.argv:
        extra["PEPOMOTE_NO_WGC"] = "1"
    env = receiver_env(port, cfg, **extra)
    print(f"receptor {exe} en el puerto {port} ({'PrintWindow' if '--no-wgc' in sys.argv else 'WGC'})", flush=True)
    receiver = start_receiver(exe, env)
    print(f"PID del receptor: {receiver.pid}", flush=True)
    phone = None
    try:
        wait_log(cfg, "primer fotograma pintado", 30, receiver)
        t0 = time.time()
        phone = Phone(port, t0)
        print("hola:", phone.ok, flush=True)
        phone.screen(seconds, text_at)
        time.sleep(1.0)
    finally:
        stop(receiver)
    print("---- línea de tiempo (s, tipo, detalle) ----")
    for e in phone.events if phone else []:
        print(e)
    log = read_log(cfg)
    print("---- receptor.log (pantalla, Cemu, teclado) ----")
    for l in log.splitlines():
        if any(k in l for k in ("Pantalla del GamePad", "administrador", "texto", "Cemu", "WGC", "Capture")):
            print(l)
    events = phone.events if phone else []
    statuses = [d for _t, k, d in events if k == "estado"]
    notices = [d for _t, k, d in events if k == "aviso"]
    frames = sum(int(d.split()[0]) for _t, k, d in events if k == "imágenes")
    print(f"RESUMEN: {frames} imágenes; estados {statuses}; avisos {notices}")
    if expect == "admin":
        ok = (any("administrador" in s for s in statuses) and any("administrador" in n for n in notices)
              and "abierto como administrador" in log and frames == 0)
    elif expect == "frames":
        ok = frames >= 1
    else:
        ok = True
    print("RESULTADO:", "OK" if ok else "FALLO", f"(se esperaba: {expect})")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
