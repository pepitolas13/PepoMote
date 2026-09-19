"""Móvil simulado contra el receptor aislado dentro de un sway sin cabeza
(ver e2e_wayland.sh): empareja por código, envía reposo, un barrido de yaw,
un clic, la cruceta ← y →, rueda y texto, y comprueba en la salida de wev (la
ventana a pantalla completa) que el cursor se centró, recorrió lo que toca
hacia la izquierda, que llegó el botón izquierdo, las teclas XF86Back y
XF86Forward (atrás/adelante del navegador), la rueda con el signo de Wayland
y las teclas a, b e Intro.

Tambien comprueba el texto UNICODE: manda ñ, un emoji y el euro, que el
receptor teclea dandoles una tecla de repuesto y volviendo a subir el keymap
(ver input/linux_wayland.rs), y verifica en wev que llegaron con su utf8 y
que el ASCII sigue funcionando DESPUES de dos cambios de keymap.

Uso: python3 e2e_wayland.py --e2e-dir /tmp/pepomote-e2e --port 26771 --outputs outputs.json
     python3 e2e_wayland.py --parse-only fixtures/wev_ok.log   (solo el parser)
     python3 e2e_wayland.py --parse-only fixtures/wev_unicode.log --expect-text "abñ😀€ab"
"""
import argparse, json, math, os, re, socket, struct, sys, threading, time

HOST = "127.0.0.1"
DT = 0.005          # 200 Hz, como el móvil
FLAG_QUAT = 1
BTN_A = 1 << 0
BTN_DPAD_LEFT = 1 << 4
BTN_DPAD_RIGHT = 1 << 5

# Lo que se teclea en el PC a lo largo de la prueba (pasos F..F4)
EXPECTED_TEXT = "abñ😀€ab"

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
# La línea siguiente a un key: "sym: a (97), utf8: 'a'" — el keysym es lo que
# cuenta (wev imprime el código en formato xkb, evdev+8)
RE_SYM = re.compile(r"^\s+sym:\s+(\S+)")
# El texto que produce la tecla, en la misma línea. Para el Unicode es lo
# único fiable: el NOMBRE del keysym varía (U00F1 se canoniza a ntilde,
# U20AC se queda en U20AC), pero el utf8 es siempre el carácter.
RE_UTF8 = re.compile(r"utf8:\s*'(.*)'\s*$")
# Cada vez que el compositor manda un keymap nuevo a la ventana
RE_KEYMAP = re.compile(r"wl_keyboard\]\s+keymap:")
BTN_LEFT = 272
# Teclas esperadas: (keysym, código evdev); wev puede dar evdev o evdev+8
KEY_A, KEY_B, KEY_ENTER = ("a", 30), ("b", 48), ("Return", 28)
# cruceta ← / → en modo puntero (el keysym de xkb; KEY_BACK / KEY_FORWARD de evdev)
KEY_BACK, KEY_FORWARD = ("XF86Back", 158), ("XF86Forward", 159)

def parse_wev(text):
    """Eventos de wev en orden: [('motion', x, y)|('enter', x, y)|('button', code, state)|('axis', value)|('key', code, state, sym, utf8)]"""
    ev = []
    for line in text.splitlines():
        m = RE_SYM.search(line)
        if m and ev and ev[-1][0] == "key" and ev[-1][3] is None:
            u = RE_UTF8.search(line)
            ev[-1] = ev[-1][:3] + (m.group(1), u.group(1) if u else "")
            continue
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
            ev.append(("key", int(m.group(1)), int(m.group(2)), None, "")); continue
    return ev

def count_keymaps(text):
    return len(RE_KEYMAP.findall(text))

def key_is(e, want):
    """¿El evento de tecla `e` es la tecla `want` = (keysym, evdev)? Por keysym si wev lo dio; si no, por código evdev o xkb (evdev+8)."""
    sym, code = want
    if e[3] is not None:
        return e[3] == sym
    return e[1] in (code, code + 8)

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
    keys = [e for e in ev if e[0] == "key"]
    want = [(KEY_BACK, 1), (KEY_BACK, 0), (KEY_FORWARD, 1), (KEY_FORWARD, 0),
            (KEY_A, 1), (KEY_A, 0), (KEY_B, 1), (KEY_B, 0), (KEY_ENTER, 1), (KEY_ENTER, 0)]
    it = iter(keys)
    ordered = all(any(key_is(k, w) and k[2] == s for k in it) for w, s in want)
    seen = [(k[3] or k[1], k[2]) for k in keys]
    out.append((ordered, f"teclas atrás, adelante, a, b, Intro pulsadas y soltadas en orden (llegaron {seen})"))
    return out

def rules_text(ev, expected, keymaps):
    """Comprobaciones del texto: cada carácter llegó, en orden, y el keymap
    se volvió a subir para los que no tienen tecla propia."""
    out = []
    typed = [e[4] for e in ev if e[0] == "key" and e[2] == 1 and e[4]]
    for c in dict.fromkeys(expected):
        out.append((c in typed, f"llegó «{c}» al PC (texto tecleado: {typed})"))
    it = iter(typed)
    out.append((all(any(t == c for t in it) for c in expected),
                f"el texto llegó entero y en orden, {expected!r} dentro de {typed!r}"))
    out.append((keymaps >= 2,
                f"el compositor recibió un keymap nuevo al aparecer caracteres sin tecla ({keymaps})"))
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
    # D2. cruceta ← y →: atrás/adelante del navegador (XF86Back / XF86Forward)
    for _ in range(20): phone.send(args.yaw, 0.0, buttons=BTN_DPAD_LEFT)
    for _ in range(20): phone.send(args.yaw, 0.0)
    for _ in range(20): phone.send(args.yaw, 0.0, buttons=BTN_DPAD_RIGHT)
    for _ in range(20): phone.send(args.yaw, 0.0)
    # E. rueda: +30 por paquete = wheel(120) = una muesca hacia arriba, 10 veces
    for _ in range(10): phone.send(args.yaw, 0.0, scroll=30)
    for _ in range(20): phone.send(args.yaw, 0.0)
    # F. texto por TCP (modo puntero → teclado virtual)
    s.sendall(b'{"m":"text","text":"ab\\n"}\n')
    for _ in range(100): phone.send(args.yaw, 0.0)
    # F2. texto con caracteres SIN tecla: el receptor les da una de repuesto y
    # vuelve a subir el keymap (antes se perdían en silencio)
    s.sendall('{"m":"text","text":"ñ😀\\n"}\n'.encode("utf-8"))
    for _ in range(100): phone.send(args.yaw, 0.0)
    # F3. otro carácter nuevo: segundo cambio de keymap
    s.sendall('{"m":"text","text":"€\\n"}\n'.encode("utf-8"))
    for _ in range(100): phone.send(args.yaw, 0.0)
    # F4. y el ASCII sigue bien DESPUÉS de los dos cambios
    s.sendall(b'{"m":"text","text":"ab\\n"}\n')
    for _ in range(100): phone.send(args.yaw, 0.0)

    # G. sondeo de wev hasta que todo pase (los eventos tardan en llegar al archivo)
    end = time.time() + 12
    results = []
    while True:
        text = read_from(wev_log, offset)
        ev = parse_wev(text)
        results = rules(ev, W, H, args.sens, args.yaw) + rules_text(ev, EXPECTED_TEXT, count_keymaps(text))
        if all(ok for ok, _ in results) or time.time() > end:
            break
        for _ in range(50): phone.send(args.yaw, 0.0)
    for ok, msg in results:
        check(ok, msg)
    # El cambio de keymap no puede haberse llevado por delante la conexión
    rx = read_from(rx_log, 0)
    muerto = [m for m in ("inyección perdida", "El inyector", "pánico") if m in rx]
    check(not muerto, f"el inyector sigue vivo tras los cambios de keymap ({muerto})")
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
    ap.add_argument("--expect-text", default="", help="solo con --parse-only: comprueba el texto tecleado")
    args = ap.parse_args()
    if args.parse_only:
        text = open(args.parse_only, encoding="utf-8", errors="replace", newline="").read()
        ev = parse_wev(text)
        if args.expect_text:
            for ok, msg in rules_text(ev, args.expect_text, count_keymaps(text)):
                check(ok, msg)
        else:
            for ok, msg in rules(ev, 1280, 720, args.sens, args.yaw):
                check(ok, msg)
    else:
        drive(args)
    print("RESULTADO:", "OK" if fails == 0 else f"FALLO ({fails})", flush=True)
    sys.exit(0 if fails == 0 else 1)

if __name__ == "__main__":
    main()
