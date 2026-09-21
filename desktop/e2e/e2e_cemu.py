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

def input_packet(session, seq, flags, buttons, stick=(0, 0), stick2=(0, 0), touch=(0, 0), t=1_000_000,
                 gyro=(0.0, 0.0, 0.0), accel=(0.0, 0.0, 9.80665)):
    ext = flags & FLAG_EXT
    b = bytearray(80 if ext else 72)
    struct.pack_into("<I", b, 0, 0x31504D50); b[4] = 1; b[5] = flags
    b[6] = stick[0] & 0xFF; b[7] = stick[1] & 0xFF
    struct.pack_into("<I", b, 8, session); struct.pack_into("<I", b, 12, seq)
    struct.pack_into("<Q", b, 16, t + seq * 4000)
    struct.pack_into("<4f", b, 24, 1, 0, 0, 0)
    struct.pack_into("<3f", b, 40, *gyro); struct.pack_into("<3f", b, 52, *accel)
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

# 3b) MOVIMIENTO: el giroscopio tiene que llegar al PadData. Cemu no mapea el
# movimiento eje por eje como Dolphin: lo mete en su propia fusión Mahony
# (`DSUControllerProvider.cpp`), así que si estos bytes salen mal o a cero, el
# juego de Wii U no ve NADA y no hay forma de enterarse desde Cemu.
# Ejes y unidades de `dsu/mapping.rs`: accel en g = (-ax, -az, ay)/9.80665,
# gyro en °/s = (gx, -gz, gy)·57.29578.
G = 9.80665
d = pad_data(0, lambda i: send(sess1, FLAG_QUAT | FLAG_STICK | FLAG_EXT, 0,
                               gyro=(0.5, 1.0, 2.0), accel=(1.0, 2.0, 9.0)))
check(d is not None, "DSU: PadData con movimiento recibido")
if d:
    acc = struct.unpack_from("<3f", d, 76)
    gyr = struct.unpack_from("<3f", d, 88)
    esperado_acc = (-1.0 / G, -9.0 / G, 2.0 / G)
    esperado_gyr = (0.5 * 57.29578, -2.0 * 57.29578, 1.0 * 57.29578)
    check(all(abs(a - b) < 0.005 for a, b in zip(acc, esperado_acc)),
          f"DSU: accel en g = {tuple(round(v, 4) for v in esperado_acc)} (got {tuple(round(v, 4) for v in acc)})")
    check(all(abs(a - b) < 0.05 for a, b in zip(gyr, esperado_gyr)),
          f"DSU: gyro en °/s = {tuple(round(v, 2) for v in esperado_gyr)} (got {tuple(round(v, 2) for v in gyr)})")
    check(any(abs(v) > 1.0 for v in gyr), f"DSU: el giroscopio NO llega a cero (got {tuple(round(v, 2) for v in gyr)})")
    ts = struct.unpack_from("<Q", d, 68)[0]
    check(ts > 0, f"DSU: timestamp de movimiento no nulo (Cemu descarta el paquete si no crece): {ts}")
# móvil quieto y plano: gravedad en accel Y = -1 g y giro exactamente 0
d = pad_data(0, lambda i: send(sess1, FLAG_QUAT | FLAG_STICK | FLAG_EXT, 0))
if d:
    check(abs(struct.unpack_from("<f", d, 80)[0] + 1.0) < 0.005, "DSU: móvil plano → accel Y = -1 g")
    check(struct.unpack_from("<3f", d, 88) == (0.0, 0.0, 0.0), "DSU: sin giro → gyro a cero")

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
check((profile(0) or "").count("<api>DSUController</api>") == 1, "Cemu: J1 GamePad no usa el Nunchuk (1 mando de entrada)")
s1.sendall(b'{"m":"pad","pad":"wiimote"}\n'); r = readmsg(f1, "pad")
check(r and r.get("pad") == "wiimote", "jugador 1: pad wiimote aceptado")
r3 = readmsg(f3, "pad", 3.0)
check(r3 and r3.get("pad") == "wiimote", f"nunchuk: el receptor le avisa de que entra en uso (pad wiimote) ({r3})")
# Dos Mandos Wii a la vez (Mario Party 10): Cemu necesita un GamePad CON dispositivo en el mando 1 aunque nadie lo sea →
# controller0.xml = GamePad por teclado del PC (nuestro, api Keyboard, sin DSU), y los Mandos Wii bajan a los mandos 2 y 3
check(wait_profile(0, lambda x: x and "<type>Wii U GamePad</type>" in x and "<profile>PepoMote</profile>" in x
                   and "<api>Keyboard</api>" in x and "<uuid>keyboard</uuid>" in x and "DSUController" not in x and x.count("<controller>") == 1),
      "Cemu: controller0.xml = GamePad por teclado del PC (todos son Mando Wii; Cemu lo necesita con dispositivo)")
check("<mapping>1</mapping>\n\t\t\t\t<button>13</button>" in (profile(0) or ""), "Cemu: GamePad por teclado: A → Intro")
check(wait_profile(1, lambda x: x and "<type>Wiimote</type>" in x and "<device_type>6</device_type>" in x and x.count("<api>DSUController</api>") == 2
                   and "<uuid>0</uuid>" in x and "<uuid>3</uuid>" in x and "PepoMote J1 Mando Wii" in x),
      "Cemu: controller1.xml = J1 Wiimote + Nunchuk (device_type 6, pads 0 y 3)")
check(wait_profile(2, lambda x: x and "<type>Wiimote</type>" in x and "<device_type>5</device_type>" in x and "<uuid>1</uuid>" in x and "PepoMote J2 Mando Wii" in x),
      "Cemu: controller2.xml = J2 Wiimote (pad 1)")
check(sum(1 for i in range(8) if profile(i) and "<type>Wii U GamePad</type>" in profile(i)) == 1, "Cemu: exactamente un GamePad, y en el mando 1")
# el Mando Wii en Cemu manda su puntero por el touchpad: con quat identidad tras recentrar → centro
d = pad_data(0, lambda i: send(sess1, FLAG_QUAT, BTN["A"] | BTN["HOME"]))
check(d is not None and d[39] == 0xFF and d[37] & (1 << 6), "DSU: Mando Wii en Cemu → A=Cross, Home=Touch")
check(d is not None and d[56] == 1, f"DSU: puntero IR del Mando Wii activo en el touchpad (got {d[56] if d else None}, {struct.unpack_from('<HH', d, 58) if d else None})")
# y el segundo Mando Wii tiene su propio pad y su propio puntero
sess2 = ok2["session_id"]
d2 = pad_data(1, lambda i: send(sess2, FLAG_QUAT, BTN["B"]))
check(d2 is not None and d2[37] & (1 << 5) and d2[56] == 1, f"DSU: J2 Mando Wii en el pad 1 → B=Circle, puntero IR activo (got {list(d2[36:40]) if d2 else None})")

# 7) vuelve a GamePad y el Nunchuk se va → controller0 vuelve a ser el GamePad del móvil J1 y J2 sube al mando 2
s1.sendall(b'{"m":"pad","pad":"gamepad"}\n'); r = readmsg(f1, "pad")
check(r and r.get("pad") == "gamepad", "jugador 1: vuelve a gamepad")
r3 = readmsg(f3, "pad", 3.0)
check(r3 and r3.get("pad") == "nunchuk", f"nunchuk: deja de estar en uso (pad nunchuk) ({r3})")
check(wait_profile(0, lambda x: x and "<type>Wii U GamePad</type>" in x and "<uuid>0</uuid>" in x and "<motion>true</motion>" in x),
      "Cemu: controller0.xml vuelve a ser el GamePad del móvil J1 (pad 0)")
check(wait_profile(1, lambda x: x and "<type>Wiimote</type>" in x and "<device_type>5</device_type>" in x and "<uuid>1</uuid>" in x),
      "Cemu: controller1.xml = J2 Wiimote otra vez en el mando 2")
check(wait_profile(2, lambda x: x is None), "Cemu: controller2.xml limpiado")
s3.sendall(b'{"m":"bye"}\n'); s3.close()

# 8) el jugador 2 se va → controller1.xml desaparece (era nuestro)
s2.sendall(b'{"m":"bye"}\n'); s2.close()
check(wait_profile(1, lambda x: x is None), "Cemu: controller1.xml eliminado al irse el jugador 2")

# 8a) REGRESIÓN: los mandos de los OTROS jugadores no se pisan. Jugando a
# Nintendo Land con un móvil de GamePad y los amigos con mandos de verdad,
# meter dos móviles más como Mando Wii se comía sus perfiles y Cemu dejaba de
# reconocer sus mandos hasta desconectarlos y reiniciar. Ahora el móvil se
# coloca en el primer mando LIBRE.
SUYO = ('<?xml version="1.0" encoding="UTF-8"?>\n<emulated_controller>\n\t<type>Wii U Pro Controller</type>\n'
        '\t<controller>\n\t\t<api>XInput</api>\n\t\t<uuid>1</uuid>\n\t\t<display_name>Mando de otro jugador</display_name>\n'
        '\t\t<mappings>\n\t\t</mappings>\n\t</controller>\n</emulated_controller>\n')
open(os.path.join(PROFILES, "controller1.xml"), "w", encoding="utf-8").write(SUYO)
s2, f2, ok2 = hello({"token": token, "name": "Mando2E2E"})
check(wait_profile(2, lambda x: x and "<profile>PepoMote</profile>" in x), "Cemu: el móvil nuevo va al mando 3, que está libre")
check(profile(1) == SUYO, "Cemu: el mando del otro jugador sigue intacto")
check(not os.path.exists(os.path.join(PROFILES, "controller1.xml.pepomote.bak")), "Cemu: no ha hecho falta respaldar nada suyo")
s2.sendall(b'{"m":"bye"}\n'); s2.close()
check(wait_profile(2, lambda x: x is None), "Cemu: al irse el móvil, su perfil desaparece y el del otro jugador sigue")
check(profile(1) == SUYO, "Cemu: el mando del otro jugador, intacto también después")
os.remove(os.path.join(PROFILES, "controller1.xml"))

# 8a-bis) REGRESIÓN del latido DSU: un pad SIN móvil tiene que contestar
# igualmente «aquí no hay nadie». Cemu solo vuelve a pedir un pad cuando
# recibe ESE pad (pide los suyos una vez al arrancar y nada más), así que sin
# latido un móvil callado 3 s —pantalla apagada, bache de wifi— dejaba su
# mando muerto hasta reiniciar Cemu.
dsu_mudo = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); dsu_mudo.settimeout(1.5)
dsu_mudo.sendto(dsu_request(3), (HOST, DSU))
latido, fin = None, time.time() + 3.0
while time.time() < fin:
    try: d, _ = dsu_mudo.recvfrom(256)
    except socket.timeout: break
    if d[:4] == b"DSUS" and struct.unpack_from("<I", d, 16)[0] == 0x100002 and d[20] == 3:
        latido = d; break
dsu_mudo.close()
check(latido is not None and len(latido) == 100 and latido[21] == 0 and latido[31] == 0,
      f"DSU: el pad 3 (sin móvil) late igualmente diciendo desconectado ({latido[20:32].hex() if latido else None})")

# 8b) «solo pantalla»: el móvil GamePad solo hace de pantalla táctil junto al mando real del usuario
def readnotice(f, prefix, timeout=4.0):
    """Lee avisos hasta uno que empiece por `prefix` (los demás se saltan)."""
    end = time.time() + timeout
    while time.time() < end:
        n = readmsg(f, "notice", max(0.1, end - time.time()))
        if n is None: return None
        if n.get("text", "").startswith(prefix): return n
    return None

AJENO = ('<?xml version="1.0" encoding="UTF-8"?>\n<emulated_controller>\n\t<type>Wii U GamePad</type>\n\t<profile>MiMando</profile>\n'
         '\t<controller>\n\t\t<api>XInput</api>\n\t\t<uuid>0</uuid>\n\t\t<display_name>Controller 1</display_name>\n'
         '\t\t<mappings>\n\t\t\t<entry>\n\t\t\t\t<mapping>1</mapping>\n\t\t\t\t<button>0</button>\n\t\t\t</entry>\n\t\t</mappings>\n'
         '\t</controller>\n</emulated_controller>\n')
BAK0 = os.path.join(PROFILES, "controller0.xml.pepomote.bak")
s1.sendall(b'{"m":"bye"}\n'); s1.close()
time.sleep(0.6)
os.makedirs(PROFILES, exist_ok=True)
open(os.path.join(PROFILES, "controller0.xml"), "w", encoding="utf-8").write(AJENO)
if os.path.exists(BAK0): os.remove(BAK0)
s1, f1, ok1 = hello({"token": token, "name": "MandoE2E", "screen_only": True})
check(ok1.get("screen_only") is True and ok1.get("pad") == "gamepad", f"solo pantalla: ok.screen_only y pad gamepad ({ok1})")
check(wait_profile(0, lambda x: x and x.count("<controller>") == 2 and "<api>XInput</api>" in x and "PepoMote J1 pantalla" in x
                   and "<profile>MiMando</profile>" in x and "<profile>PepoMote</profile>" not in x),
      "Cemu: controller0.xml fusionado (mando XInput del usuario + DSU del móvil, sin marca PepoMote)")
p0 = profile(0) or ""
ours = p0[p0.find("PepoMote J1 pantalla"):]
check("<motion>" not in ours and "<entry>" not in ours and "<ip>127.0.0.1</ip>" in ours and "<port>26760</port>" in ours,
      "Cemu: nuestro nodo sin movimiento ni botones, con ip y puerto del DSU")
check(os.path.exists(BAK0) and open(BAK0, encoding="utf-8").read() == AJENO, "Cemu: copia .pepomote.bak del perfil del usuario tal cual")
notice = readnotice(f1, "Cemu configurado", 3.0)
check(notice is not None, f"solo pantalla: notice de configuración ({notice})")
# el táctil sigue llegando por el pad DSU del móvil
d = pad_data(0, lambda i: send(ok1["session_id"], FLAG_QUAT | FLAG_STICK | FLAG_EXT | FLAG_TOUCH, 0, touch=(0x8000, 0x4000)))
check(d is not None and d[56] == 1 and struct.unpack_from("<H", d, 58)[0] == 960 and struct.unpack_from("<H", d, 60)[0] == 236,
      f"DSU: táctil del móvil solo pantalla activo (960, 236) (got {struct.unpack_from('<HH', d, 58) if d else None})")
# apagar: perfil completo nuestro (queda la copia); encender: fusión otra vez desde la copia
s1.sendall(b'{"m":"screen_only","on":false}\n'); r = readmsg(f1, "screen_only")
check(r is not None and r.get("on") is False, f"solo pantalla: eco off ({r})")
check(wait_profile(0, lambda x: x and "<profile>PepoMote</profile>" in x and x.count("<api>DSUController</api>") == 1), "Cemu: apagado → perfil completo nuestro")
check(open(BAK0, encoding="utf-8").read() == AJENO, "Cemu: la copia del usuario sigue intacta")
s1.sendall(b'{"m":"screen_only","on":true}\n'); r = readmsg(f1, "screen_only")
check(r is not None and r.get("on") is True, f"solo pantalla: eco on ({r})")
check(wait_profile(0, lambda x: x and x.count("<controller>") == 2 and "<api>XInput</api>" in x and "<profile>PepoMote</profile>" not in x),
      "Cemu: encendido → fusionado otra vez desde la copia")
# un jugador 2 (sin la opción) es Pro; se va antes que J1
s2, f2, ok2 = hello({"token": token, "name": "Mando2E2E"})
check(ok2.get("screen_only") is False and ok2.get("pad") == "pro", f"jugador 2: ok.screen_only false, pad pro ({ok2})")
check(wait_profile(1, lambda x: x and "<type>Wii U Pro Controller</type>" in x), "Cemu: controller1.xml Pro para el jugador 2")
s2.sendall(b'{"m":"bye"}\n'); s2.close()
check(wait_profile(1, lambda x: x is None), "Cemu: controller1.xml eliminado al irse el jugador 2")
check((profile(0) or "").count("<controller>") == 2, "Cemu: controller0.xml sigue fusionado con J2 fuera")
# el último en irse era solo pantalla → el perfil del usuario vuelve tal cual y la copia desaparece
s1.sendall(b'{"m":"bye"}\n'); s1.close()
check(wait_profile(0, lambda x: x == AJENO), "Cemu: al irse el móvil solo pantalla, el perfil del usuario vuelve tal cual")
check(not os.path.exists(BAK0), "Cemu: y la copia desaparece")
# Controller 1 del usuario que no es un GamePad: se fusiona igual y el móvil recibe el aviso
open(os.path.join(PROFILES, "controller0.xml"), "w", encoding="utf-8").write(AJENO.replace("Wii U GamePad", "Wii U Pro Controller"))
s1, f1, ok1 = hello({"token": token, "name": "MandoE2E", "screen_only": True})
check(readnotice(f1, "Cemu: el mando 1 no es un GamePad", 4.0) is not None, "solo pantalla: aviso de que el mando 1 no es un GamePad")
check(wait_profile(0, lambda x: x and x.count("<controller>") == 2 and "Wii U Pro Controller" in x), "Cemu: fusionado también con tipo Pro (sin cambiar el tipo)")
s1.sendall(b'{"m":"screen_only","on":false}\n'); r = readmsg(f1, "screen_only")
check(r is not None and r.get("on") is False, "solo pantalla: apagado para seguir con el resto")

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
