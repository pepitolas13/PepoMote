"""Móvil simulado contra el receptor aislado dentro de un sway sin cabeza
(ver e2e_wayland.sh): empareja por código, envía reposo, un barrido de yaw,
un clic, rueda y texto, y comprueba en la salida de wev (la ventana a
pantalla completa) que el cursor se centró, recorrió lo que toca hacia la
izquierda, que llegó el botón izquierdo, la rueda con el signo de Wayland y
las teclas a, b e Intro.

Uso: python3 e2e_wayland.py --e2e-dir /tmp/pepomote-e2e --port 26771 --outputs outputs.json
     python3 e2e_wayland.py --parse-only fixtures/wev_ok.log   (solo el parser)
"""
import argparse, json, math, os, re, socket, struct, sys, threading, time

HOST = "127.0.0.1"
DT = 0.005          # 200 Hz, como el móvil
FLAG_QUAT = 1
BTN_A = 1 << 0

fails = 0
def check(cond, msg):
    global fails
    print(("OK   " if cond else "FAIL ") + msg, flush=True)
    if not cond:
        fails += 1

# ---------------------------------------------------------------- wev ------

RE_MOTION = re.compile(r"wl_pointer\]\s+motion:.*?x, y:\s*([-\d.]+),\s*([-\d.]+)")
RE_ENTER = re.compile(r"wl_pointer\]\s+enter:.*?x, y:\s*([-\d.]+),\s*([-\d.]+)")
RE_BUTTON = re.compile(r"wl_pointer\]\s+button:.*?button:\s*(\d+)\b.*?state:\s*(\d)")
RE_AXIS = re.compile(r"wl_pointer\]\s+axis:.*?axis:\s*0\b.*?value:\s*([-\d.]+)")
RE_AXIS_DISCRETE = re.compile(r"wl_pointer\]\s+axis_discrete:.*?axis:\s*0\b.*?discrete:\s*(-?\d+)")
RE_KEY = re.compile(r"wl_keyboard\]\s+key:.*?key:\s*(\d+);\s*state:\s*(\d)")
KEY_A, KEY_B, KEY_ENTER, BTN_LEFT = 30, 48, 28, 272

def parse_wev(text):
    """Eventos de wev en orden: [('motion', x, y)|('enter', x, y)|('button', code, state)|('axis', value)|('key', code, state)]"""
    ev = []
    for line in text.splitlines():
        m = RE_MOTION.search(line)
        if m:
            ev.append(("motion", float(m.group(1)), float(m.group(2)))); continue
        m = RE_ENTER.search(line)
        if m:
            ev.append(("enter", float(m.group(1)), float(m.group(2)))); continue
        m = RE_BUTTON.search(line)
        if m:
            ev.append(("button", int(m.group(1)), int(m.group(2)))); continue
        m = RE_AXIS.search(line)
        if m:
            ev.append(("axis", float(m.group(1)))); continue
        m = RE_AXIS_DISCRETE.search(line)
        if m:
            ev.append(("axis", float(m.group(1)) * 15.0)); continue
        m = RE_KEY.search(line)
        if m:
            ev.append(("key", int(m.group(1)), int(m.group(2)))); continue
    return ev

def rules(ev, W, H, sens, yaw_deg):
    """Devuelve la lista de (ok, descripción) de todas las comprobaciones."""
    out = []
    motions = [(e[1], e[2]) for e in ev if e[0] == "motion"]
    positions = [(e[1], e[2]) for e in ev if e[0] in ("motion", "enter")]
    out.append((len(motions) >= 20, f"al menos 20 motion (hay {len(motions)})"))
    if positions:
        x0, y0 = positions[0]
        out.append((abs(x0 - W / 2) <= 40 and abs(y0 - H / 2) <= 32,
                    f"primera posición cerca del centro ({x0:.0f},{y0:.0f}) ~ ({W/2:.0f},{H/2:.0f})"))
    else:
        out.append((False, "ninguna posición del puntero"))
    if motions:
        xe = W / 2 - W * yaw_deg / sens
        tol = 0.0375 * W
        xl, yl = motions[-1]
        out.append((abs(xl - xe) <= tol and abs(yl - H / 2) <= 32,
                    f"recorrido: última x {xl:.0f} ~ {xe:.0f}±{tol:.0f}, y {yl:.0f} ~ {H/2:.0f}"))
        creciente = [i for i in range(1, len(motions)) if motions[i][0] > motions[i - 1][0] + 8]
        out.append((not creciente, f"x nunca vuelve a la derecha entre motion consecutivos ({len(creciente)} saltos)"))
    pressed = [i for i, e in enumerate(ev) if e[0] == "button" and e[1] == BTN_LEFT and e[2] == 1]
    released = [i for i, e in enumerate(ev) if e[0] == "button" and e[1] == BTN_LEFT and e[2] == 0]
    out.append((bool(pressed) and bool(released) and released[-1] > pressed[0],
                f"botón izquierdo (272) pulsado y soltado ({len(pressed)}/{len(released)})"))
    axes = [e[1] for e in ev if e[0] == "axis"]
    out.append((bool(axes) and all(v < 0 for v in axes),
                f"rueda: {len(axes)} eventos de eje vertical, todos negativos (scroll arriba)"))
    keys = [(e[1], e[2]) for e in ev if e[0] == "key"]
    want = [(KEY_A, 1), (KEY_A, 0), (KEY_B, 1), (KEY_B, 0), (KEY_ENTER, 1), (KEY_ENTER, 0)]
    it = iter(keys)
    ordered = all(any(k == w for k in it) for w in want)
    out.append((ordered, f"teclas a(30), b(48), Intro(28) pulsadas y soltadas en orden (llegaron {keys})"))
    return out

def outputs_size(path):
    try:
        outs = json.load(open(path, encoding="utf-8"))
        for o in outs:
            r = o.get("rect") or {}
            if o.get("active", True) and r.get("width"):
                return int(r["width"]), int(r["height"])
    except Exception:
        pass
    return 1280, 720

def read_from(path, offset):
    try:
        with open(path, "rb") as f:
            f.seek(offset)
            return f.read().decode("utf-8", "replace")
    except FileNotFoundError:
        return ""

def tail(path, n):
    try:
        lines = open(path, encoding="utf-8", errors="replace").read().splitlines()
        return "\n".join(lines[-n:])
    except FileNotFoundError:
        return "(no existe)"

# ------------------------------------------------------------- receptor ----

def wait_port(port, secs):
    end = time.time() + secs
    while time.time() < end:
        try:
            socket.create_connection((HOST, port), timeout=0.5).close()
            return True
        except OSError:
            time.sleep(0.2)
    return False

def wait_log(path, needle, bad, secs):
    end = time.time() + secs
    while time.time() < end:
        text = read_from(path, 0)
        if needle in text:
            return True
        for b in bad:
            if b in text:
                print("FAIL el receptor no eligió Wayland:", b, "\n" + tail(path, 30))
                return False
        time.sleep(0.25)
    print("FAIL sin", repr(needle), "en", path, "\n" + tail(path, 30))
    return False

def hello(port):
    s = socket.create_connection((HOST, port), timeout=5)
    s.sendall((json.dumps({"m": "hello", "pv": 1, "code": "1234", "name": "e2e-wayland", "model": "e2e"}) + "\n").encode())
    f = s.makefile("r", encoding="utf-8")
    ok = json.loads(f.readline())
    def beat():
        while True:
            time.sleep(0.8)
            try: s.sendall(b'{"m":"ping","t":1}\n')
            except OSError: return
    threading.Thread(target=beat, daemon=True).start()
    return s, f, ok

class Phone:
    def __init__(self, session, port):
        self.session, self.port = session, port
        self.udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.seq, self.t_us = 0, 1_000_000

    def send(self, yaw_deg=0.0, gyro_z=0.0, buttons=0, scroll=0):
        self.seq += 1
        self.t_us += int(DT * 1e6)
        h = math.radians(yaw_deg) / 2
        b = bytearray(72)
        struct.pack_into("<I", b, 0, 0x31504D50); b[4] = 1; b[5] = FLAG_QUAT
        struct.pack_into("<I", b, 8, self.session); struct.pack_into("<I", b, 12, self.seq)
        struct.pack_into("<Q", b, 16, self.t_us)
        struct.pack_into("<4f", b, 24, math.cos(h), 0.0, 0.0, math.sin(h))
        struct.pack_into("<3f", b, 40, 0.0, 0.0, gyro_z)
        struct.pack_into("<3f", b, 52, 0.0, 0.0, 9.80665)
        struct.pack_into("<I", b, 64, buttons); b[68] = 0; b[69] = 90
        struct.pack_into("<h", b, 70, scroll)
        self.udp.sendto(bytes(b), (HOST, self.port))
        time.sleep(DT)

def drive(args):
    W, H = outputs_size(args.outputs)
    wev_log = os.path.join(args.e2e_dir, "wev.log")
    rx_log = os.path.join(args.e2e_dir, "config", "receptor.log")
    print(f"pantalla {W}x{H}; sens {args.sens}°; barrido {args.yaw}°", flush=True)

    check(wait_port(args.port, 30), f"receptor escuchando en {args.port}")
    if fails: return
    check(wait_log(rx_log, "Inyección: Wayland", ["Inyección: uinput", "sin inyector"], 25),
          "el receptor eligió el backend Wayland (puntero virtual)")
    if fails: return
    time.sleep(0.5)
    offset = os.path.getsize(wev_log) if os.path.exists(wev_log) else 0

    s, f, ok = hello(args.port)
    check(ok.get("m") == "ok" and ok.get("slot") == 0, f"hello por código → ok (slot {ok.get('slot')})")
    if fails: return
    s.sendall(b'{"m":"mode","mode":"pointer"}\n')
    phone = Phone(ok["session_id"], ok.get("udp_port", args.port))

    # A. quieto 1,0 s: el primer paquete centra el cursor; a los 0,3 s se congela
    for _ in range(200): phone.send(0.0, 0.0)
    # B. barrido 0 → +yaw° en 100 paquetes (0,5 s): a la IZQUIERDA (yaw = -θ en el motor)
    n = 100
    rate = math.radians(args.yaw) / (n * DT)
    for i in range(1, n + 1): phone.send(args.yaw * i / n, rate)
    # C. quieto 0,7 s en +yaw°: se congela donde está
    for _ in range(140): phone.send(args.yaw, 0.0)
    # D. clic: A pulsada 20 paquetes, soltada 20
    for _ in range(20): phone.send(args.yaw, 0.0, buttons=BTN_A)
    for _ in range(20): phone.send(args.yaw, 0.0)
    # E. rueda: +30 por paquete = wheel(120) = una muesca hacia arriba, 10 veces
    for _ in range(10): phone.send(args.yaw, 0.0, scroll=30)
    for _ in range(20): phone.send(args.yaw, 0.0)
    # F. texto por TCP (modo puntero → teclado virtual)
    s.sendall(b'{"m":"text","text":"ab\\n"}\n')
    for _ in range(100): phone.send(args.yaw, 0.0)

    # G. sondeo de wev hasta que todo pase (los eventos tardan en llegar al archivo)
    end = time.time() + 12
    results = []
    while True:
        ev = parse_wev(read_from(wev_log, offset))
        results = rules(ev, W, H, args.sens, args.yaw)
        if all(ok for ok, _ in results) or time.time() > end:
            break
        for _ in range(50): phone.send(args.yaw, 0.0)
    for ok, msg in results:
        check(ok, msg)
    if fails:
        print("\n--- últimas líneas de wev.log:\n" + tail(wev_log, 60))
        print("\n--- últimas líneas de receptor.log:\n" + tail(rx_log, 60))
    try:
        s.sendall(b'{"m":"bye"}\n'); s.close()
    except OSError:
        pass

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--e2e-dir", default="/tmp/pepomote-e2e")
    ap.add_argument("--port", type=int, default=26771)
    ap.add_argument("--outputs", default="")
    ap.add_argument("--sens", type=float, default=40.0)
    ap.add_argument("--yaw", type=float, default=10.0)
    ap.add_argument("--parse-only", default="")
    args = ap.parse_args()
    if args.parse_only:
        ev = parse_wev(open(args.parse_only, encoding="utf-8", errors="replace", newline="").read())
        for ok, msg in rules(ev, 1280, 720, args.sens, args.yaw):
            check(ok, msg)
    else:
        drive(args)
    print("RESULTADO:", "OK" if fails == 0 else f"FALLO ({fails})", flush=True)
    sys.exit(0 if fails == 0 else 1)

if __name__ == "__main__":
    main()
