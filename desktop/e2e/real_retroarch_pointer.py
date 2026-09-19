"""Windows integration with a real RetroArch and the no-ROM diagnostic core.

rustc --edition 2021 --crate-type cdylib fixtures/pointer_core.rs -o pointer_libretro.dll
python real_retroarch_pointer.py <PepoMote.exe> <retroarch.exe> <pointer_libretro.dll>

Creates fresh settings, ports and logs. Never edits the supplied RetroArch.
The test opens its own fullscreen window and moves/clicks inside that window.
"""
import ctypes
from ctypes import wintypes
import json
import math
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import time

ROOT = Path(tempfile.mkdtemp(prefix="pepomote-pointer-real-"))
BIN, RA, CORE = (Path(a).resolve() for a in sys.argv[1:4])
for name in ("config", "retroarch", "appdata", "dolphin", "cemu", "eden"):
    (ROOT / name).mkdir()
# Keep the same local ports for all three participants.
PORT = 29771
os.environ.update(PEPOMOTE_PORT=str(PORT), PEPOMOTE_RETROARCH_PORT=str(PORT + 2),
                  PEPOMOTE_RETROARCH_CMD_PORT=str(PORT + 7), PEPOMOTE_RETROARCH_DIR=str(ROOT / "retroarch"))
from e2e_retroarch import Phone, check, eventually, BTN_A, BTN_B, BTN_ONE, BTN_TWO

env = os.environ.copy()
env.update(APPDATA=str(ROOT / "appdata"), PEPOMOTE_CONFIG_DIR=str(ROOT / "config"),
           PEPOMOTE_DOLPHIN_DIR=str(ROOT / "dolphin"), PEPOMOTE_CEMU_DIR=str(ROOT / "cemu"),
           PEPOMOTE_EDEN_DIR=str(ROOT / "eden"), PEPOMOTE_DSU_PORT=str(PORT - 1),
           PEPOMOTE_PAIR_CODE="1234", PEPOMOTE_NO_TRAY="1", PEPOMOTE_NO_UI="1",
           PEPOMOTE_ASSUME_EMULATOR_CLOSED="1", PEPOMOTE_POINTER_LOG=str(ROOT / "input.csv"))
(ROOT / "config" / "settings.json").write_text(json.dumps({
    "sens_deg": 60, "abs_mode": True, "auto_mode": False, "auto_retroarch": True,
    "auto_dolphin": False, "auto_cemu": False, "auto_eden": False, "update_check": False}), encoding="utf-8")
cfg = ROOT / "retroarch" / "retroarch.cfg"
cfg.write_text('''video_driver = "gl"
input_driver = "dinput"
video_fullscreen = "true"
video_windowed_fullscreen = "true"
video_aspect_ratio_auto = "false"
video_scale_integer = "false"
video_force_aspect = "false"
audio_enable = "false"
pause_nonactive = "false"
menu_pause_libretro = "false"
menu_driver = "rgui"
config_save_on_exit = "false"
''', encoding="utf-8")
udp = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
receiver = retro = p1 = p2 = None

def focus_test_window():
    user = ctypes.windll.user32
    user.GetForegroundWindow.restype = wintypes.HWND
    user.SetForegroundWindow.argtypes = [wintypes.HWND]
    found = []
    callback_type = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    @callback_type
    def visit(hwnd, _):
        pid = wintypes.DWORD()
        user.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if pid.value == retro.pid and user.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True
    user.EnumWindows(visit, 0)
    if not found: return False
    # A no-op input lets this test process activate its own child's window.
    user.mouse_event(1, 0, 0, 0, 0)
    user.SetForegroundWindow(found[0])
    return user.GetForegroundWindow() == found[0]

def rows():
    try:
        lines = (ROOT / "input.csv").read_text().splitlines()
        return [list(map(int, line.split(","))) for line in lines if len(line.split(",")) == 9]
    except (FileNotFoundError, ValueError):
        return []

def sample(phone, buttons=0, yaw=0, rate=0, frame=0, rec=0):
    phone.seq += 1
    b = bytearray(72)
    # A coherent yaw sweep in the remapped sensor frame.
    angle = math.radians(-yaw - frame * 90) / 2
    struct.pack_into("<IBBbbIIQ", b, 0, 0x31504D50, 1, 1 | (frame << 5), 0, 0,
                     phone.session, phone.seq, 1_000_000 + phone.seq * 10_000)
    struct.pack_into("<4f", b, 24, math.cos(angle), 0, 0, math.sin(angle))
    struct.pack_into("<3f", b, 40, 0, 0, math.radians(-rate))
    struct.pack_into("<3f", b, 52, 0, 0, 9.81)
    struct.pack_into("<IBBB", b, 64, buttons, rec, 90, 0)
    udp.sendto(b, ("127.0.0.1", PORT))

def stream(seconds, buttons=0, sweep=0, frame=0, rec=0, second=None):
    start = len(rows())
    steps = max(1, int(seconds * 100))
    for i in range(steps):
        sample(p1, buttons, sweep * (i + 1) / steps, sweep / seconds, frame, rec)
        if second:
            sample(second, BTN_TWO, frame=0)
        time.sleep(.01)
    time.sleep(.06)
    return rows()[start:]

try:
    with (ROOT / "receiver.log").open("wb") as log:
        receiver = subprocess.Popen([str(BIN), "--minimized"], env=env, stdout=log, stderr=log,
                                    creationflags=subprocess.CREATE_NO_WINDOW)
    def ready():
        try:
            with socket.create_connection(("127.0.0.1", PORT), timeout=.1): return True
        except OSError: return False
    eventually(ready, "receiver starts isolated")
    p1 = Phone("Pointer test", pad="nes")
    p1.mode("retroarch")
    p1.pad("nes")
    check(p1.ok.get("frame_rotation") is True, "orientation capability negotiated")
    eventually(lambda: 'menu_mouse_enable = "true"' in cfg.read_text(), "menu mouse configured")
    retro = subprocess.Popen([str(RA), "-c", str(cfg), "-L", str(CORE),
                              "--verbose", "--log-file", str(ROOT / "retroarch.log")],
                             cwd=str(ROOT), env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                             creationflags=subprocess.CREATE_NO_WINDOW)
    eventually(lambda: len(rows()) > 15, "real RetroArch is calling the diagnostic core", timeout=25)
    eventually(focus_test_window, "RetroArch test window is in the foreground")
    stream(.4, rec=1)
    for frame in (0, 1, 3, 0):
        stream(.15, frame=frame, rec=2)
        motion = stream(.6, sweep=12, frame=frame, rec=2)
        xs = [r[5] for r in motion]
        check(max(xs) - min(xs) > 2000, f"frame {frame}: core sees absolute lightgun movement")
        check(sum(abs(r[1]) for r in motion) > 20, f"frame {frame}: core sees relative mouse movement")
        check(all(r[3] == 0 and r[4] == 0 for r in motion), "Wii movement creates no clicks")
    buttons = stream(.5, BTN_A | BTN_B | BTN_ONE | BTN_TWO, rec=3)
    expected = (1 << 9) | (1 << 1) | (1 << 0) | (1 << 8)
    check(sum(r[8] == expected for r in buttons) > 15, "all four Wii buttons arrive as RetroPad X/Y/B/A")
    check(all(r[3] == 0 and r[4] == 0 for r in buttons), "Wii buttons do not duplicate mouse clicks")
    stream(.1)
    p1.pad("gun")
    p2 = Phone("Second player", pad="nes")
    held = stream(.6, BTN_B, second=p2)
    middle = held[8:-5]
    check(middle and all(r[3] != 0 and r[7] != 0 for r in middle), "J2 never releases J1 mouse or lightgun trigger")
    p1.pad("nes")
    released = stream(.4, second=p2)
    check(all(r[3] == 0 and r[4] == 0 for r in released[5:]), "switching Gun to Wii releases the trigger")
    p1.pad("gun")
    right = stream(.35, BTN_A)
    check(any(r[4] != 0 for r in right), "Gun reload reaches the core as right mouse")
    stream(.15)
    p1.pad("retropad")
    still = stream(.25)
    moved = stream(.5, sweep=12)
    check(max(r[5] for r in moved) - min(r[5] for r in moved) < 100, "console RetroPad never moves the cursor")
    print(f"PASS real RetroArch pointer; evidence: {ROOT}", flush=True)
finally:
    if p1:
        try:
            sample(p1)
            time.sleep(.12)
            p1.close()
        except OSError: pass
    if p2: p2.close()
    if retro:
        udp.sendto(b"QUIT\n", ("127.0.0.1", PORT + 7))
        try: retro.wait(timeout=5)
        except subprocess.TimeoutExpired: retro.terminate()
    if receiver:
        receiver.terminate()
        receiver.wait(timeout=5)
    print(f"Logs: {ROOT}", flush=True)
