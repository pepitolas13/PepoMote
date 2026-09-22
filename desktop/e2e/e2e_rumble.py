"""e2e de la vibración de los juegos (Windows): un móvil simulado por TCP/UDP
reales, el receptor en modo Dolphin y, al otro lado, el mando de Xbox 360
virtual al que se le escribe la vibración **desde fuera** con XInputSetState,
igual que hace Dolphin. Se comprueba lo que le llega al móvil (los RUMBLE de
PROTOCOL.md §4.5):

1. Ráfagas con el patrón de Dolphin (cada cambio son dos llamadas seguidas:
   primero el motor L y luego el R) que acaban en «apaga»: el último RUMBLE
   es un cero y no llega ninguno con nivel después. Hasta la 1.13.2 el
   receptor dejaba UNA sola petición de aviso esperando en el driver, y el
   segundo aviso de un cambio se podía perder: el móvil se quedaba vibrando.
2. Encender y callar (Dolphin en pausa, el juego cerrado): el receptor lo da
   por acabado a los 10 s y el móvil para; otro aviso lo vuelve a encender.
3. Un cambio en una sola llamada (como Cemu), y la retirada del mando al
   cambiar de modo aunque el emulador le siga escribiendo mientras se va.

Requiere el receptor aislado con PEPOMOTE_PAIR_CODE=1234 y el driver del
mando virtual. Sin driver o sin XInput se salta y sale con 0, salvo con
PEPOMOTE_E2E_REQUIRE_GAMEPAD=1 (un PC con el driver, antes de publicar).
PEPOMOTE_E2E_RUMBLE_EPISODES cambia el número de ráfagas (25 de serie).

Uso: python e2e_rumble.py"""
import ctypes, json, os, random, socket, struct, sys, threading, time

HOST = "127.0.0.1"
PORT = int(os.environ.get("PEPOMOTE_PORT", "26761"))
EPISODES = int(os.environ.get("PEPOMOTE_E2E_RUMBLE_EPISODES", "25"))
MAGIC = 0x31504D50
FLAG_QUAT = 1
TYPE_INPUT, TYPE_RUMBLE = 1, 4

fails = 0
def check(cond, msg):
    global fails
    print(("OK   " if cond else "FAIL ") + msg, flush=True)
    if not cond:
        fails += 1

def done(extra=""):
    global fails
    if extra:
        if extra.startswith("SKIP") and os.environ.get("PEPOMOTE_E2E_REQUIRE_GAMEPAD"):
            print("FAIL " + extra + " (PEPOMOTE_E2E_REQUIRE_GAMEPAD: no se admite saltarlo)")
            fails += 1
        else:
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

class XVibration(ctypes.Structure):
    _fields_ = [("wLeftMotorSpeed", ctypes.c_ushort), ("wRightMotorSpeed", ctypes.c_ushort)]

_get_state = _set_state = None
for name in ("XInput1_4.dll", "XInput1_3.dll", "XInput9_1_0.dll"):
    try:
        _xi = ctypes.WinDLL(name)
        _get_state, _set_state = _xi.XInputGetState, _xi.XInputSetState
        break
    except (OSError, AttributeError):
        continue

def connected():
    st = XState()
    return {i for i in range(4) if _get_state(ctypes.c_uint(i), ctypes.byref(st)) == 0}

PAD = None

def motors(left, right):
    """Una llamada a XInputSetState en el mando virtual (y solo en él)."""
    v = XVibration(left, right)
    return _set_state(ctypes.c_uint(PAD), ctypes.byref(v)) == 0

def dolphin(on):
    """Un cambio de Dolphin: `Motor L | Motor R` escribe primero L y luego R,
    cada uno con su propia llamada (Motor::SetState → UpdateMotors)."""
    if on:
        motors(0xFFFF, 0)
        motors(0xFFFF, 0xFFFF)
    else:
        motors(0, 0xFFFF)
        motors(0, 0)

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

def input_packet(session, seq):
    b = bytearray(72)
    struct.pack_into("<I", b, 0, MAGIC)
    b[4] = TYPE_INPUT
    b[5] = FLAG_QUAT
    struct.pack_into("<I", b, 8, session)
    struct.pack_into("<I", b, 12, seq)
    struct.pack_into("<Q", b, 16, 1_000_000 + seq * 4000)
    struct.pack_into("<4f", b, 24, 1, 0, 0, 0)
    struct.pack_into("<3f", b, 52, 0.0, 0.0, 9.80665)
    b[69] = 90
    return bytes(b)

udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
udp.bind((HOST, 0))
udp.settimeout(0.2)
# Lo que le llega al móvil: (instante, seq, fuerte, débil, ttl)
rumbles = []
lock = threading.Lock()
stop = threading.Event()

def sender(session):
    """El móvil manda INPUT a 50 Hz: así el receptor sabe a qué puerto
    contestar y la sesión sigue viva."""
    seq = 0
    while not stop.is_set():
        seq += 1
        try:
            udp.sendto(input_packet(session, seq), (HOST, PORT))
        except OSError:
            return
        time.sleep(0.02)

def receiver():
    while not stop.is_set():
        try:
            data, _ = udp.recvfrom(2048)
        except socket.timeout:
            continue
        except OSError:
            return
        if len(data) == 20 and struct.unpack_from("<I", data, 0)[0] == MAGIC and data[4] == TYPE_RUMBLE:
            seq, = struct.unpack_from("<I", data, 12)
            ttl, = struct.unpack_from("<H", data, 18)
            with lock:
                rumbles.append((time.monotonic(), seq, data[16], data[17], ttl))

def since(t0):
    with lock:
        return [r for r in rumbles if r[0] >= t0]

def vibrating(r):
    return r[2] != 0 or r[3] != 0

def last_after(t0):
    """El último RUMBLE (por seq) de los llegados desde t0."""
    got = since(t0)
    return max(got, key=lambda r: r[1]) if got else None

def wait_for(pred, t0, timeout):
    """Primer RUMBLE desde t0 que cumpla pred, esperando hasta timeout."""
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        hit = [r for r in since(t0) if pred(r)]
        if hit:
            return hit[0]
        time.sleep(0.01)
    return None

# ================================================================= prueba
s1, f1, ok1 = hello({"code": "1234", "name": "Vibra"})
check(ok1.get("m") == "ok" and ok1.get("slot") == 0, f"emparejado: {ok1}")
sess = ok1["session_id"]
if _set_state is None:
    done("SKIP: sin XInput en este sistema")
if ok1.get("rumble") != "ready":
    done(f"SKIP: sin mando virtual (rumble={ok1.get('rumble')!r}); instala el driver con `PepoMote.exe --install-driver`")
antes = connected()
if len(antes) >= 4:
    done("SKIP: los 4 huecos de XInput están ocupados; no cabe el mando virtual")

threading.Thread(target=sender, args=(sess,), daemon=True).start()
threading.Thread(target=receiver, daemon=True).start()

s1.sendall(b'{"m":"mode","mode":"dolphin"}\n')
r = read_mode(f1)
check(r and r.get("mode") == "dolphin", f"modo Dolphin aceptado ({r})")
end = time.time() + 6
while time.time() < end and PAD is None:
    nuevos = connected() - antes
    if nuevos:
        PAD = sorted(nuevos)[0]
    time.sleep(0.05)
check(PAD is not None, "aparece el mando virtual del jugador 1 en XInput")
if PAD is None:
    done()
print(f"     (el mando virtual es XInput {PAD}; antes había {sorted(antes)})", flush=True)
time.sleep(0.5)

try:
    # 0) El camino entero: lo que se escribe en el mando llega al móvil
    t = time.monotonic()
    check(motors(0, 0), "XInputSetState en el mando virtual")
    dolphin(True)
    hit = wait_for(vibrating, t, 1.0)
    check(hit is not None and hit[2] == 255, f"encender el motor hace vibrar el móvil ({hit})")
    dolphin(False)
    time.sleep(0.8)

    # 1) Ráfagas de Dolphin que acaban en «apaga»
    atascos = []
    for ep in range(EPISODES):
        t_ep = time.monotonic()
        for _ in range(random.randint(2, 6)):
            dolphin(True)
            time.sleep(random.uniform(0.005, 0.05))
            dolphin(False)
            time.sleep(random.uniform(0.005, 0.05))
        t_off = time.monotonic()
        time.sleep(0.7)
        con_nivel = [r for r in since(t_off + 0.15) if vibrating(r)]
        ult = last_after(t_ep)
        if con_nivel or ult is None or vibrating(ult):
            atascos.append((ep, len(con_nivel), ult))
    check(not atascos, f"{EPISODES} ráfagas de Dolphin acaban con el móvil parado (atascadas: {atascos[:5]}{'…' if len(atascos) > 5 else ''})")
    time.sleep(0.6)

    # 2) Encender y callar: como Dolphin en pausa o con el juego cerrado.
    # En un PC de verdad otros programas también escriben en los mandos
    # XInput: medido en el de Daniel, un 0/0 cada veinte segundos o así que
    # ni esta prueba ni PepoMote mandan (lo único abierto que toca mandos es
    # la Xbox Game Bar). Ese cero para el móvil antes de tiempo, que es lo
    # correcto, pero no deja medir los 10 s: se repite.
    for intento in range(3):
        t_on = time.monotonic()
        dolphin(True)
        time.sleep(9.0)
        ajenos = [r for r in since(t_on + 0.05) if not vibrating(r) and r[0] < t_on + 9.0]
        if not ajenos:
            break
        print(f"INFO a los {ajenos[0][0] - t_on:.2f} s llegó un 0/0 que no mandó la prueba (otro programa escribe en el mando); se repite", flush=True)
        dolphin(False)
        time.sleep(1.0)
    check(any(vibrating(r) for r in since(t_on + 8.5)), "a los 9 s sigue vibrando: el receptor refresca")
    cero = wait_for(lambda r: not vibrating(r), t_on + 9.0, 3.0)
    tarda = (cero[0] - t_on) if cero else None
    check(cero is not None and 9.9 <= tarda <= 10.8,
          f"encendido y callado: el receptor lo da por acabado a los 10 s ({'nunca' if tarda is None else f'{tarda:.2f} s'})")
    time.sleep(1.2)
    tras = [r for r in since((cero[0] if cero else t_on + 10.0) + 0.15) if vibrating(r)]
    check(not tras, f"y después no vuelve a vibrar ({len(tras)} RUMBLE con nivel)")

    # Otro aviso lo vuelve a encender, aunque traiga el mismo nivel
    t = time.monotonic()
    motors(0xFFFF, 0xFFFF)
    hit = wait_for(vibrating, t, 0.6)
    if hit is None:
        print("INFO XInput no reenvía al driver un estado igual al anterior; se prueba con un cambio", flush=True)
        dolphin(False)
        t = time.monotonic()
        dolphin(True)
        hit = wait_for(vibrating, t, 0.6)
    check(hit is not None, "el siguiente aviso del emulador vuelve a encenderlo")
    t = time.monotonic()
    dolphin(False)
    time.sleep(0.4)
    ult = last_after(t)
    check(ult is not None and not vibrating(ult), f"y el «apaga» lo para ({ult})")
    time.sleep(0.6)

    # 3) Un cambio en una sola llamada, como Cemu
    t = time.monotonic()
    motors(0x8000, 0x8000)
    hit = wait_for(vibrating, t, 0.6)
    check(hit is not None and hit[2] == 128, f"un cambio en una sola llamada llega ({hit})")
    t = time.monotonic()
    motors(0, 0)
    time.sleep(0.7)
    ult = last_after(t)
    tras = [r for r in since(t + 0.15) if vibrating(r)]
    check(ult is not None and not vibrating(ult) and not tras, "y se para con una sola llamada")

    # 4) Retirar el mando (otro modo) mientras el emulador aún le escribe: ni
    # su cero se pierde ni un aviso tardío vuelve a encender al jugador
    dolphin(True)
    time.sleep(0.3)
    t_mode = time.monotonic()
    s1.sendall(b'{"m":"mode","mode":"pointer"}\n')
    fin = time.monotonic() + 0.3
    while time.monotonic() < fin:
        dolphin(False)
        dolphin(True)
        time.sleep(0.005)
    r = read_mode(f1)
    time.sleep(1.5)
    tras = [x for x in since(t_mode + 0.6) if vibrating(x)]
    check(not tras, f"al retirar el mando el móvil para y ningún aviso tardío lo enciende ({len(tras)} RUMBLE con nivel)")
finally:
    try:
        if PAD is not None:
            motors(0, 0)
    except Exception:
        pass
    stop.set()
    f1.close()
    s1.close()

done()
