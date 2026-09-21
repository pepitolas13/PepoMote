"""e2e del receptor en modo «mando universal»: un móvil simulado por TCP/UDP
reales y, al otro lado, el mando de Xbox 360 virtual leído **desde fuera** con
la API XInput de Windows. Es la única prueba que demuestra el camino entero:
si aquí sale un botón, el juego ve ese botón.

Requiere el receptor arrancado con PEPOMOTE_PAIR_CODE=1234 y el driver del
mando virtual (ViGEmBus) instalado: va dentro del exe y la CI lo instala antes
con `PepoMote.exe --install-driver`. Sin driver se salta sola y sale con 0
(el mapeo ya está cubierto por los tests unitarios).

Uso: python e2e_gamepad.py <APPDATA aislado>"""
import ctypes, json, os, socket, struct, sys, threading, time

HOST = "127.0.0.1"
PORT = int(os.environ.get("PEPOMOTE_PORT", "26761"))
APPDATA = sys.argv[1] if len(sys.argv) > 1 else os.environ.get("APPDATA", ".")

FLAG_QUAT, FLAG_STICK, FLAG_EXT, FLAG_TILT = 1, 2, 4, 16
# Bits del INPUT (pmp/src/lib.rs)
BTN = dict(A=1 << 0, B=1 << 1, UP=1 << 2, DOWN=1 << 3, LEFT=1 << 4, RIGHT=1 << 5, PLUS=1 << 6, MINUS=1 << 7,
           HOME=1 << 8, ONE=1 << 9, TWO=1 << 10, MIC=1 << 27, X=1 << 19, Y=1 << 20, L=1 << 21, R=1 << 22,
           ZL=1 << 23, ZR=1 << 24, L3=1 << 25, R3=1 << 26)
# Bits de XInput (XINPUT_GAMEPAD.wButtons)
XB = dict(UP=0x0001, DOWN=0x0002, LEFT=0x0004, RIGHT=0x0008, START=0x0010, BACK=0x0020, LTHUMB=0x0040,
          RTHUMB=0x0080, LB=0x0100, RB=0x0200, GUIDE=0x0400, A=0x1000, B=0x2000, X=0x4000, Y=0x8000)

fails = 0
def check(cond, msg):
    global fails
    print(("OK   " if cond else "FAIL ") + msg)
    if not cond:
        fails += 1
        # Un fallo aquí casi siempre es «estoy leyendo el mando que no es» o
        # «XInput aún no lo refleja»: enseñar los cuatro huecos lo distingue
        # de un mapeo mal hecho sin tener que volver a ejecutarlo.
        for i in range(4):
            for etiqueta, ex in (("ex", True), ("plano", False)):
                g = xstate(i, ex=ex)
                estado = "vacío" if g is None else (
                    f"b={g.wButtons:#06x} lt={g.bLeftTrigger} rt={g.bRightTrigger} "
                    f"l=({g.sThumbLX},{g.sThumbLY}) r=({g.sThumbRX},{g.sThumbRY})")
                print(f"       XInput {i} [{etiqueta}]: {estado}")

def done(extra=""):
    if extra:
        print(extra)
    print("FAILS:", fails)
    sys.exit(1 if fails else 0)

# --------------------------------------------------------------- XInput
class XGamepad(ctypes.Structure):
    _fields_ = [("wButtons", ctypes.c_ushort), ("bLeftTrigger", ctypes.c_ubyte), ("bRightTrigger", ctypes.c_ubyte),
                ("sThumbLX", ctypes.c_short), ("sThumbLY", ctypes.c_short),
                ("sThumbRX", ctypes.c_short), ("sThumbRY", ctypes.c_short)]

class XState(ctypes.Structure):
    _fields_ = [("dwPacketNumber", ctypes.c_uint), ("Gamepad", XGamepad)]

_xi = None
_get_state = None
_get_state_ex = None
for name in ("XInput1_4.dll", "XInput1_3.dll", "XInput9_1_0.dll"):
    try:
        _xi = ctypes.WinDLL(name)
        _get_state = _xi.XInputGetState
        # El Guide no sale por la API pública; el ordinal 100 (XInputGetStateEx,
        # sin documentar pero estable desde Windows 7) sí lo trae.
        try:
            _get_state_ex = _xi[100]
        except Exception:
            _get_state_ex = None
        break
    except OSError:
        continue

def xstate(index, ex=False):
    """Estado del mando XInput, o None si ese hueco está vacío."""
    fn = _get_state_ex if (ex and _get_state_ex) else _get_state
    st = XState()
    if fn(ctypes.c_uint(index), ctypes.byref(st)) != 0:
        return None
    return st.Gamepad

def connected():
    return {i for i in range(4) if xstate(i) is not None}

# XInput cachea por proceso qué mandos hay y no se entera de las RETIRADAS sin
# un bucle de mensajes de Windows (las altas sí las ve). Este proceso no tiene
# ventana, así que para comprobar que un mando se ha ido hay que preguntarlo
# desde uno nuevo. Comprobado a mano: con el receptor muerto, un proceso
# recién nacido ve los cuatro huecos vacíos y este mismo proceso no.
_FRESH = (
    "import ctypes;"
    "xi=ctypes.WinDLL('XInput1_4.dll');"
    "b=ctypes.create_string_buffer(16);"
    "print(''.join(str(i) for i in range(4)"
    " if xi.XInputGetState(ctypes.c_uint(i), b)==0))"
)

def connected_fresh():
    import subprocess
    out = subprocess.run([sys.executable, "-c", _FRESH], capture_output=True, text=True, timeout=20)
    return {int(c) for c in out.stdout.strip() if c.isdigit()}

# ----------------------------------------------------------- móvil falso
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
            try:
                s.sendall(b'{"m":"ping","t":1}\n')
            except OSError:
                return
    threading.Thread(target=beat, daemon=True).start()
    return s, f, ok

def read_mode(f, timeout=3.0):
    end = time.time() + timeout
    while time.time() < end:
        try:
            line = f.readline()
        except (socket.timeout, OSError):
            return None
        if not line:
            break
        msg = json.loads(line)
        if msg.get("m") == "mode":
            return msg
    return None

def input_packet(session, seq, flags, buttons, stick=(0, 0), stick2=(0, 0), gyro=(0.0, 0.0, 0.0)):
    ext = flags & FLAG_EXT
    b = bytearray(80 if ext else 72)
    struct.pack_into("<I", b, 0, 0x31504D50)
    b[4] = 1
    b[5] = flags
    b[6] = stick[0] & 0xFF
    b[7] = stick[1] & 0xFF
    struct.pack_into("<I", b, 8, session)
    struct.pack_into("<I", b, 12, seq)
    struct.pack_into("<Q", b, 16, 1_000_000 + seq * 4000)
    struct.pack_into("<4f", b, 24, 1, 0, 0, 0)
    struct.pack_into("<3f", b, 40, *gyro)
    struct.pack_into("<3f", b, 52, 0.0, 0.0, 9.80665)
    struct.pack_into("<I", b, 64, buttons)
    b[69] = 90
    if ext:
        b[72] = stick2[0] & 0xFF
        b[73] = stick2[1] & 0xFF
    return bytes(b)

udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
seq = [0]

def hold(session, buttons=0, secs=0.3, **kw):
    """Mantiene un estado un rato (como un dedo de verdad) y devuelve lo
    último que ve XInput.

    Se lee DENTRO del bucle y con una pausa entre el envío y la lectura: el
    valor que ViGEm acaba de escribir no aparece en XInput al instante (pasa
    por la pila USB), y leer pegado al `sendto` devuelve el estado anterior.
    Los botones colaban y los ejes no, que es de los fallos más tramposos que
    hay. Quedarse con la última lectura de varias lo resuelve sin inventarse
    un tiempo de espera fijo."""
    end = time.time() + secs
    flags = kw.pop("flags", FLAG_QUAT | FLAG_STICK | FLAG_EXT)
    g = None
    while time.time() < end:
        seq[0] += 1
        udp.sendto(input_packet(session, seq[0], flags, buttons, **kw), (HOST, PORT))
        time.sleep(0.05)
        g = xstate(PAD, ex=True)
    return g

# ================================================================= prueba
s1, f1, ok1 = hello({"code": "1234", "name": "MandoUniversal"})
check(ok1.get("m") == "ok" and ok1.get("slot") == 0, f"emparejado: {ok1}")
check("gamepad" in ok1.get("modes", []), f"ok.modes anuncia gamepad: {ok1.get('modes')}")
sess = ok1["session_id"]

if _get_state is None:
    done("SKIP: sin XInput en este sistema")
if ok1.get("rumble") != "ready":
    done(f"SKIP: sin mando virtual (rumble={ok1.get('rumble')!r}); instala el driver con `PepoMote.exe --install-driver`")

antes = connected()
if len(antes) >= 4:
    done("SKIP: los 4 huecos de XInput están ocupados; no cabe el mando virtual")

# 1) El modo crea el mando: aparece un hueco de XInput que antes no estaba
s1.sendall(b'{"m":"mode","mode":"gamepad"}\n')
r = read_mode(f1)
check(r and r.get("mode") == "gamepad", f"modo mando universal aceptado ({r})")

PAD = None
end = time.time() + 6
while time.time() < end:
    seq[0] += 1
    udp.sendto(input_packet(sess, seq[0], FLAG_QUAT | FLAG_STICK | FLAG_EXT, 0), (HOST, PORT))
    nuevos = connected() - antes
    if nuevos:
        PAD = sorted(nuevos)[0]
        break
    time.sleep(0.05)
check(PAD is not None, "aparece un mando de Xbox 360 nuevo en XInput")
if PAD is None:
    done()
print(f"     (el mando virtual es XInput {PAD}; antes había {sorted(antes)})")

# 2) Cada botón del móvil llega a XInput como el suyo, y solo el suyo
PAREJAS = [("A", "A"), ("B", "B"), ("X", "X"), ("Y", "Y"),
           ("UP", "UP"), ("DOWN", "DOWN"), ("LEFT", "LEFT"), ("RIGHT", "RIGHT"),
           ("L", "LB"), ("R", "RB"), ("PLUS", "START"), ("MINUS", "BACK"),
           ("L3", "LTHUMB"), ("R3", "RTHUMB")]
for pmp_name, x_name in PAREJAS:
    g = hold(sess, BTN[pmp_name], 0.2)
    got = g.wButtons if g else -1
    check(got == XB[x_name], f"{pmp_name} → XInput {x_name} (leído {got:#06x}, esperado {XB[x_name]:#06x})")

# 3) Lo que no es del mando no pulsa nada
g = hold(sess, BTN["ONE"] | BTN["TWO"] | BTN["MIC"], 0.2)
check(g is not None and g.wButtons == 0 and g.bLeftTrigger == 0,
      f"los botones que no existen en un Xbox no pulsan nada ({g.wButtons:#06x})")

# 4) Gatillos analógicos
g = hold(sess, BTN["ZL"], 0.2)
check(g is not None and g.bLeftTrigger == 255 and g.wButtons == 0, f"ZL → gatillo izquierdo a fondo ({g.bLeftTrigger})")
g = hold(sess, BTN["ZR"], 0.2)
check(g is not None and g.bRightTrigger == 255 and g.bLeftTrigger == 0, f"ZR → gatillo derecho a fondo ({g.bRightTrigger})")

# 5) Sticks: signo y recorrido. Arriba y derecha son positivos en los dos.
g = hold(sess, 0, 0.2, stick=(127, 127))
check(g is not None and g.sThumbLX > 32000 and g.sThumbLY > 32000,
      f"stick izquierdo arriba-derecha ({g.sThumbLX}, {g.sThumbLY})")
g = hold(sess, 0, 0.2, stick=(-127 & 0xFF, -127 & 0xFF))
check(g is not None and g.sThumbLX < -32000 and g.sThumbLY < -32000,
      f"stick izquierdo abajo-izquierda ({g.sThumbLX}, {g.sThumbLY})")
g = hold(sess, 0, 0.2, stick2=(127, 0))
check(g is not None and g.sThumbRX > 32000, f"stick derecho táctil a la derecha ({g.sThumbRX})")

# 6) El giro mueve el stick derecho, y el dedo le gana
g = hold(sess, 0, 0.3, gyro=(0.0, 0.0, 4.0))
giro_x = g.sThumbRX if g else 0
check(giro_x != 0, f"mover el móvil mueve el stick derecho ({giro_x})")
g = hold(sess, 0, 0.3, gyro=(0.0, 0.0, -4.0))
check(g is not None and (g.sThumbRX < 0) != (giro_x < 0), f"girar al otro lado lo manda al otro lado ({g.sThumbRX})")
g = hold(sess, 0, 0.3, stick2=(127, 0), gyro=(0.0, 0.0, -4.0))
check(g is not None and g.sThumbRX > 32000, f"el dedo en el stick derecho gana al giro ({g.sThumbRX})")

# 6b) Con el giro apagado en el móvil (la pregunta de la primera vez, o
# Ajustes → «Mover el móvil apunta»), el paquete sale con el giroscopio a
# cero: exactamente esto. El stick derecho se queda quieto y no cambia nada
# más — botones y stick izquierdo siguen llegando.
g = hold(sess, BTN["A"], 0.3, stick=(127, 0), gyro=(0.0, 0.0, 0.0))
check(g is not None and g.sThumbRX == 0 and g.sThumbRY == 0,
      f"con el giro apagado el móvil no mueve la cámara ({g.sThumbRX}, {g.sThumbRY})")
check(g is not None and g.wButtons == XB["A"] and g.sThumbLX > 32000,
      f"y con el giro apagado el resto del mando sigue igual ({g.wButtons:#06x}, {g.sThumbLX})")

# 7) Un móvil sin giroscopio real (apuntado por inclinación) no apunta
g = hold(sess, 0, 0.3, gyro=(0.0, 0.0, 4.0), flags=FLAG_QUAT | FLAG_STICK | FLAG_EXT | FLAG_TILT)
check(g is not None and g.sThumbRX == 0, f"sin giroscopio de verdad el stick no se mueve solo ({g.sThumbRX})")

# 8) Si el móvil se calla con un botón pulsado, no se queda clavado
hold(sess, BTN["A"], 0.2)
time.sleep(1.8)
g = xstate(PAD)
check(g is not None and g.wButtons == 0, f"un móvil que calla suelta el botón ({g.wButtons if g else 'sin mando'})")

# 9) Este modo no le escribe la configuración a nadie
tocados = []
for sub in ("Cemu", "Dolphin Emulator", "eden", "RetroArch"):
    d = os.path.join(APPDATA, sub)
    if os.path.isdir(d) and any(os.scandir(d)):
        tocados.append(sub)
check(not tocados, f"el mando universal no configura ningún emulador (tocados: {tocados})")

# 9b) Salir del modo con un botón pulsado no lo deja clavado: el mando
# virtual sobrevive (Dolphin también lo quiere, para la vibración), así que
# hay que soltarlo a mano o el emulador lo vería pulsado para siempre.
hold(sess, BTN["A"], 0.3)
s1.sendall(b'{"m":"mode","mode":"dolphin"}\n')
read_mode(f1)
time.sleep(0.6)
g = xstate(PAD, ex=True)
check(g is not None and g.wButtons == 0, f"al cambiar de modo el mando se suelta ({g.wButtons if g else 'sin mando'}); ")
s1.sendall(b'{"m":"mode","mode":"gamepad"}\n')
read_mode(f1)
g = hold(sess, BTN["B"], 0.4)
check(g is not None and g.wButtons == XB["B"], f"y al volver el mando responde otra vez ({g.wButtons if g else -1:#06x})")

# El Guide va el ÚLTIMO a propósito: es el botón Xbox, y Windows lo captura
# para la barra de juego. Pulsarlo deja de vernos el mando en ESTE proceso
# (el receptor sigue escribiéndolo tan ricamente, lo dice su log), así que
# en medio de la tanda envenenaba todo lo que viniera detrás.
if _get_state_ex:
    g = hold(sess, BTN["HOME"], 0.2)
    check(g is not None and g.wButtons == XB["GUIDE"], f"Home → Guide (leído {g.wButtons:#06x})")
else:
    print("SKIP  Home → Guide: este Windows no expone XInputGetStateEx")

# 10) Al irse el móvil, el mando desaparece del sistema (desde fuera: ver
# el comentario de `connected_fresh`, este proceso tiene la lista cacheada)
# Ojo: en Python el socket NO se cierra de verdad mientras viva el
# `makefile` que cuelga de él, así que hay que cerrar los dos o el receptor
# no se entera de que el móvil se ha ido.
f1.close()
s1.close()
end = time.time() + 8
while time.time() < end and PAD in connected_fresh():
    time.sleep(0.3)
check(PAD not in connected_fresh(), "al desconectar el móvil el mando virtual se desenchufa")

done()
