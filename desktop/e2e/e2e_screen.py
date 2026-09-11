"""e2e del canal de pantalla (doble pantalla del GamePad): conecta un mando,
pone modo Wii U, abre el canal `screen` y recibe tramas durante unos
segundos; guarda el último JPEG para verlo. Con Cemu abierto y su ventana
GamePad View a la vista llegan imágenes reales; sin Cemu, solo tramas de
estado (y se comprueba que lo diga).
Uso: python e2e_screen.py <segundos> <salida.jpg>
Entorno: PEPOMOTE_PORT (26761), PEPOMOTE_PAIR_CODE=1234 en el receptor,
o PEPOMOTE_TOKEN para usar el token real."""
import json, os, socket, struct, sys, threading, time

HOST = "127.0.0.1"; PORT = int(os.environ.get("PEPOMOTE_PORT", "26761"))
SECONDS = float(sys.argv[1]) if len(sys.argv) > 1 else 5.0
OUT = sys.argv[2] if len(sys.argv) > 2 else "screen_last.jpg"
MAGIC = b"PMPS"

fails = 0
def check(cond, msg):
    global fails
    print(("OK   " if cond else "FAIL ") + msg, flush=True)
    if not cond: fails += 1

def hello(extra):
    s = socket.create_connection((HOST, PORT), timeout=5)
    msg = {"m": "hello", "pv": 1, "name": extra.pop("name", "e2e"), "model": "e2e"}
    msg.update(extra)
    s.sendall((json.dumps(msg) + "\n").encode())
    f = s.makefile("r", encoding="utf-8")
    ok = json.loads(f.readline())
    def beat():
        while True:
            time.sleep(0.8)
            try: s.sendall(b'{"m":"ping","t":1}\n')
            except OSError: return
    threading.Thread(target=beat, daemon=True).start()
    return s, f, ok

def readmsg(f, m, timeout=3.0):
    end = time.time() + timeout
    while time.time() < end:
        try:
            line = f.readline()
        except (socket.timeout, OSError):
            return None
        if not line: break
        msg = json.loads(line)
        if msg.get("m") == m: return msg
    return None

cred = {"token": os.environ["PEPOMOTE_TOKEN"]} if os.environ.get("PEPOMOTE_TOKEN") else {"code": "1234"}
s1, f1, ok1 = hello(dict(cred, name="PantallaE2E"))
check(ok1.get("m") == "ok", f"mando conectado: {ok1}")
sid = ok1["session_id"]
if ok1.get("slot") == 0:
    s1.sendall(b'{"m":"mode","mode":"cemu"}\n'); r = readmsg(f1, "mode")
    check(r and r.get("mode") == "cemu", f"modo cemu ({r})")
else:
    print("(no soy el jugador 1: el modo lo decide otro móvil; sigo)")

# canal de pantalla: sesión mala → err
bad = socket.create_connection((HOST, PORT), timeout=5)
bad.sendall(b'{"m":"screen","session_id":12345,"w":854,"h":480}\n')
line = bad.makefile("r", encoding="utf-8").readline()
check('"bad_session"' in line, f"sesión desconocida rechazada: {line.strip()}")
bad.close()

sc = socket.create_connection((HOST, PORT), timeout=5)
sc.sendall((json.dumps({"m": "screen", "session_id": sid, "w": 854, "h": 480, "q": 70}) + "\n").encode())
buf = b""
while b"\n" not in buf:
    chunk = sc.recv(4096)
    if not chunk: break
    buf += chunk
line, _, buf = buf.partition(b"\n")
ok = json.loads(line)
check(ok.get("m") == "screen" and ok.get("ok") is True, f"canal abierto: {ok}")

frames = 0; status = []; keep = 0; last = None; sizes = []; t_first = None
sc.settimeout(3.0)
end = time.time() + SECONDS
while time.time() < end:
    while len(buf) < 9:
        try:
            chunk = sc.recv(65536)
        except socket.timeout:
            chunk = b""
        if not chunk: break
        buf += chunk
    if len(buf) < 9: break
    if buf[:4] != MAGIC:
        check(False, f"magic incorrecto: {buf[:4]!r}"); break
    kind = buf[4]; length = struct.unpack_from("<I", buf, 5)[0]
    while len(buf) < 9 + length:
        try:
            chunk = sc.recv(65536)
        except socket.timeout:
            chunk = b""
        if not chunk: break
        buf += chunk
    payload = buf[9:9 + length]; buf = buf[9 + length:]
    if kind == 1:
        frames += 1; last = payload; sizes.append(len(payload))
        if t_first is None: t_first = time.time()
        sc.sendall(b"\x01")  # confirmación: siguiente imagen
    elif kind == 2:
        status.append(payload.decode("utf-8", "replace"))
    elif kind == 3:
        keep += 1
sc.close()
elapsed = (time.time() - t_first) if t_first else 0
print(f"tramas: {frames} imágenes, {len(status)} estados, {keep} keepalive; estados: {status[-3:]}")
if frames:
    print(f"fps medios: {frames / max(elapsed, 1e-6):.1f}, tamaño medio {sum(sizes) // len(sizes)} B, máx {max(sizes)} B")
    check(last[:2] == b"\xff\xd8" and last[-2:] == b"\xff\xd9", "el último JPEG está bien formado (SOI/EOI)")
    open(OUT, "wb").write(last); print("guardado", OUT)
    check(frames >= SECONDS * 5, f"ritmo razonable (≥5 fps): {frames / max(elapsed, 1e-6):.1f} fps")
else:
    check(any("Cemu" in s or "GamePad" in s for s in status), "sin ventana: el receptor explica por qué (estado)")
    check(keep >= 1 or len(status) >= 1, "sin imágenes el canal sigue vivo (keepalive/estado)")
try:
    s1.sendall(b'{"m":"bye"}\n'); s1.close()
except OSError:
    pass
print("FAILS:", fails); sys.exit(1 if fails else 0)
