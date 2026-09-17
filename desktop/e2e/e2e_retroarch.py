"""Real TCP/UDP RetroArch integration against an isolated running receiver.

A fake RetroArch listens on the network-gamepad ports (one datagram per
player per frame, exactly like input_driver.c) and on the command interface
(VERSION replies; GET_STATUS, which must only reach versions after 1.22.2
because it crashes the released ones; hotkeys; SHOW_MSG). The script also plays
RetroArch's history playlist (content_history.lpl) and core info files, from
which the receiver announces the loaded game and its console (`game`). Optionally it empties the
pad on frames without a datagram, which is what a Windows build does.

Set PEPOMOTE_RETROARCH_DIR, PEPOMOTE_PORT, PEPOMOTE_RETROARCH_PORT,
PEPOMOTE_RETROARCH_CMD_PORT and pairing code 1234. No emulator or phone is
required. Does not alter a real RetroArch install.
"""
import json
import os
from pathlib import Path
import queue
import socket
import struct
import threading
import time

HOST = "127.0.0.1"
PORT = int(os.environ.get("PEPOMOTE_PORT", "26771"))
BASE = int(os.environ.get("PEPOMOTE_RETROARCH_PORT", "55400"))
CMD = int(os.environ.get("PEPOMOTE_RETROARCH_CMD_PORT", "55355"))
RA_DIR = Path(os.environ["PEPOMOTE_RETROARCH_DIR"])
CONFIG = RA_DIR / "retroarch.cfg"
HISTORY = RA_DIR / "playlists" / "builtin" / "content_history.lpl"
SEP = "\\" if os.name == "nt" else "/"
CORE_EXT = ".dll" if os.name == "nt" else ".so"
COUNT = 0

# PROTOCOL.md §4.2
BTN_A, BTN_B, BTN_UP, BTN_DOWN, BTN_LEFT, BTN_RIGHT = 1, 2, 4, 8, 16, 32
BTN_PLUS, BTN_MINUS, BTN_HOME, BTN_ONE, BTN_TWO = 64, 128, 256, 512, 1024
BTN_X, BTN_Y, BTN_L, BTN_R, BTN_ZL, BTN_ZR = 1 << 19, 1 << 20, 1 << 21, 1 << 22, 1 << 23, 1 << 24
BTN_STICK_L, BTN_STICK_R, BTN_SCREEN = 1 << 25, 1 << 26, 1 << 28
# libretro.h RETRO_DEVICE_ID_JOYPAD_*
RP_B, RP_Y, RP_SELECT, RP_START, RP_UP, RP_DOWN, RP_LEFT, RP_RIGHT = range(8)
RP_A, RP_X, RP_L, RP_R, RP_L2, RP_R2, RP_L3, RP_R3 = range(8, 16)


def check(condition, label):
    global COUNT
    if not condition:
        raise AssertionError(label)
    COUNT += 1
    print("OK  " + label, flush=True)


def eventually(predicate, label, timeout=8):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if predicate():
            check(True, label)
            return
        time.sleep(0.01)
    raise AssertionError(label)


class FakeRetroArch(threading.Thread):
    """input_driver.c + command.c, reduced to what matters for pacing."""

    def __init__(self, users=4, fps=60.0):
        super().__init__(daemon=True)
        self.fps = fps
        self.users = users
        self.cmd = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.cmd.bind((HOST, CMD))
        self.cmd.setblocking(False)
        self.pads = []
        for u in range(users):
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.bind((HOST, BASE + u))
            s.setblocking(False)
            self.pads.append(s)
        self.lock = threading.Lock()
        self.buttons = [0] * users
        self.axes = [[0, 0, 0, 0] for _ in range(users)]
        self.history = []          # per frame: (buttons[], axes[], had_datagram[])
        self.commands = []         # (frame, command text)
        self.osd = []
        self.doubles = 0           # frames where a second datagram was already waiting
        self.reset_on_empty = False
        self.mute = False          # stop answering probes (RetroArch gone)
        self.stopped = False
        self.frames = 0
        self.probes = 0
        self.version = "1.22.2"        # what VERSION answers (the released RetroArch)
        self.status_requests = 0       # GET_STATUS received (crashes a real <= 1.22.2)
        self.status_after_probes = None  # probes answered before the first GET_STATUS

    def stop(self):
        self.stopped = True

    def frame(self):
        with self.lock:
            # Command interface: everything queued, replies to the sender
            while True:
                try:
                    data, src = self.cmd.recvfrom(2048)
                except (BlockingIOError, OSError):
                    break
                if self.mute:
                    continue
                for tok in data.decode("utf-8", "replace").split("\n"):
                    tok = tok.strip()
                    if not tok:
                        continue
                    if tok == "VERSION":
                        self.probes += 1
                        self.cmd.sendto((self.version + "\n").encode(), src)
                    elif tok == "GET_STATUS":
                        # A real RetroArch <= 1.22.2 dies here when the core is not in
                        # its info list (command_get_status copies a NULL); the fake
                        # only counts so that the check below names the regression
                        if self.status_after_probes is None:
                            self.status_after_probes = self.probes
                        self.probes += 1
                        self.status_requests += 1
                        self.cmd.sendto(b"GET_STATUS PLAYING snes9x,Test Game.sfc\n", src)
                    elif tok.startswith("SHOW_MSG "):
                        self.osd.append(tok[9:])
                    else:
                        self.commands.append((self.frames, tok))
            had = []
            for u, s in enumerate(self.pads):
                try:
                    data = s.recv(64)
                except (BlockingIOError, OSError):
                    data = None
                if data is not None and len(data) == 20:
                    port, device, index, ident, state = struct.unpack("<iiiiH", data[:18])
                    if device == 1 and ident < 16:
                        self.buttons[u] &= ~(1 << ident)
                        if state:
                            self.buttons[u] |= 1 << ident
                    elif device == 5 and ident < 2 and index < 2:
                        v = state - 65536 if state >= 32768 else state
                        self.axes[u][index * 2 + ident] = v
                    had.append(True)
                    try:
                        s.recv(64, socket.MSG_PEEK)
                        self.doubles += 1
                    except (BlockingIOError, OSError):
                        pass
                elif data is not None:
                    # wrong size: RetroArch empties the pad
                    self.buttons[u] = 0
                    self.axes[u] = [0, 0, 0, 0]
                    had.append(False)
                else:
                    if self.reset_on_empty:
                        self.buttons[u] = 0
                        self.axes[u] = [0, 0, 0, 0]
                    had.append(False)
            self.history.append((list(self.buttons), [list(a) for a in self.axes], had))
            self.frames += 1

    def run(self):
        period = 1.0 / self.fps
        nxt = time.perf_counter()
        while not self.stopped:
            self.frame()
            nxt += period
            delay = nxt - time.perf_counter()
            if delay > 0:
                time.sleep(delay)
            else:
                nxt = time.perf_counter()

    def snapshot(self):
        with self.lock:
            return self.frames, list(self.history), list(self.commands), list(self.osd), self.doubles

    def held_fraction(self, user, bit, since_frame):
        with self.lock:
            rows = self.history[since_frame:]
        if not rows:
            return 0.0
        return sum(1 for b, _, _ in rows if b[user] & (1 << bit)) / len(rows)

    def count_command(self, name, since_frame=0):
        with self.lock:
            return sum(1 for f, c in self.commands if f >= since_frame and c == name)


class Phone:
    def __init__(self, name, **extra):
        self.s = socket.create_connection((HOST, PORT), timeout=5)
        self.s.settimeout(None)
        self.q = queue.Queue()
        self.alive = True
        self.seq = 0
        self.write_lock = threading.Lock()
        self.send(m="hello", pv=1, name=name, model="RetroArch E2E", code="1234", **extra)
        threading.Thread(target=self.reader, daemon=True).start()
        self.ok = self.receive(lambda m: m.get("m") in ("ok", "err"))
        check(self.ok.get("m") == "ok", f"{name}: authenticated")
        self.session = self.ok["session_id"]
        threading.Thread(target=self.beat, daemon=True).start()

    def reader(self):
        try:
            with self.s.makefile("r", encoding="utf-8") as f:
                for line in f:
                    self.q.put(json.loads(line))
        except (OSError, ValueError):
            pass

    def beat(self):
        while self.alive:
            try:
                self.send(m="ping", t=1)
            except OSError:
                return
            time.sleep(0.5)

    def send(self, **message):
        with self.write_lock:
            self.s.sendall((json.dumps(message) + "\n").encode())

    def receive(self, pred, timeout=8):
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            try:
                m = self.q.get(timeout=max(0.001, until - time.monotonic()))
            except queue.Empty:
                break
            if pred(m):
                return m
        raise AssertionError("Missing receiver echo")

    def mode(self, name):
        self.send(m="mode", mode=name)
        return self.receive(lambda m: m.get("m") == "mode" and m.get("mode") == name)

    def pad(self, kind, effective=None):
        self.send(m="pad", pad=kind)
        return self.receive(lambda m: m.get("m") == "pad" and m.get("pad") == (effective or kind))

    def hotkey(self, name, down=True):
        self.send(m="hotkey", name=name, down=down)
        return self.receive(lambda m: m.get("m") == "hotkey" and m.get("name") == name)

    def close(self):
        self.alive = False
        try:
            self.send(m="bye")
            self.s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.s.close()


UDP = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)


def sample(phone, buttons=0, stick=(0, 0), right=(0, 0)):
    phone.seq += 1
    b = bytearray(80)
    struct.pack_into("<IBBbbIIQ", b, 0, 0x31504D50, 1, 1 | 2 | 4, *stick,
                     phone.session, phone.seq, 1_000_000 + phone.seq * 4000)
    struct.pack_into("<4f", b, 24, 1, 0, 0, 0)
    struct.pack_into("<3f", b, 52, 0, 0, 9.80665)
    struct.pack_into("<I", b, 64, buttons)
    b[69] = 90
    struct.pack_into("<bbHH", b, 72, *right, 0, 0)
    UDP.sendto(b, (HOST, PORT))


def hold(phone, seconds, buttons=0, stick=(0, 0), right=(0, 0), hz=100):
    """Send the same state at `hz` for `seconds` (a phone keeps streaming)."""
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        sample(phone, buttons, stick, right)
        time.sleep(1 / hz)


def write_info(core, systemid, corename, systemname):
    """info/<core>_libretro.info as RetroArch ships them (same key = "value" format)."""
    d = RA_DIR / "info"
    d.mkdir(parents=True, exist_ok=True)
    (d / f"{core}_libretro.info").write_text(
        f'display_name = "{systemname} ({corename})"\ncorename = "{corename}"\nsystemname = "{systemname}"\nsystemid = "{systemid}"\n',
        encoding="utf-8")


def write_history(*entries):
    """content_history.lpl with the given (path, core, label, db_name) entries, newest first."""
    HISTORY.parent.mkdir(parents=True, exist_ok=True)
    items = []
    for path, core, label, db_name in entries:
        items.append({"path": path, "label": label, "core_path": str(RA_DIR / "cores" / f"{core}_libretro{CORE_EXT}"),
                      "core_name": f"x ({core})", "crc32": "", "db_name": db_name})
    HISTORY.write_text(json.dumps({"version": "1.5", "default_core_path": "", "items": items}, indent=2), encoding="utf-8")


def quiet(phone, pred, seconds, label):
    """No message matching pred for `seconds` (others are dropped)."""
    until = time.monotonic() + seconds
    while time.monotonic() < until:
        try:
            m = phone.q.get(timeout=max(0.001, until - time.monotonic()))
        except queue.Empty:
            break
        if pred(m):
            raise AssertionError(f"{label}: unexpected {m}")
    check(True, label)


def is_game(console=None):
    return lambda m: m.get("m") == "game" and (console is None or m.get("console") == console)


def cfg_value(key):
    try:
        text = CONFIG.read_text(encoding="utf-8")
    except FileNotFoundError:
        return None
    for line in text.splitlines():
        if line.startswith(key + " ="):
            return line.split("=", 1)[1].strip().strip('"')
    return None


def main():
    original = (b'video_fullscreen = "true"\r\nnetwork_cmd_enable = "false"\r\naudio_volume = "0.0"\r\n'
                b'history_list_enable = "true"\r\n'
                + f'content_history_path = ":{SEP}playlists{SEP}builtin{SEP}content_history.lpl"\r\n'.encode()
                + f'libretro_info_path = ":{SEP}info"\r\n'.encode())
    CONFIG.write_bytes(original)
    # RetroArch's history: what it just loaded (Cave Story on Genesis Plus GX, a
    # multi-system core whose db_name lists every Sega system)
    write_info("genesis_plus_gx", "mega_drive", "Genesis Plus GX", "Sega 8/16-bit (Various)")
    write_info("fceumm", "nes", "FCEUmm", "Nintendo Entertainment System")
    write_info("mgba", "game_boy_advance", "mGBA", "Game Boy Advance")
    SEGA_DBS = "Sega - Game Gear|Sega - Master System - Mark III|Sega - Mega Drive - Genesis"
    CAVE = (str(RA_DIR / "roms" / "cave_story_v0.7.0.zip"), "genesis_plus_gx", "", SEGA_DBS)
    write_history(CAVE)
    ra = FakeRetroArch()
    ra.start()
    time.sleep(0.2)

    # --- setup: mode, pad vocabulary, retroarch.cfg
    p1 = Phone("Phone A")
    check("retroarch" in p1.ok["modes"], "ok.modes announces retroarch")
    p1.mode("retroarch")
    eventually(lambda: cfg_value("network_remote_enable") == "true", "retroarch.cfg: network gamepad enabled")
    check(cfg_value("network_remote_base_port") == str(BASE), "retroarch.cfg: our base port")
    check(cfg_value("network_cmd_enable") == "true", "retroarch.cfg: command interface enabled")
    check(cfg_value("network_cmd_port") == str(CMD), "retroarch.cfg: our command port")
    for u in range(1, 5):
        check(cfg_value(f"network_remote_enable_user_p{u}") == "true", f"retroarch.cfg: user {u} enabled")
    check(cfg_value("network_remote_enable_user_p5") is None, "retroarch.cfg: only four players")
    text = CONFIG.read_bytes()
    check(text.startswith(b'video_fullscreen = "true"\r\nnetwork_cmd_enable = "true"\r\naudio_volume = "0.0"\r\n'),
          "retroarch.cfg: other lines kept in place, CRLF kept")
    check(CONFIG.with_suffix(".cfg.pepomote.bak").read_bytes() == original, "backup byte for byte")
    mtime = CONFIG.stat().st_mtime_ns
    p1.mode("pointer")
    p1.mode("retroarch")
    time.sleep(0.6)
    check(CONFIG.stat().st_mtime_ns == mtime, "already configured: not rewritten")
    check(p1.pad("nes")["pad"] == "nes", "pad echo: nes")
    check(p1.pad("gun")["pad"] == "gun", "pad echo: gun")
    check(p1.pad("pro", effective="gun")["pad"] == "gun", "unknown pad in RetroArch mode: unchanged")
    check(p1.pad("retropad")["pad"] == "retropad", "pad echo: retropad")

    # --- the loaded game: history playlist + core info → `game` (console template on the phone)
    g = p1.receive(is_game(), timeout=6)
    check(g["console"] == "md" and g["system"] == "Mega Drive" and g["core"] == "Genesis Plus GX"
          and g["title"] == "cave_story_v0.7.0" and g["path"] == CAVE[0],
          f"game announced from the history: {g['title']} · {g['system']} ({g['core']})")
    write_history((str(RA_DIR / "roms" / "Zelda.nes"), "fceumm", "", "Nintendo - Nintendo Entertainment System.lpl"), CAVE)
    g = p1.receive(is_game(), timeout=6)
    check(g["console"] == "nes" and g["system"] == "NES" and g["core"] == "FCEUmm" and g["title"] == "Zelda",
          "new top entry (Zelda.nes / FCEUmm) → nes")
    time.sleep(0.05)
    write_history((str(RA_DIR / "roms" / "Zelda.nes"), "fceumm", "", "Nintendo - Nintendo Entertainment System.lpl"), CAVE)
    quiet(p1, is_game(), 4.5, "same entry rewritten (new mtime): no new announcement")
    write_history((str(RA_DIR / "roms" / "sonic.sms"), "picodrive", "", ""))
    g = p1.receive(is_game(), timeout=6)
    check(g["console"] == "ms" and g["core"] == "picodrive", "sonic.sms on PicoDrive (no info file) → ms by extension")
    write_history((str(RA_DIR / "roms" / "sonic.md"), "picodrive", "", ""))
    g = p1.receive(is_game(), timeout=6)
    check(g["console"] == "md" and g["system"] == "Mega Drive", "sonic.md on PicoDrive → md by the core name")
    write_history((str(RA_DIR / "roms" / "pack.zip") + "#Tetris.gb", "mgba", "", ""))
    g = p1.receive(is_game(), timeout=6)
    check(g["console"] == "gb" and g["title"] == "Tetris" and g["core"] == "mGBA", "zip member Tetris.gb on mGBA → gb, title from the member")
    write_history((str(RA_DIR / "roms" / "game.zip"), "dosbox_pure", "", ""))
    g = p1.receive(is_game(), timeout=6)
    check(g["console"] is None and g["core"] == "dosbox_pure" and g["title"] == "game", "unknown core (DOSBox Pure) → console null")
    time.sleep(0.05)
    HISTORY.write_text(HISTORY.read_text(encoding="utf-8")[:40], encoding="utf-8")
    quiet(p1, is_game(), 4.5, "half-written history: kept the last game, no announcement")
    write_history((str(RA_DIR / "roms" / "Zelda.nes"), "fceumm", "", ""))
    g = p1.receive(is_game(), timeout=6)
    check(g["console"] == "nes", "complete file again → nes")
    # the phone tells the receiver which template it shows (for the window); echoed back
    p1.send(m="pad", pad="retropad", layout="md")
    check(p1.receive(lambda m: m.get("m") == "pad")["layout"] == "md", "pad.layout md echoed")
    p1.send(m="pad", pad="retropad", layout="bogus")
    check(p1.receive(lambda m: m.get("m") == "pad")["layout"] is None, "unknown layout → null")
    p1.send(m="pad", pad="retropad")
    check(p1.receive(lambda m: m.get("m") == "pad")["layout"] is None, "no layout → null")
    # a phone that connects later gets the game right after ok
    p3 = Phone("Phone C")
    first = p3.q.get(timeout=3)
    check(first["m"] == "game" and first["console"] == "nes", "late phone: game is the first line after ok")
    p3.close()
    time.sleep(0.3)
    write_history(CAVE)
    p1.receive(is_game("md"), timeout=6)

    # --- clock: the receiver probes the command interface once RetroArch answers
    eventually(lambda: ra.probes >= 5, "receiver probes the command interface")
    eventually(lambda: any("PepoMote" in m for m in ra.snapshot()[3]), "SHOW_MSG on the RetroArch screen")
    since = ra.frames
    time.sleep(1.0)
    frames, hist, _, _, _ = ra.snapshot()
    check(frames - since >= 40, f"fake RetroArch runs (~60 fps): {frames - since} frames")

    # --- one button: next frame, latched, released
    f0 = ra.frames
    hold(p1, 0.5, buttons=BTN_A)
    first = next((i for i, (b, _, _) in enumerate(ra.snapshot()[1]) if i >= f0 and b[0] & (1 << RP_A)), None)
    check(first is not None and first - f0 <= 6, f"A arrives within a few frames ({first} vs {f0})")
    check(ra.held_fraction(0, RP_A, first + 1) > 0.8, "A stays pressed while held (no reset)")
    f1 = ra.frames
    hold(p1, 0.3, buttons=0)
    check(ra.held_fraction(0, RP_A, f1 + 6) == 0.0, "A released after the phone lets go")

    # --- pacing: never more than one datagram waiting per frame
    d0 = ra.doubles
    f2 = ra.frames
    for i in range(60):
        v = int(120 * ((i % 20) / 10.0 - 1.0))
        hold(p1, 0.02, stick=(v, -v), right=(-v, v))
    frames_now = ra.frames
    check(ra.doubles - d0 <= max(3, (frames_now - f2) // 10), f"no backlog: {ra.doubles - d0} double frames out of {frames_now - f2}")
    hold(p1, 0.1)

    # --- sticks: scale and sign
    hold(p1, 0.3, stick=(127, 0))
    check(ra.snapshot()[1][-1][1][0][0] == 32767, "left stick X full right = 32767")
    hold(p1, 0.3, stick=(0, 127))
    check(ra.snapshot()[1][-1][1][0][1] == -32767, "left stick up = -32767 (libretro +Y is down)")
    hold(p1, 0.3, right=(-127, -127))
    axes = ra.snapshot()[1][-1][1][0]
    check(axes[2] == -32767 and axes[3] == 32767, "right stick left/down")
    hold(p1, 0.3)
    check(ra.snapshot()[1][-1][1][0] == [0, 0, 0, 0], "sticks back to centre")

    # --- every RetroPad button from the two-stick pad
    for bit, rp, name in [(BTN_B, RP_B, "B"), (BTN_X, RP_X, "X"), (BTN_Y, RP_Y, "Y"), (BTN_L, RP_L, "L"),
                          (BTN_R, RP_R, "R"), (BTN_ZL, RP_L2, "ZL→L2"), (BTN_ZR, RP_R2, "ZR→R2"),
                          (BTN_STICK_L, RP_L3, "L3"), (BTN_STICK_R, RP_R3, "R3"), (BTN_PLUS, RP_START, "+→Start"),
                          (BTN_MINUS, RP_SELECT, "−→Select"), (BTN_UP, RP_UP, "up"), (BTN_DOWN, RP_DOWN, "down"),
                          (BTN_LEFT, RP_LEFT, "left"), (BTN_RIGHT, RP_RIGHT, "right")]:
        hold(p1, 0.12, buttons=bit)
        check(ra.snapshot()[1][-1][0][0] == 1 << rp, f"retropad: {name}")
    hold(p1, 0.15)
    check(ra.snapshot()[1][-1][0][0] == 0, "all released")

    # --- Home = menu (one toggle per press), Capture = fast-forward while held
    c0 = ra.frames
    hold(p1, 0.3, buttons=BTN_HOME)
    hold(p1, 0.1)
    check(ra.count_command("MENU_TOGGLE", c0) == 1, "Home → one MENU_TOGGLE")
    check(ra.snapshot()[1][-1][0][0] == 0, "Home is not a RetroPad button")
    c1 = ra.frames
    hold(p1, 0.5, buttons=BTN_SCREEN)
    n_ff = ra.count_command("FAST_FORWARD_HOLD", c1)
    check(n_ff >= 15, f"Capture held → FAST_FORWARD_HOLD every frame ({n_ff})")
    hold(p1, 0.2)
    c2 = ra.frames
    time.sleep(0.3)
    check(ra.count_command("FAST_FORWARD_HOLD", c2) == 0, "released → no more FAST_FORWARD_HOLD")

    # --- hotkeys by name
    c3 = ra.frames
    check(p1.hotkey("save_state")["ok"] is True, "hotkey echo: save_state ok")
    eventually(lambda: ra.count_command("SAVE_STATE", c3) == 1, "SAVE_STATE sent once")
    check(p1.hotkey("nope")["ok"] is False, "unknown hotkey rejected")
    check(p1.hotkey("rewind", True)["ok"] is True, "hotkey echo: rewind down")
    time.sleep(0.4)
    n_rw = ra.count_command("REWIND", c3)
    check(n_rw >= 12, f"REWIND repeated while held ({n_rw})")
    p1.hotkey("rewind", False)
    time.sleep(0.15)
    c4 = ra.frames
    time.sleep(0.3)
    check(ra.count_command("REWIND", c4) == 0, "REWIND stops when released")

    # --- Windows: a frame without a datagram empties the pad; keepalive must hide it
    ra.reset_on_empty = True
    f3 = ra.frames
    hold(p1, 1.0, buttons=BTN_A | BTN_RIGHT)
    both = 0
    rows = ra.snapshot()[1][f3 + 8:]
    for b, _, _ in rows:
        if b[0] & (1 << RP_A) and b[0] & (1 << RP_RIGHT):
            both += 1
    check(rows and both / len(rows) >= 0.9, f"with per-frame reset, A+Right held in {both}/{len(rows)} frames")
    hold(p1, 0.3)
    check(ra.snapshot()[1][-1][0][0] == 0, "released under reset mode too")

    # --- second player, NES pad, disconnect mid-press
    p2 = Phone("Phone B", pad="nes")
    check(p2.ok["slot"] == 1 and p2.ok["pad"] == "nes", "second phone: player 2, nes pad from hello")
    hold(p2, 0.3, buttons=BTN_ONE | BTN_TWO | BTN_RIGHT)
    b2 = ra.snapshot()[1][-1][0]
    check(b2[1] == (1 << RP_B) | (1 << RP_A) | (1 << RP_RIGHT), "player 2 on its own port: 1→B, 2→A, right")
    check(b2[0] == 0, "player 1 untouched")
    hold(p2, 0.2, buttons=BTN_A | BTN_B)
    check(ra.snapshot()[1][-1][0][1] == (1 << RP_X) | (1 << RP_Y), "nes: big A/B → X/Y")
    hold(p2, 0.2, buttons=BTN_TWO)
    p2.close()
    time.sleep(0.4)
    check(ra.snapshot()[1][-1][0][1] == 0, "phone gone mid-press: released")

    # --- mode change with something pressed releases it
    hold(p1, 0.2, buttons=BTN_B)
    check(ra.snapshot()[1][-1][0][0] == 1 << RP_B, "B pressed before leaving the mode")
    p1.mode("pointer")
    time.sleep(0.4)
    check(ra.snapshot()[1][-1][0][0] == 0, "leaving RetroArch mode releases B")
    p1.mode("retroarch")
    time.sleep(0.2)

    # --- RetroArch restarts (silence, then answers again): state is rebuilt
    ra.reset_on_empty = False
    hold(p1, 0.3, buttons=BTN_A)
    ra.mute = True
    time.sleep(0.25)  # whatever was already in flight lands
    with ra.lock:
        ra.buttons = [0] * ra.users
        f_mute = ra.frames
    hold(p1, 2.6, buttons=BTN_A)
    rows = ra.snapshot()[1][f_mute:]
    check(rows and not any(h[0] for _, _, h in rows), f"while RetroArch is silent nothing is sent ({len(rows)} frames)")
    ra.mute = False
    f4 = ra.frames
    hold(p1, 1.5, buttons=BTN_A)
    check(ra.held_fraction(0, RP_A, f4 + 45) > 0.8, "after the restart the held A is sent again")
    hold(p1, 0.3)

    # --- GET_STATUS crashes the released RetroArch (1.22.2 and older) when the
    # core is not in its info list: a 1.22.2 must never be asked for its status
    check(ra.probes > 3 * 30 and ra.status_requests == 0,
          f"RetroArch 1.22.2 never received GET_STATUS ({ra.probes} probes answered)")
    # --- a newer RetroArch (fix merged January 2026) is asked, but only after
    # its VERSION reply says it is safe
    ra.mute = True
    time.sleep(1.6)  # past REACH_TTL: the receiver drops the sync and forgets the version
    ra.version = "1.23.0"
    n0 = ra.probes
    ra.mute = False
    eventually(lambda: ra.status_requests >= 1, "RetroArch 1.23.0: GET_STATUS asked (one probe in 30)", timeout=5)
    check(ra.status_after_probes is not None and ra.status_after_probes >= n0 + 1,
          f"and only after its VERSION reply ({ra.status_after_probes - n0} probes answered first)")
    hold(p1, 0.3)

    p1.close()
    ra.stop()
    print(f"\nAll {COUNT} checks passed", flush=True)


if __name__ == "__main__":
    main()
