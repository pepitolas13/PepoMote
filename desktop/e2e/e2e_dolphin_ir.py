"""e2e del receptor en Windows/Linux: puntero IR del perfil Wii (Dolphin ≥ 2407,
protocol/DSU.md «Puntero IR») con un móvil simulado que gira de verdad
(quaternion y giroscopio coherentes): recentrado, yaw, pitch, roll, «acercar»
(bit 30) en rampa, fuera de cámara (centinela), Nunchuk propio (sin roll) y el
WiimoteNew.ini que escribe el receptor (IRPassthrough + IMUIR). El sentido de
los ejes se cruza con el Mando Wii de Cemu, que usa el MISMO motor de puntero
y manda la posición por el touchpad: lo que en Cemu va a la derecha, en la
cámara de Dolphin baja la X (espejo, como la cámara real). Requiere el
receptor de pruebas aislado (README: PEPOMOTE_PAIR_CODE=1234, PEPOMOTE_PORT,
PEPOMOTE_DSU_PORT, PEPOMOTE_DOLPHIN_DIR, PEPOMOTE_ASSUME_EMULATOR_CLOSED=1)."""
import json, math, os, socket, struct, sys, time, zlib

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

HOST = "127.0.0.1"
PORT = int(os.environ.get("PEPOMOTE_PORT", "26761"))
DSU = int(os.environ.get("PEPOMOTE_DSU_PORT", "26760"))
DOLPHIN_DIR = os.environ.get("PEPOMOTE_DOLPHIN_DIR")
BTN_NEAR = 1 << 30
FLAG_QUAT = 0x01
FLAG_STICK = 0x02

fails = 0
def check(cond, msg):
    global fails
    print(("OK   " if cond else "FAIL ") + msg)
    if not cond:
        fails += 1

def hello(extra):
    s = socket.create_connection((HOST, PORT), timeout=5)
    msg = {"m": "hello", "pv": 1, "name": extra.pop("name", "e2e-ir"), "model": "e2e"}
    msg.update(extra)
    s.sendall((json.dumps(msg) + "\n").encode())
    f = s.makefile("r")
    ok = json.loads(f.readline())
    while ok.get("m") == "notice":
        ok = json.loads(f.readline())
    return s, f, ok

def recv(f, m, want=None, limit=40):
    """Siguiente mensaje `m` (que cumpla `want`); se saltan avisos, pongs y
    difusiones anteriores."""
    for _ in range(limit):
        try:
            line = f.readline()
        except OSError:
            return {}
        if not line:
            return {}
        msg = json.loads(line)
        if msg.get("m") == m and (want is None or want(msg)):
            return msg
    return {}

PING = (json.dumps({"m": "ping", "t": 0}) + "\n").encode()

def dsu_request(pad):
    payload = struct.pack("<IBB6s", 0x100002, 1, pad, b"\0" * 6)
    hdr = bytearray(b"DSUC" + struct.pack("<HHII", 1001, len(payload), 0, 0xE2E)) + payload
    struct.pack_into("<I", hdr, 8, zlib.crc32(bytes(hdr)) & 0xFFFFFFFF)
    return bytes(hdr)

# --- giros: quaternion (w, x, y, z) dispositivo → mundo, ejes del dispositivo
# X derecha, Y borde superior (apuntado), Z hacia arriba de la pantalla ---
def q_axis(axis, deg):
    s = math.sin(math.radians(deg) / 2)
    c = math.cos(math.radians(deg) / 2)
    return (c, s * axis[0], s * axis[1], s * axis[2])

Z = (0.0, 0.0, 1.0)  # yaw (girar en el plano horizontal): negativo = a la derecha
X = (1.0, 0.0, 0.0)  # pitch: positivo = arriba
Y = (0.0, 1.0, 0.0)  # roll sobre el eje de apuntado: positivo = lado derecho abajo

class Phone:
    """Un móvil simulado: manda INPUT a 250 Hz con reloj de sensor propio y
    lee el PadData de su pad DSU (como Dolphin: suscrito a ese slot)."""
    def __init__(self, session, slot, tcp):
        self.session = session
        self.slot = slot
        self.tcp = tcp          # la app manda ping 1 Hz: sin él el receptor cierra a los 5 s
        self.last_ping = 0.0
        self.seq = 1
        self.t = 1_000_000
        self.udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.dsu = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.dsu.setblocking(False)
        self.last_sub = 0.0
        self.last = None

    def packet(self, quat, gyro, buttons, stick=None):
        b = bytearray(72)
        struct.pack_into("<I", b, 0, 0x31504D50)
        b[4] = 1
        b[5] = FLAG_QUAT | (FLAG_STICK if stick is not None else 0)
        if stick is not None:
            b[6] = stick[0] & 0xFF
            b[7] = stick[1] & 0xFF
        struct.pack_into("<I", b, 8, self.session)
        struct.pack_into("<I", b, 12, self.seq)
        struct.pack_into("<Q", b, 16, self.t)
        struct.pack_into("<4f", b, 24, *quat)
        struct.pack_into("<3f", b, 40, *gyro)
        struct.pack_into("<3f", b, 52, 0.0, 0.0, 9.80665)
        struct.pack_into("<I", b, 64, buttons)
        b[69] = 90
        self.seq += 1
        self.t += 4000
        return bytes(b)

    def pump(self, quat, gyro, buttons, n, stick=None):
        """n paquetes (4 ms) con esa pose; devuelve el último PadData visto."""
        for _ in range(n):
            if time.time() - self.last_ping > 1.0:
                try:
                    self.tcp.sendall(PING)
                except OSError:
                    pass
                self.last_ping = time.time()
            if time.time() - self.last_sub > 1.0:
                self.dsu.sendto(dsu_request(self.slot), (HOST, DSU))
                self.last_sub = time.time()
            self.udp.sendto(self.packet(quat, gyro, buttons, stick), (HOST, PORT))
            time.sleep(0.004)
            while True:
                try:
                    d, _ = self.dsu.recvfrom(256)
                except (BlockingIOError, OSError):
                    break
                if d[:4] == b"DSUS" and struct.unpack_from("<I", d, 16)[0] == 0x100002 and d[20] == self.slot:
                    self.last = d
        return self.last

    def turn(self, axis, deg_from, deg_to, buttons=0, stick=None, secs=0.4):
        """Giro real: quaternion interpolado y giroscopio coherente (rad/s en
        ejes del dispositivo), luego 0,6 s quieto en la pose final."""
        n = max(1, int(secs / 0.004))
        rate = math.radians(deg_to - deg_from) / secs
        gyro = tuple(rate * a for a in axis)
        for i in range(1, n + 1):
            self.pump(q_axis(axis, deg_from + (deg_to - deg_from) * i / n), gyro, buttons, 1, stick)
        return self.pump(q_axis(axis, deg_to), (0.0, 0.0, 0.0), buttons, 150, stick)

def ir(d):
    """Campos del puntero IR del PadData Wii: (RX, RY, L2, R2, nivel, LX)."""
    return (d[42], d[43], d[55], d[54], (d[36] >> 1) & 0b11, d[40])

def mid(d):
    """Punto medio reconstruido como lo hace el perfil, en píxeles de cámara."""
    x = ((max(d[42] - 128, 0) / 127) * 32512 + d[55]) / 32767 * 1023
    y = ((max(d[43] - 128, 0) / 127) * 32512 + d[54]) / 32767 * 767
    return x, y

# 1) mando por código
s1, f1, ok1 = hello({"code": "1234", "name": "MandoIR"})
check(ok1.get("m") == "ok" and ok1.get("slot") == 0, f"mando: {ok1}")
token = ok1.get("token")
p = Phone(ok1["session_id"], 0, s1)

# 2) sentido de los ejes con el Mando Wii de Cemu (mismo motor, touchpad)
s1.sendall(b'{"m":"mode","mode":"cemu"}\n'); r = recv(f1, "mode", lambda x: x.get("mode") == "cemu")
check(r.get("mode") == "cemu", "modo cemu aceptado")
s1.sendall(b'{"m":"pad","pad":"wiimote"}\n'); r = recv(f1, "pad", lambda x: x.get("pad") == "wiimote")
check(r.get("pad") == "wiimote", f"pad Mando Wii aceptado ({r})")
d = p.pump(q_axis(Z, 0), (0.0, 0.0, 0.0), 0, 250)
center_touch = struct.unpack_from("<HH", d, 58) if d else None
check(d is not None and d[56] == 1 and center_touch and abs(center_touch[0] - 960) < 40 and abs(center_touch[1] - 471) < 40,
      f"Cemu: quat identidad tras recentrar → touchpad en el centro ({center_touch})")
d = p.turn(Z, 0, -10)
right_touch = struct.unpack_from("<HH", d, 58)
cemu_right = right_touch[0] > 960 + 150
check(right_touch[0] != center_touch[0], f"Cemu: girar −10° sobre Z mueve el touchpad en X ({center_touch} → {right_touch})")
print(f"     giro −10° sobre Z = {'DERECHA' if cemu_right else 'IZQUIERDA'} según el motor")
d = p.turn(Z, -10, 0)
d = p.turn(X, 0, 10)
up_touch = struct.unpack_from("<HH", d, 58)
cemu_up = up_touch[1] < 471 - 100
check(up_touch[1] != center_touch[1], f"Cemu: girar +10° sobre X mueve el touchpad en Y ({center_touch} → {up_touch})")
print(f"     giro +10° sobre X = {'ARRIBA' if cemu_up else 'ABAJO'} según el motor")
p.turn(X, 10, 0)

# 3) modo Dolphin: el mismo giro en la cámara
s1.sendall(b'{"m":"mode","mode":"dolphin"}\n'); r = recv(f1, "mode", lambda x: x.get("mode") == "dolphin")
check(r.get("mode") == "dolphin", "modo dolphin aceptado")
d = p.pump(q_axis(Z, 0), (0.0, 0.0, 0.0), 0, 250)
check(d is not None, "Dolphin: PadData del pad 0")
rx, ry, l2, r2, lvl, lx = ir(d)
cx, cy = mid(d)
check(abs(cx - 511.5) < 12, f"Dolphin: centro → X media {cx:.0f} ≈ 511 (RX={rx} L2={l2})")
check(abs(cy - 452) < 14, f"Dolphin: centro → Y media {cy:.0f} ≈ 452 (barra arriba, nivel 0) (RY={ry} R2={r2})")
check(lvl == 0, f"Dolphin: nivel 0 sin acercar (L3/R3 = {lvl})")
check(lx == 192, f"Dolphin: roll 0 → Left X = 192 (got {lx})")
check(d[37] & 0b11 == 0, "Dolphin: los bits L2/R2 del bitmask no se tocan")

# yaw: el sentido que el motor llamó derecha tiene que BAJAR la X (espejo)
d = p.turn(Z, 0, -10)
x_turn, _ = mid(d)
expect_lower = cemu_right
check((x_turn < cx - 150) if expect_lower else (x_turn > cx + 150),
      f"Dolphin: −10° sobre Z → X media {x_turn:.0f} ({'baja' if expect_lower else 'sube'} respecto a {cx:.0f}: espejo de la cámara)")
check(abs(abs(x_turn - cx) - 240) < 45, f"Dolphin: 10° de yaw ≈ 240 px de cámara (got {abs(x_turn - cx):.0f})")
d = p.turn(Z, -10, 0)
x_back, _ = mid(d)
check(abs(x_back - cx) < 12, f"Dolphin: de vuelta al centro ({x_back:.0f})")

# pitch: arriba = Y baja
d = p.turn(X, 0, 10)
_, y_turn = mid(d)
expect_lower_y = cemu_up
check((y_turn < cy - 150) if expect_lower_y else (y_turn > cy + 150),
      f"Dolphin: +10° sobre X → Y media {y_turn:.0f} ({'baja' if expect_lower_y else 'sube'} respecto a {cy:.0f})")
check(abs(abs(y_turn - cy) - 240) < 45, f"Dolphin: 10° de pitch ≈ 240 px de cámara (got {abs(y_turn - cy):.0f})")
p.turn(X, 10, 0)

# roll: +30° sobre el eje de apuntado → Left X = 128 + 63,5·(1 + 30/90) ≈ 213; el punto medio no se mueve
d = p.turn(Y, 0, 30)
rx, ry, l2, r2, lvl, lx = ir(d)
rx_, ry_ = mid(d)
check(abs(lx - 213) <= 3, f"Dolphin: roll +30° → Left X ≈ 213 (got {lx})")
check(abs(rx_ - cx) < 25 and abs(ry_ - cy) < 25, f"Dolphin: el roll no mueve el punto medio ({rx_:.0f}, {ry_:.0f})")
d = p.turn(Y, 30, -30)
check(abs(ir(d)[5] - 170) <= 3, f"Dolphin: roll −30° → Left X ≈ 170 (got {ir(d)[5]})")
p.turn(Y, -30, 0)

# acercar: mantener el bit 30 sube el nivel en rampa (0 → 3 en ~360 ms) sin mover el punto medio
d = p.pump(q_axis(Z, 0), (0.0, 0.0, 0.0), BTN_NEAR, 25)   # 100 ms: aún 0 o 1
lvl_early = ir(d)[4]
d = p.pump(q_axis(Z, 0), (0.0, 0.0, 0.0), BTN_NEAR, 150)  # +600 ms: 3
lvl_late = ir(d)[4]
_, y_near = mid(d)
check(lvl_early <= 1, f"Dolphin: acercar recién pulsado → nivel {lvl_early} (rampa)")
check(lvl_late == 3, f"Dolphin: acercar mantenido 0,7 s → nivel 3 (got {lvl_late})")
check(abs(y_near - cy) < 6, f"Dolphin: acercar no mueve el punto medio (Y {y_near:.0f} ≈ {cy:.0f})")
d = p.pump(q_axis(Z, 0), (0.0, 0.0, 0.0), 0, 175)  # soltar 0,7 s
check(ir(d)[4] == 0, f"Dolphin: soltar → nivel 0 (got {ir(d)[4]})")

# fuera de la cámara: 45° a un lado → centinela Right X = 0 (tamaño 0 en el perfil)
d = p.turn(Z, 0, 45)
check(ir(d)[0] == 0, f"Dolphin: 45° de yaw → fuera de cámara, Right X = 0 (got {ir(d)[0]})")
d = p.turn(Z, 45, 0)
check(ir(d)[0] != 0 and abs(mid(d)[0] - cx) < 12, f"Dolphin: de vuelta, puntos visibles en el centro ({mid(d)[0]:.0f})")

# 4) Nunchuk propio: Left X sigue siendo el stick aunque el mando ruede
s3, f3, ok3 = hello({"token": token, "nunchuk": "own", "name": "MandoNunchukIR"})
check(ok3.get("m") == "ok" and ok3.get("slot") == 1 and ok3.get("nunchuk") == "own", f"mando+nunchuk: slot 1, nunchuk=own ({ok3})")
p3 = Phone(ok3["session_id"], 1, s3)
d3 = p3.pump(q_axis(Z, 0), (0.0, 0.0, 0.0), 0, 250, stick=(100, -50))
d3 = p3.turn(Y, 0, 30, stick=(100, -50))
check(d3 is not None and d3[40] == 228 and d3[41] == 78, f"Nunchuk propio: LX/LY = stick (228/78), sin roll (got {list(d3[40:42])})")
check(d3[42] != 0 and abs(mid(d3)[0] - 511.5) < 25, f"Nunchuk propio: puntos IR visibles en su pad ({mid(d3)[0]:.0f})")

# 5) el WiimoteNew.ini escrito por el receptor
if DOLPHIN_DIR:
    time.sleep(1.5)
    path = os.path.join(DOLPHIN_DIR, "WiimoteNew.ini")
    txt = open(path, encoding="utf-8").read() if os.path.exists(path) else ""
    check("IMUIR/Recenter = `Touch Button`" in txt and "IMUPointer" not in txt, "ini: grupo IMUIR (no IMUPointer)")
    check(txt.count("IRPassthrough/Enabled = True") == 2, f"ini: passthrough en los dos mandos ({txt.count('IRPassthrough/Enabled = True')})")
    check(txt.count("cos(") == 2, f"ini: roll solo en el mando sin Nunchuk propio (cos( × {txt.count('cos(')})")
    check("Nunchuk/Stick/Left = `DSUClient/1/PepoMote:Left X-`" in txt, "ini: el Nunchuk propio lee Left X de su pad")
else:
    print("SKIP ini: PEPOMOTE_DOLPHIN_DIR no definido")

for s in (s1, s3):
    try:
        s.sendall(b'{"m":"bye"}\n'); s.close()
    except OSError:
        pass
print("FAILS:", fails)
sys.exit(1 if fails else 0)
