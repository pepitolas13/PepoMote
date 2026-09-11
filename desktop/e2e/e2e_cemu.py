"""e2e del receptor en modo Wii U (Cemu): TCP/UDP reales, cliente DSU y
perfiles de Cemu escritos en un APPDATA aislado. Requiere el receptor
arrancado con PEPOMOTE_PAIR_CODE=1234 y APPDATA=<dir aislado> (con una
carpeta Cemu\\ vacía dentro para que haya "rastro" de Cemu).
Uso: python e2e_cemu.py <APPDATA aislado>"""
import json, os, socket, struct, sys, threading, time, zlib

HOST = "127.0.0.1"; PORT = int(os.environ.get("PEPOMOTE_PORT", "26761")); DSU = int(os.environ.get("PEPOMOTE_DSU_PORT", "26760"))
DOLPHIN_DIR = os.environ.get("PEPOMOTE_DOLPHIN_DIR")
APPDATA = sys.argv[1]
PROFILES = os.path.join(APPDATA, "Cemu", "controllerProfiles")
FLAG_QUAT, FLAG_STICK, FLAG_EXT, FLAG_TOUCH = 1, 2, 4, 8
BTN = dict(A=1 << 0, B=1 << 1, UP=1 << 2, PLUS=1 << 6, MINUS=1 << 7, HOME=1 << 8, ONE=1 << 9, C=1 << 17, Z=1 << 18,
           X=1 << 19, Y=1 << 20, L=1 << 21, R=1 << 22, ZL=1 << 23, ZR=1 << 24, L3=1 << 25, R3=1 << 26, MIC=1 << 27, SCREEN=1 << 28)

fails = 0
def check(cond, msg):
    global fails
    print(("OK   " if cond else "FAIL ") + msg)
    if not cond: fails += 1

def hello(extra):
    s = socket.create_connection((HOST, PORT), timeout=5)
    msg = {"m": "hello", "pv": 1, "name": extra.pop("name", "e2e"), "model": "e2e"}
    msg.update(extra)
    s.sendall((json.dumps(msg) + "\n").encode())
    f = s.makefile("r", encoding="utf-8")
    ok = json.loads(f.readline())
    # latido TCP a 1 Hz como el móvil (el receptor da por muerta una sesión a los 5 s sin nada)
    def beat():
        while True:
            time.sleep(0.8)
            try: s.sendall(b'{"m":"ping","t":1}\n')
            except OSError: return
    threading.Thread(target=beat, daemon=True).start()
    return s, f, ok

def readmsg(f, m, timeout=3.0):
    """Lee líneas hasta encontrar un mensaje con m == m (ignora ping/notice…)."""
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

def input_packet(session, seq, flags, buttons, stick=(0, 0), stick2=(0, 0), touch=(0, 0), t=1_000_000):
    ext = flags & FLAG_EXT
    b = bytearray(80 if ext else 72)
    struct.pack_into("<I", b, 0, 0x31504D50); b[4] = 1; b[5] = flags
    b[6] = stick[0] & 0xFF; b[7] = stick[1] & 0xFF
    struct.pack_into("<I", b, 8, session); struct.pack_into("<I", b, 12, seq)
    struct.pack_into("<Q", b, 16, t + seq * 4000)
    struct.pack_into("<4f", b, 24, 1, 0, 0, 0)
    struct.pack_into("<3f", b, 40, 0, 0, 0); struct.pack_into("<3f", b, 52, 0, 0, 9.80665)
    struct.pack_into("<I", b, 64, buttons); b[69] = 90
    if ext:
        b[72] = stick2[0] & 0xFF; b[73] = stick2[1] & 0xFF
        struct.pack_into("<H", b, 74, touch[0]); struct.pack_into("<H", b, 76, touch[1])
    return bytes(b)

def dsu_request(pad):
    payload = struct.pack("<IBB6s", 0x100002, 1, pad, b"\0" * 6)
    hdr = bytearray(b"DSUC" + struct.pack("<HHII", 1001, len(payload), 0, 0xE2E)) + payload
    struct.pack_into("<I", hdr, 8, zlib.crc32(bytes(hdr)) & 0xFFFFFFFF)
    return bytes(hdr)

def pad_data(pad, send_inputs, tries=150):
    """Suscribe al pad y devuelve el primer PadData de ese pad mientras se emiten INPUTs."""
    dsu = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); dsu.settimeout(0.5)
    dsu.sendto(dsu_request(pad), (HOST, DSU))
    for i in range(tries):
        send_inputs(i)
        if i % 20 == 0: dsu.sendto(dsu_request(pad), (HOST, DSU))
        try:
            d, _ = dsu.recvfrom(256)
        except socket.timeout:
            continue
        if d[:4] == b"DSUS" and struct.unpack_from("<I", d, 16)[0] == 0x100002 and d[20] == pad:
            dsu.close(); return d
        time.sleep(0.004)
    dsu.close(); return None

def profile(index):
    p = os.path.join(PROFILES, f"controller{index}.xml")
    return open(p, encoding="utf-8").read() if os.path.exists(p) else None

def wait_profile(index, pred, timeout=4.0):
    end = time.time() + timeout
    while time.time() < end:
        if pred(profile(index)): return True
        time.sleep(0.1)
    return pred(profile(index))

udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
seqs = {}
def send(session, flags, buttons, **kw):
    seqs[session] = seqs.get(session, 0) + 1
    udp.sendto(input_packet(session, seqs[session], flags, buttons, **kw), (HOST, PORT))

# 1) mando: código, ok con modes/pad
s1, f1, ok1 = hello({"code": "1234", "name": "MandoE2E"})
check(ok1.get("m") == "ok" and ok1.get("slot") == 0, f"mando: {ok1}")
token = ok1.get("token")
check("cemu" in ok1.get("modes", []), f"ok.modes anuncia cemu: {ok1.get('modes')}")
check(ok1.get("pad") == "gamepad", f"ok.pad del jugador 1 = gamepad ({ok1.get('pad')})")

# 2) modo cemu → eco y perfil controller0.xml (GamePad)
s1.sendall(b'{"m":"mode","mode":"cemu"}\n'); r = readmsg(f1, "mode")
check(r and r.get("mode") == "cemu", f"mando: modo cemu aceptado ({r})")
check(wait_profile(0, lambda x: x and "<type>Wii U GamePad</type>" in x and "<uuid>0</uuid>" in x and "<profile>PepoMote</profile>" in x),
      "Cemu: controller0.xml = Wii U GamePad del pad 0 (marca PepoMote)")
p0 = profile(0) or ""
check("<motion>true</motion>" in p0 and "<ip>127.0.0.1</ip>" in p0 and "<port>26760</port>" in p0, "Cemu: motion + DSU 127.0.0.1:26760 en el perfil")
check("<mapping>27</mapping>\n\t\t\t\t<button>16</button>" in p0, "Cemu: Home → Touch (botón 16)")
check("<mapping>7</mapping>\n\t\t\t\t<button>42</button>" in p0, "Cemu: ZL → X-Trigger+ (42)")
notice = readmsg(f1, "notice", 1.0)
check(notice is not None and notice.get("text", "").startswith("Cemu configurado"), f"notice al móvil: {notice}")

# 3) DSU pad 0 con el GamePad: botones, sticks, gatillos, táctil, Home→Touch
sess1 = ok1["session_id"]
btns = BTN["A"] | BTN["X"] | BTN["Y"] | BTN["L"] | BTN["R"] | BTN["ZL"] | BTN["ZR"] | BTN["L3"] | BTN["R3"] | BTN["MIC"] | BTN["SCREEN"] | BTN["HOME"] | BTN["PLUS"] | BTN["MINUS"] | BTN["UP"]
d = pad_data(0, lambda i: send(sess1, FLAG_QUAT | FLAG_STICK | FLAG_EXT | FLAG_TOUCH, btns, stick=(100, -50), stick2=(-30, 120), touch=(0x8000, 0x4000)))
check(d is not None, "DSU: PadData del pad 0 recibido")
if d:
    check(d[36] == 0xFF & (1 | (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4)), f"DSU b36 Share L3 R3 Options Up: {d[36]:08b}")
    check(d[37] == ((1 << 0) | (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4) | (1 << 6) | (1 << 7)), f"DSU b37 L2(mic) R2(pantalla) L1 R1 Triangle Cross Square: {d[37]:08b}")
    check(d[39] == 0xFF, "DSU: Home → Touch (byte 39)")
    check(list(d[40:44]) == [228, 78, 98, 248], f"DSU: LX LY RX RY = 228/78/98/248 (got {list(d[40:44])})")
    check(list(d[52:56]) == [0xFF, 0xFF, 0xFF, 0xFF], f"DSU: R1 L1 R2(ZR) L2(ZL) analógicos (got {list(d[52:56])})")
    check(d[56] == 1 and struct.unpack_from("<H", d, 58)[0] == 960 and struct.unpack_from("<H", d, 60)[0] == 236,
          f"DSU: touch1 activo en (960, 236) = (0.5·1920, 0.25·942) (got {d[56]}, {struct.unpack_from('<HH', d, 58)})")
# sin dedo ni Home: touch inactivo y byte 39 a 0
d = pad_data(0, lambda i: send(sess1, FLAG_QUAT | FLAG_STICK | FLAG_EXT, 0))
check(d is not None and d[39] == 0 and d[56] == 0 and list(d[40:44]) == [128, 128, 128, 128], f"DSU: sin dedo/Home → touch inactivo, sticks neutros (got {list(d[36:62]) if d else None})")

# 4) jugador 2 → controller1.xml Pro Controller
s2, f2, ok2 = hello({"token": token, "name": "Mando2E2E"})
check(ok2.get("slot") == 1 and ok2.get("pad") == "pro" and ok2.get("mode") == "cemu", f"jugador 2: slot 1, pad pro, modo cemu ({ok2})")
check(wait_profile(1, lambda x: x and "<type>Wii U Pro Controller</type>" in x and "<uuid>1</uuid>" in x), "Cemu: controller1.xml = Pro Controller del pad 1")

# 5) el jugador 2 pide Mando Wii → eco y perfil Wiimote (device_type 5)
s2.sendall(b'{"m":"pad","pad":"wiimote"}\n'); r = readmsg(f2, "pad")
check(r and r.get("pad") == "wiimote", f"jugador 2: pad wiimote aceptado ({r})")
check(wait_profile(1, lambda x: x and "<type>Wiimote</type>" in x and "<device_type>5</device_type>" in x), "Cemu: controller1.xml = Wiimote MotionPlus")

# 6) Nunchuk entra → se acopla al jugador 1 SOLO si es Mando Wii: J1 es GamePad → sin uso; J1 pide wiimote → device_type 6 y 2 controllers
s3, f3, ok3 = hello({"token": token, "role": "nunchuk", "name": "NunchukE2E"})
check(ok3.get("slot") == 3 and ok3.get("player") == 1 and ok3.get("pad") == "nunchuk", f"nunchuk: slot 3, jugador 1 ({ok3})")
time.sleep(0.8)
check((profile(0) or "").count("<controller>") == 1, "Cemu: J1 GamePad no usa el Nunchuk (1 controller)")
s1.sendall(b'{"m":"pad","pad":"wiimote"}\n'); r = readmsg(f1, "pad")
check(r and r.get("pad") == "wiimote", "jugador 1: pad wiimote aceptado")
r3 = readmsg(f3, "pad", 3.0)
check(r3 and r3.get("pad") == "wiimote", f"nunchuk: el receptor le avisa de que entra en uso (pad wiimote) ({r3})")
check(wait_profile(0, lambda x: x and "<device_type>6</device_type>" in x and x.count("<controller>") == 2 and "<uuid>3</uuid>" in x),
      "Cemu: controller0.xml = Wiimote + Nunchuk (device_type 6, pad 3)")
# el Mando Wii en Cemu manda su puntero por el touchpad: con quat identidad tras recentrar → centro
d = pad_data(0, lambda i: send(sess1, FLAG_QUAT, BTN["A"] | BTN["HOME"]))
check(d is not None and d[39] == 0xFF and d[37] & (1 << 6), "DSU: Mando Wii en Cemu → A=Cross, Home=Touch")
check(d is not None and d[56] == 1, f"DSU: puntero IR del Mando Wii activo en el touchpad (got {d[56] if d else None}, {struct.unpack_from('<HH', d, 58) if d else None})")

# 7) vuelve a GamePad y el Nunchuk se va → controller0 vuelve a GamePad
s1.sendall(b'{"m":"pad","pad":"gamepad"}\n'); r = readmsg(f1, "pad")
check(r and r.get("pad") == "gamepad", "jugador 1: vuelve a gamepad")
r3 = readmsg(f3, "pad", 3.0)
check(r3 and r3.get("pad") == "nunchuk", f"nunchuk: deja de estar en uso (pad nunchuk) ({r3})")
check(wait_profile(0, lambda x: x and "<type>Wii U GamePad</type>" in x), "Cemu: controller0.xml vuelve a GamePad")
s3.sendall(b'{"m":"bye"}\n'); s3.close()

# 8) el jugador 2 se va → controller1.xml desaparece (era nuestro)
s2.sendall(b'{"m":"bye"}\n'); s2.close()
check(wait_profile(1, lambda x: x is None), "Cemu: controller1.xml eliminado al irse el jugador 2")

# 9) difusión del modo: un jugador 2 nuevo recibe el cambio a dolphin sin pedirlo, y Dolphin se configura como antes (regresión)
s2, f2, ok2 = hello({"token": token, "name": "Mando2E2E"})
s1.sendall(b'{"m":"mode","mode":"dolphin"}\n'); r = readmsg(f1, "mode")
check(r and r.get("mode") == "dolphin", "mando: modo dolphin aceptado")
r2 = readmsg(f2, "mode")
check(r2 and r2.get("mode") == "dolphin", f"difusión: el jugador 2 recibe mode=dolphin sin pedirlo ({r2})")
if DOLPHIN_DIR:
    wii = os.path.join(DOLPHIN_DIR, "WiimoteNew.ini")
    time.sleep(1.5)
    ini = open(wii, encoding="utf-8").read() if os.path.exists(wii) else ""
    check("[Wiimote1]" in ini and "Device = DSUClient/0/PepoMote" in ini and ini.count("Source = 1") == 2 and "Extension = None" in ini,
          "Dolphin (dir aislado): Wiimote1 y Wiimote2 con Source = 1, sin Nunchuk (regresión 1.2)")
# un modo desconocido cae a puntero (compatibilidad)
s1.sendall(b'{"m":"mode","mode":"loquesea"}\n'); r = readmsg(f1, "mode")
check(r and r.get("mode") == "pointer", "modo desconocido → pointer")

for s in (s1, s2):
    try: s.sendall(b'{"m":"bye"}\n'); s.close()
    except OSError: pass
print("FAILS:", fails); sys.exit(1 if fails else 0)
