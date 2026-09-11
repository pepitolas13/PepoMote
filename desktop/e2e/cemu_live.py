"""Móvil simulado contra el receptor REAL (token de %APPDATA%): modo Wii U y
un bucle de pulsaciones para verlas en el diálogo de mandos de Cemu.
Uso: python cemu_live.py [patrón]   patrón = idle | buttons | sticks | touch | all
Escribe en stdout lo que hace y se para con Ctrl+C. Un archivo 'cemu_live.cmd'
en el scratchpad cambia el patrón en caliente (contenido = nombre del patrón)."""
import json, os, socket, struct, sys, threading, time

HOST = "127.0.0.1"; PORT = 26761
SP = os.path.dirname(os.path.abspath(__file__))
FLAG_QUAT, FLAG_STICK, FLAG_EXT, FLAG_TOUCH = 1, 2, 4, 8
BTN = dict(A=1 << 0, B=1 << 1, UP=1 << 2, DOWN=1 << 3, LEFT=1 << 4, RIGHT=1 << 5, PLUS=1 << 6, MINUS=1 << 7, HOME=1 << 8,
           X=1 << 19, Y=1 << 20, L=1 << 21, R=1 << 22, ZL=1 << 23, ZR=1 << 24, L3=1 << 25, R3=1 << 26, MIC=1 << 27, SCREEN=1 << 28)

# OJO: el python de este PC es un paquete MSIX y ve un AppData\Roaming
# virtualizado (otro token.txt): el token real llega por variable de entorno.
token = os.environ.get("PEPOMOTE_TOKEN") or open(os.path.join(os.environ["APPDATA"], "pepotech", "PepoMote", "config", "token.txt"), encoding="utf-8").read().strip()
s = socket.create_connection((HOST, PORT), timeout=5)
s.sendall((json.dumps({"m": "hello", "pv": 1, "token": token, "name": "MovilPrueba", "model": "Simulado"}) + "\n").encode())
f = s.makefile("r", encoding="utf-8")
ok = json.loads(f.readline())
print("ok:", ok, flush=True)
if ok.get("m") != "ok":
    sys.exit(1)
session = ok["session_id"]

def reader():
    while True:
        try:
            line = f.readline()
        except OSError:
            return
        if not line: return
        msg = json.loads(line)
        if msg.get("m") != "pong":
            print("<-", msg, flush=True)
threading.Thread(target=reader, daemon=True).start()

def beat():
    while True:
        time.sleep(0.8)
        try: s.sendall(b'{"m":"ping","t":1}\n')
        except OSError: return
threading.Thread(target=beat, daemon=True).start()

# Primero un paso por modo Dolphin con un solo mando: deja WiimoteNew.ini
# como debe (Wiimote1 sin Nunchuk, los demás en Ninguno) tras los e2e
if os.environ.get("PEPOMOTE_HEAL_DOLPHIN"):
    s.sendall(b'{"m":"mode","mode":"dolphin"}\n')
    time.sleep(2.5)
s.sendall(b'{"m":"mode","mode":"cemu"}\n')

udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
seq = 0
t0 = time.time()
pattern = sys.argv[1] if len(sys.argv) > 1 else "idle"
cmd_file = os.path.join(SP, "cemu_live.cmd")

def packet(buttons, stick, stick2, touch, touch_down):
    global seq
    seq += 1
    flags = FLAG_QUAT | FLAG_STICK | FLAG_EXT | (FLAG_TOUCH if touch_down else 0)
    b = bytearray(80)
    struct.pack_into("<I", b, 0, 0x31504D50); b[4] = 1; b[5] = flags
    b[6] = stick[0] & 0xFF; b[7] = stick[1] & 0xFF
    struct.pack_into("<I", b, 8, session); struct.pack_into("<I", b, 12, seq)
    struct.pack_into("<Q", b, 16, int((time.time() - t0) * 1e6) + 1_000_000)
    struct.pack_into("<4f", b, 24, 1, 0, 0, 0)
    struct.pack_into("<3f", b, 40, 0, 0, 0); struct.pack_into("<3f", b, 52, 0, 0, 9.80665)
    struct.pack_into("<I", b, 64, buttons); b[69] = 88
    b[72] = stick2[0] & 0xFF; b[73] = stick2[1] & 0xFF
    struct.pack_into("<H", b, 74, touch[0]); struct.pack_into("<H", b, 76, touch[1])
    return bytes(b)

SEQ_BUTTONS = ["A", "B", "X", "Y", "L", "R", "ZL", "ZR", "PLUS", "MINUS", "HOME", "UP", "DOWN", "LEFT", "RIGHT", "L3", "R3", "MIC", "SCREEN"]
last_print = ""
while True:
    if os.path.exists(cmd_file):
        try:
            pattern = open(cmd_file).read().strip() or pattern
        except OSError:
            pass
    t = time.time() - t0
    buttons, stick, stick2, touch, down = 0, (0, 0), (0, 0), (0, 0), False
    if pattern in ("buttons", "all"):
        name = SEQ_BUTTONS[int(t / 1.5) % len(SEQ_BUTTONS)]
        buttons = BTN[name]
        desc = f"boton {name}"
    elif pattern == "sticks":
        import math
        a = t * 1.5
        stick = (int(120 * math.cos(a)), int(120 * math.sin(a)))
        stick2 = (int(-120 * math.cos(a)), int(-120 * math.sin(a)))
        desc = f"sticks {stick} {stick2}"
    elif pattern == "touch":
        touch = (int(65535 * ((t / 4) % 1.0)), 32767); down = True
        desc = f"touch {touch}"
    else:
        desc = "idle"
    if pattern == "all":
        stick = (100, 0); stick2 = (0, 100)
    if desc != last_print:
        print(desc, flush=True); last_print = desc
    udp.sendto(packet(buttons, stick, stick2, touch, down), (HOST, PORT))
    time.sleep(0.01)
