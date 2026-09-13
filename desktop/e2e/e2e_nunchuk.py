"""e2e del receptor en Windows: mando + Nunchuk por TCP/UDP reales y lectura
del pad DSU del Nunchuk como haría Dolphin. Requiere el receptor arrancado
con PEPOMOTE_PAIR_CODE=1234 (puerto por defecto 26761, DSU 26760)."""
import json, socket, struct, sys, time, zlib

# Consola de Windows en cp1252: los mensajes llevan flechas y acentos
if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")

import os
HOST = "127.0.0.1"; PORT = int(os.environ.get("PEPOMOTE_PORT", "26761")); DSU = int(os.environ.get("PEPOMOTE_DSU_PORT", "26760"))

def hello(extra):
    s = socket.create_connection((HOST, PORT), timeout=5)
    msg = {"m": "hello", "pv": 1, "name": extra.pop("name", "e2e"), "model": "e2e"}
    msg.update(extra)
    s.sendall((json.dumps(msg) + "\n").encode())
    f = s.makefile("r")
    ok = json.loads(f.readline())
    while ok.get("m") == "notice":  # un aviso del receptor puede colarse antes del ok
        ok = json.loads(f.readline())
    return s, f, ok

def input_packet(session, seq, flags, buttons, stick=(0, 0), t=1_000_000):
    b = bytearray(72)
    struct.pack_into("<I", b, 0, 0x31504D50); b[4] = 1; b[5] = flags
    b[6] = stick[0] & 0xFF; b[7] = stick[1] & 0xFF
    struct.pack_into("<I", b, 8, session); struct.pack_into("<I", b, 12, seq)
    struct.pack_into("<Q", b, 16, t + seq * 4000)
    struct.pack_into("<4f", b, 24, 1, 0, 0, 0)
    struct.pack_into("<3f", b, 40, 0, 0, 0); struct.pack_into("<3f", b, 52, 0, 0, 9.80665)
    struct.pack_into("<I", b, 64, buttons); b[69] = 90
    return bytes(b)

def dsu_request(pad):
    payload = struct.pack("<IBB6s", 0x100002, 1, pad, b"\0" * 6)
    hdr = bytearray(b"DSUC" + struct.pack("<HHII", 1001, len(payload), 0, 0xE2E)) + payload
    crc = zlib.crc32(bytes(hdr)) & 0xFFFFFFFF
    struct.pack_into("<I", hdr, 8, crc)
    return bytes(hdr)

def recv(f, m, limit=20):
    """Siguiente mensaje con `m` (los avisos `notice` del receptor se saltan)."""
    for _ in range(limit):
        msg = json.loads(f.readline())
        if msg.get("m") == m:
            return msg
    return {}

fails = 0
def check(cond, msg):
    global fails
    print(("OK   " if cond else "FAIL ") + msg)
    if not cond: fails += 1

# 1) mando: emparejar por código y conectar
s1, f1, ok1 = hello({"code": "1234", "name": "MandoE2E"})
check(ok1.get("m") == "ok" and ok1.get("slot") == 0, f"mando: {ok1}")
token = ok1.get("token")
check(ok1.get("role") == "wiimote" and ok1.get("player") == 1, "mando: role wiimote, jugador 1")
# 2) Nunchuk con el token
s2, f2, ok2 = hello({"token": token, "role": "nunchuk", "name": "NunchukE2E"})
check(ok2.get("m") == "ok" and ok2.get("slot") == 3, f"nunchuk: slot 3 ({ok2})")
check(ok2.get("role") == "nunchuk" and ok2.get("player") == 1, "nunchuk: role nunchuk, jugador 1")
# 3) el mando pone modo Dolphin; el Nunchuk no puede cambiarlo
s2.sendall(b'{"m":"mode","mode":"pointer"}\n'); r = recv(f2, "mode")
check(r.get("mode") == ok1.get("mode"), f"nunchuk pide modo: se le contesta el vigente ({r.get('mode')} == {ok1.get('mode')})")
s1.sendall(b'{"m":"mode","mode":"dolphin"}\n'); r = recv(f1, "mode")
check(r.get("mode") == "dolphin", "mando: modo dolphin aceptado")
# 4) UDP: telemetría de los dos + cliente DSU suscrito al pad 3
udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); udp.settimeout(1)
dsu = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); dsu.settimeout(1)
dsu.sendto(dsu_request(3), (HOST, DSU))
got = None
for seq in range(1, 120):
    udp.sendto(input_packet(ok1["session_id"], seq, 0x01, 0x1), (HOST, PORT))          # mando: A
    udp.sendto(input_packet(ok2["session_id"], seq, 0x03, (1 << 17) | (1 << 18), (100, -50)), (HOST, PORT))
    if seq % 20 == 0:
        dsu.sendto(dsu_request(3), (HOST, DSU))
    try:
        d, _ = dsu.recvfrom(256)
    except socket.timeout:
        continue
    if d[:4] == b"DSUS" and struct.unpack_from("<I", d, 16)[0] == 0x100002 and d[20] == 3:
        got = d; break
    time.sleep(0.004)
check(got is not None, "DSU: PadData del pad 3 (Nunchuk) recibido")
if got:
    check(got[21] == 2 and got[31] == 1, "DSU: pad 3 conectado")
    check(list(got[40:44]) == [228, 78, 128, 128], f"DSU: stick LX/LY = 228/78 (got {list(got[40:44])})")
    check(got[53] == 0xFF and got[52] == 0xFF, "DSU: C→L1 y Z→R1 analógicos")
    check(list(got[48:52]) == [0, 0, 0, 0], "DSU: A/B intactos (C/Z ya no son Cross/Circle)")
    check(got[37] & (3 << 2) == (3 << 2), "DSU: C/Z en el bitmask (L1/R1)")
# 5) pad 0 (mando) sigue con stick neutro
dsu0 = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); dsu0.settimeout(1)
dsu0.sendto(dsu_request(0), (HOST, DSU)); got0 = None
for seq in range(120, 200):
    udp.sendto(input_packet(ok1["session_id"], seq, 0x01, 0x1), (HOST, PORT))
    try:
        d, _ = dsu0.recvfrom(256)
    except socket.timeout:
        continue
    if d[:4] == b"DSUS" and d[20] == 0: got0 = d; break
check(got0 is not None and list(got0[40:44]) == [128, 128, 128, 128], "DSU: pad 0 (mando) con sticks neutros")
# 6) Nunchuk en el mismo móvil: un tercer móvil, mando con "nunchuk":"own"
s3, f3, ok3 = hello({"token": token, "nunchuk": "own", "name": "MandoNunchukE2E"})
check(ok3.get("m") == "ok" and ok3.get("slot") == 1, f"mando+nunchuk: slot 1 ({ok3})")
check(ok3.get("nunchuk") == "own", "mando+nunchuk: el receptor confirma nunchuk=own")
check(ok1.get("nunchuk") == "none", "mando sin Nunchuk propio: nunchuk=none")
dsu1 = socket.socket(socket.AF_INET, socket.SOCK_DGRAM); dsu1.settimeout(1)
dsu1.sendto(dsu_request(1), (HOST, DSU)); got1 = None
for seq in range(1, 120):
    # A + C + Z con stick: todo en la misma trama de 72 bytes
    udp.sendto(input_packet(ok3["session_id"], seq, 0x03, 0x1 | (1 << 17) | (1 << 18), (100, -50)), (HOST, PORT))
    if seq % 20 == 0:
        dsu1.sendto(dsu_request(1), (HOST, DSU))
    try:
        d, _ = dsu1.recvfrom(256)
    except socket.timeout:
        continue
    if d[:4] == b"DSUS" and struct.unpack_from("<I", d, 16)[0] == 0x100002 and d[20] == 1:
        got1 = d; break
    time.sleep(0.004)
check(got1 is not None, "DSU: PadData del pad 1 (mando + Nunchuk) recibido")
if got1:
    check(list(got1[40:44]) == [228, 78, 128, 128], f"DSU: stick del Nunchuk en el pad del mando (got {list(got1[40:44])})")
    check(got1[49] == 0xFF and got1[53] == 0xFF and got1[52] == 0xFF, "DSU: A→Cross, C→L1 y Z→R1 distinguibles en el mismo pad")
# 7) el mensaje nunchuk apaga y enciende el Nunchuk propio (eco)
s3.sendall(b'{"m":"nunchuk","own":false}\n'); r = recv(f3, "nunchuk")
check(r.get("m") == "nunchuk" and r.get("own") is False, f"nunchuk off: eco {r}")
s3.sendall(b'{"m":"nunchuk","own":true}\n'); r = recv(f3, "nunchuk")
check(r.get("m") == "nunchuk" and r.get("own") is True, f"nunchuk on: eco {r}")
s2.sendall(b'{"m":"nunchuk","own":true}\n'); r = recv(f2, "nunchuk")
check(r.get("own") is False, "un Nunchuk (rol) no puede llevar Nunchuk propio")
for s in (s1, s2, s3):
    try: s.sendall(b'{"m":"bye"}\n'); s.close()
    except OSError: pass
print("FAILS:", fails); sys.exit(1 if fails else 0)
