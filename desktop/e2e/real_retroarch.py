"""Manual end-to-end check against a REAL RetroArch (not run in CI).

Drives an isolated PepoMote receiver with a simulated phone and a real
RetroArch you point it at, and looks at what RetroArch does: retroarch.cfg
written and backed up, probes answered, no GET_STATUS sent to a 1.22.2 (it
would crash it), the RetroPad reaching the core (screenshots of the board),
Home opening the menu, SAVE_STATE / SCREENSHOT / fast-forward hotkeys, and
the automatic mode switch when RetroArch is opened.

    python real_retroarch.py <PepoMote.exe> <RetroArch folder> --wipe \
        [--core <2048 core>] [--game <rom> --game-core <core> --console <id>]

Use a scratch copy of RetroArch (the official zip extracted anywhere): with
--wipe the script deletes that folder's retroarch.cfg, its backup, its history
playlist, and every file in saves/, states/ and screenshots/, and it writes
pause_nonactive=false so a window that does not get the focus keeps running.
--core runs the pad checks with the free 2048 core (title screen → Start →
board moves with the d-pad); that core must live OUTSIDE the RetroArch folder
(that is what makes GET_STATUS deadly on 1.22.2: a core that is not in
RetroArch's info list). --game loads a real game on its core and checks the
receiver announces it from RetroArch's history with the expected console id
(e.g. Cave Story on genesis_plus_gx → md). Needs Pillow for --core.
"""
import io
import json
import os
from pathlib import Path
import queue
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time

try:
    from PIL import Image
except ImportError:  # only --core needs it
    Image = None

HOST = "127.0.0.1"
PORT = int(os.environ.get("PEPOMOTE_PORT", "26771"))
DSU_PORT = str(PORT - 1)
CMD_PORT = 55355
COUNT = 0

# PROTOCOL.md §4.2
BTN_A, BTN_B, BTN_UP, BTN_DOWN, BTN_LEFT, BTN_RIGHT = 1, 2, 4, 8, 16, 32
BTN_PLUS, BTN_MINUS, BTN_HOME, BTN_SCREEN = 64, 128, 256, 1 << 28


def usage():
    print(__doc__)
    sys.exit(2)


def opt(name):
    return Path(sys.argv[sys.argv.index(name) + 1]).resolve() if name in sys.argv else None


if len(sys.argv) < 3:
    usage()
BIN = Path(sys.argv[1]).resolve()
RA = Path(sys.argv[2]).resolve()
CORE = opt("--core")
GAME = opt("--game")
GAME_CORE = opt("--game-core")
CONSOLE = sys.argv[sys.argv.index("--console") + 1] if "--console" in sys.argv else None
WIPE = "--wipe" in sys.argv[3:]
EXE = next((RA / n for n in ("retroarch.exe", "retroarch") if (RA / n).exists()), None)
if not BIN.exists() or EXE is None or (CORE is None and GAME is None):
    usage()
if CORE is not None and (not CORE.exists() or Image is None):
    sys.exit("--core needs an existing core and Pillow")
if GAME is not None and (GAME_CORE is None or not GAME.exists() or not GAME_CORE.exists() or CONSOLE is None):
    sys.exit("--game needs --game-core <core> and --console <id>")
if not WIPE:
    sys.exit("refusing to touch that RetroArch folder without --wipe (see the docstring)")
if CORE is not None and RA in CORE.parents:
    sys.exit("put the 2048 core outside the RetroArch folder (it must not be in RetroArch's info list)")
RCV = Path(tempfile.mkdtemp(prefix="pepomote-real-retroarch-"))


def check(cond, label):
    global COUNT
    if not cond:
        raise AssertionError(label)
    COUNT += 1
    print("OK  " + label, flush=True)


def eventually(pred, label, timeout=10):
    until = time.monotonic() + timeout
    while time.monotonic() < until:
        if pred():
            check(True, label)
            return
        time.sleep(0.05)
    raise AssertionError(label)


class Phone:
    def __init__(self, name, **extra):
        self.s = socket.create_connection((HOST, PORT), timeout=5)
        self.s.settimeout(None)
        self.q = queue.Queue()
        self.alive = True
        self.seq = 0
        self.lock = threading.Lock()
        self.send(m="hello", pv=1, name=name, model="Real E2E", code="1234", **extra)
        threading.Thread(target=self.reader, daemon=True).start()
        self.ok = self.receive(lambda m: m.get("m") in ("ok", "err"))
        check(self.ok.get("m") == "ok", f"{name}: authenticated")
        self.session = self.ok["session_id"]
        threading.Thread(target=self.beat, daemon=True).start()
        self.udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

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
        with self.lock:
            self.s.sendall((json.dumps(message) + "\n").encode())

    def receive(self, pred, timeout=10):
        until = time.monotonic() + timeout
        while time.monotonic() < until:
            try:
                m = self.q.get(timeout=max(0.001, until - time.monotonic()))
            except queue.Empty:
                break
            if pred(m):
                return m
        raise AssertionError("Missing receiver message")

    def sample(self, buttons=0, stick=(0, 0), right=(0, 0)):
        self.seq += 1
        b = bytearray(80)
        struct.pack_into("<IBBbbIIQ", b, 0, 0x31504D50, 1, 1 | 2 | 4, *stick, self.session, self.seq, 1_000_000 + self.seq * 4000)
        struct.pack_into("<4f", b, 24, 1, 0, 0, 0)
        struct.pack_into("<3f", b, 52, 0, 0, 9.80665)
        struct.pack_into("<I", b, 64, buttons)
        b[69] = 90
        struct.pack_into("<bbHH", b, 72, *right, 0, 0)
        self.udp.sendto(b, (HOST, PORT))

    def hold(self, seconds, buttons=0, stick=(0, 0), right=(0, 0), hz=100):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            self.sample(buttons, stick, right)
            time.sleep(1 / hz)

    def tap(self, buttons, seconds=0.15):
        self.hold(seconds, buttons=buttons)
        self.hold(0.15)

    def hotkey(self, name, down=True):
        self.send(m="hotkey", name=name, down=down)
        return self.receive(lambda m: m.get("m") == "hotkey" and m.get("name") == name)

    def close(self):
        self.alive = False
        try:
            self.send(m="bye")
        except OSError:
            pass
        self.s.close()


def cmd(text, wait=1.5):
    """One command to RetroArch's command interface; None when nobody answers."""
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.settimeout(wait)
    s.sendto(text.encode(), (HOST, CMD_PORT))
    try:
        return s.recv(4096).decode("utf-8", "replace")
    except (socket.timeout, OSError):
        # Windows: an ICMP port-unreachable (RetroArch not up yet) surfaces as ConnectionResetError
        return None
    finally:
        s.close()


def cfg_value(key):
    p = RA / "retroarch.cfg"
    if not p.exists():
        return None
    for line in p.read_text(encoding="utf-8", errors="replace").splitlines():
        if line.startswith(key + " ="):
            return line.split("=", 1)[1].strip().strip('"')
    return None


def receiver_log():
    p = RCV / "config" / "receptor.log"
    return p.read_text(encoding="utf-8", errors="replace") if p.exists() else ""


def shot(p1):
    """Screenshot through the phone hotkey; RetroArch names them per second, so space them out."""
    before = {f.name for f in (RA / "screenshots").glob("*.png")}
    time.sleep(1.1)
    p1.hotkey("screenshot")
    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        new = [f for f in (RA / "screenshots").glob("*.png") if f.name not in before]
        if new:
            time.sleep(0.4)
            # RetroArch's GPU screenshot includes its notifications (top strip,
            # "screenshot saved <name>" differs every time): compare the board only
            im = Image.open(io.BytesIO(new[-1].read_bytes())).convert("RGB")
            return im.crop((0, 130, im.width, im.height)).tobytes()
        time.sleep(0.05)
    raise AssertionError("screenshot not written")


def start_retroarch(tag, core=None, content=None):
    log = RA / f"pepomote-real-{tag}.log"
    if log.exists():
        log.unlink()
    args = [str(EXE), "-L", str(core or CORE)]
    if content is not None:
        args.append(str(content))
    p = subprocess.Popen(args + ["--verbose", "--log-file", str(log)],
                         cwd=str(RA), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return p, log


def quit_retroarch(ra):
    cmd("QUIT")
    try:
        ra.wait(timeout=15)
    except subprocess.TimeoutExpired:
        ra.kill()


def main():
    # --- isolated receiver with REAL process detection (no PEPOMOTE_ASSUME_EMULATOR_CLOSED)
    env = os.environ.copy()
    for key, path in {"APPDATA": RCV / "appdata", "PEPOMOTE_CONFIG_DIR": RCV / "config", "PEPOMOTE_DOLPHIN_DIR": RCV / "dolphin",
                      "PEPOMOTE_CEMU_DIR": RCV / "appdata" / "Cemu", "PEPOMOTE_EDEN_DIR": RCV / "eden"}.items():
        path.mkdir(parents=True, exist_ok=True)
        env[key] = str(path)
    env.update(PEPOMOTE_RETROARCH_DIR=str(RA), PEPOMOTE_PORT=str(PORT), PEPOMOTE_DSU_PORT=DSU_PORT, PEPOMOTE_PAIR_CODE="1234",
               PEPOMOTE_NO_TRAY="1", PEPOMOTE_NO_UI="1", PYTHONIOENCODING="utf-8")
    env.pop("PEPOMOTE_ASSUME_EMULATOR_CLOSED", None)
    (RCV / "config" / "settings.json").write_text(json.dumps({
        "sens_deg": 40, "abs_mode": True, "auto_mode": True, "auto_dolphin": False, "auto_cemu": False,
        "auto_eden": False, "auto_retroarch": True, "update_check": False, "lang": "es"}), encoding="utf-8")
    # No core-info cache: with the core loaded from outside the folder, RetroArch 1.22.2
    # dies on GET_STATUS (0xC0000005), so staying alive proves the receiver never sends it
    for stale in [RA / "retroarch.cfg", RA / "retroarch.cfg.pepomote.bak", RA / "info" / "core_info.cache",
                  RA / "playlists" / "builtin" / "content_history.lpl", RA / "content_history.lpl"]:
        if stale.exists():
            stale.unlink()
    # the 2048 core keeps its game in SRAM (saves/2048/2048.srm): start on the title screen
    for d in ["screenshots", "states", "saves"]:
        for f in (RA / d).rglob("*"):
            if f.is_file():
                f.unlink()
    # RetroArch pauses the core while its window is not in front (pause_nonactive), and a
    # window launched from a script may not get the focus: keep it running
    (RA / "retroarch.cfg").write_text('pause_nonactive = "false"\n', encoding="utf-8")
    rcv = subprocess.Popen([str(BIN), "--minimized"], env=env, stdout=open(RCV / "receiver.out", "wb"), stderr=subprocess.STDOUT,
                           creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
    ra = None
    try:
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            try:
                with socket.create_connection((HOST, PORT), timeout=0.1):
                    break
            except OSError:
                time.sleep(0.05)
        else:
            raise TimeoutError("receiver not listening")

        # --- phone in RetroArch mode with RetroArch closed: retroarch.cfg gets our keys
        p1 = Phone("Movil real")
        check("retroarch" in p1.ok["modes"], "ok.modes announces retroarch")
        first = p1.q.get(timeout=3)
        check(first["m"] == "game" and first["console"] is None, "game (nothing in the history yet) is the first line after ok")
        p1.send(m="mode", mode="retroarch")
        p1.receive(lambda m: m.get("m") == "mode" and m.get("mode") == "retroarch")
        notice = p1.receive(lambda m: m.get("m") == "notice", timeout=10)
        print("notice:", notice["text"])
        check("listo" in notice["text"].lower(), "receiver says RetroArch is ready")
        check(cfg_value("network_remote_enable") == "true", "retroarch.cfg next to the executable got the network gamepad keys")
        check(cfg_value("pause_nonactive") == "false", "existing keys kept")
        check((RA / "retroarch.cfg.pepomote.bak").read_text(encoding="utf-8") == 'pause_nonactive = "false"\n', "backup is the file as it was")
        check(cfg_value("network_cmd_enable") == "true" and cfg_value("network_cmd_port") == "55355", "command interface enabled")
        check(cfg_value("network_remote_base_port") == "55400", "base port 55400")

        if CORE is not None:
            # --- the real RetroArch starts with the core
            ra, log = start_retroarch("1")
            eventually(lambda: cmd("VERSION") is not None, "RetroArch answers VERSION on 55355", timeout=25)
            version = cmd("VERSION").strip()
            print("RetroArch version:", version)
            check(version.startswith("1."), f"version {version}")
            eventually(lambda: "Bringing up remote interface on port 55400" in log.read_text(errors="replace"), "RetroArch log: remote interface on 55400", timeout=10)
            eventually(lambda: "RetroArch responde" in receiver_log(), "receiver synced with RetroArch (probes answered)", timeout=10)
            check("Loading dynamic libretro core" in log.read_text(errors="replace"), "core loaded")
            time.sleep(5)
            check(ra.poll() is None, "RetroArch still alive 5 s after the sync (no GET_STATUS sent to a 1.22.2)")
            check("ya no responde" not in receiver_log(), "receiver kept the sync the whole time")

            # --- screenshot hotkey from the phone
            title = shot(p1)
            check(len(title) > 100, "SCREENSHOT via phone hotkey wrote a PNG")
            # the 2048 core waits on its title screen: Start (+ on the phone) begins the game
            p1.tap(BTN_PLUS, 0.3)
            time.sleep(0.5)
            shot0 = shot(p1)
            check(title != shot0, "Start from the phone (RetroPad Start) left the title screen")

            # --- d-pad from the phone moves the board: the next screenshot differs
            for i in range(3):
                p1.tap(BTN_LEFT)
                p1.tap(BTN_UP)
            time.sleep(0.5)
            shot1 = shot(p1)
            check(shot0 != shot1, "board changed after Left/Up taps sent through PepoMote (RetroPad d-pad reaches the core)")
            # and with no input, two screenshots are identical (so the diff above is the input)
            check(shot(p1) == shot(p1), "without input the board stays put (control)")

            # --- menu toggle via Home: inside the menu the d-pad moves the cursor, not the game
            before_menu = shot(p1)
            p1.tap(BTN_HOME, 0.3)
            time.sleep(0.5)
            for i in range(3):
                p1.tap(BTN_DOWN)
                p1.tap(BTN_UP)
            p1.tap(BTN_HOME, 0.3)
            time.sleep(0.5)
            after_menu = shot(p1)
            check(before_menu == after_menu, "Home opened the RetroArch menu: d-pad taps inside it did not reach the game")
            check(ra.poll() is None, "RetroArch alive after the menu round trip")

            # --- save state hotkey (states go to states/<core>/<core>.state by default)
            p1.hotkey("save_state")
            eventually(lambda: any(f.suffix == ".state" for f in (RA / "states").rglob("*")), "SAVE_STATE via phone hotkey wrote a state file", timeout=8)

            # --- fast-forward held for a second does not upset anything
            p1.hold(1.0, buttons=BTN_SCREEN)
            p1.hold(0.3)
            check(ra.poll() is None, "RetroArch alive after the fast-forward hold")
            check("pánico" not in receiver_log().lower() and "panick" not in receiver_log().lower(), "receiver log without panics")

            # --- automatic mode: close RetroArch, go to pointer, reopen → the PC switches to RetroArch by itself
            cmd("QUIT")
            try:
                ra.wait(timeout=15)
            except subprocess.TimeoutExpired:
                ra.kill()
            ra = None
            eventually(lambda: "ya no responde" in receiver_log(), "receiver notices RetroArch is gone", timeout=10)
            p1.send(m="mode", mode="pointer")
            p1.receive(lambda m: m.get("m") == "mode" and m.get("mode") == "pointer")
            time.sleep(5)  # let the watcher see 'closed' as stable
            ra, log = start_retroarch("2")
            auto = p1.receive(lambda m: m.get("m") == "mode" and m.get("mode") == "retroarch" and m.get("by") == "pc", timeout=25)
            check(auto is not None, "opening RetroArch switched the receiver to RetroArch mode automatically")
            eventually(lambda: receiver_log().count("RetroArch responde") >= 2, "resynced with the reopened RetroArch", timeout=15)
            # with RetroArch open and the cfg already right, nothing is pending
            p1.send(m="mode", mode="retroarch")
            p1.receive(lambda m: m.get("m") == "mode" and m.get("mode") == "retroarch")
            n2 = p1.receive(lambda m: m.get("m") == "notice", timeout=10)
            print("notice:", n2["text"])
            check("listo" in n2["text"].lower(), "already configured: no need to close RetroArch")
            # input still works after the restart: board changes
            before = shot(p1)
            for i in range(3):
                p1.tap(BTN_RIGHT)
                p1.tap(BTN_DOWN)
            time.sleep(0.5)
            after = shot(p1)
            check(before != after, "d-pad still reaches the core after the restart")
        if GAME is not None:
            # --- a real game: RetroArch writes its history at load and the receiver announces it
            if ra is not None:
                quit_retroarch(ra)
                ra = None
                time.sleep(3)
            ra, log = start_retroarch("game", core=GAME_CORE, content=GAME)
            g = p1.receive(lambda m: m.get("m") == "game" and m.get("path", "").lower().endswith(GAME.name.lower()), timeout=30)
            print("game:", g)
            check(g["console"] == CONSOLE, f"{GAME.name} announced from RetroArch's history as {CONSOLE} ({g['system']}, {g['core']})")
            eventually(lambda: "RetroArch responde" in receiver_log(), "receiver synced with the game running", timeout=15)
            check(ra.poll() is None, "RetroArch alive with the game loaded")
        p1.close()
    finally:
        try:
            cmd("QUIT")
        except Exception:
            pass
        if ra is not None:
            try:
                ra.wait(timeout=10)
            except subprocess.TimeoutExpired:
                ra.kill()
        rcv.terminate()
        try:
            rcv.wait(timeout=8)
        except subprocess.TimeoutExpired:
            rcv.kill()
        shutil.rmtree(RCV, ignore_errors=True)
    print(f"\nAll {COUNT} real-RetroArch checks passed", flush=True)


if __name__ == "__main__":
    main()
