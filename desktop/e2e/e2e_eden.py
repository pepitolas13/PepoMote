"""Real TCP/UDP Switch integration against an isolated running receiver.

Set PEPOMOTE_EDEN_DIR, PEPOMOTE_PORT, PEPOMOTE_DSU_PORT and pairing code 1234.
No emulator or phone is required. Does not alter a real emulator install.
"""
import json
import math
import os
from pathlib import Path
import queue
import socket
import struct
import threading
import time
import zlib

HOST = "127.0.0.1"
PORT = int(os.environ.get("PEPOMOTE_PORT", "26771"))
DSU = int(os.environ.get("PEPOMOTE_DSU_PORT", "26770"))
CONFIG = Path(os.environ["PEPOMOTE_EDEN_DIR"]) / "qt-config.ini"
GUID = "0000000000000000000000007f000001"
COUNT = 0


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
        time.sleep(0.025)
    raise AssertionError(label)


class Phone:
    def __init__(self, name, **extra):
        self.s = socket.create_connection((HOST, PORT), timeout=5)
        self.s.settimeout(None)
        self.q = queue.Queue()
        self.alive = True
        self.seq = 0
        self.write_lock = threading.Lock()
        self.send(m="hello", pv=1, name=name, model="Switch E2E", code="1234", **extra)
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
                m = self.q.get(timeout=max(0.001, until-time.monotonic()))
            except queue.Empty:
                break
            if pred(m):
                return m
        raise AssertionError("Missing receiver echo")

    def pad(self, kind, effective=None):
        self.send(m="pad", pad=kind)
        return self.receive(lambda m: m.get("m") == "pad" and m.get("pad") == (effective or kind))

    def close(self):
        self.alive = False
        try:
            self.send(m="bye")
            self.s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.s.close()


def config():
    try:
        return CONFIG.read_text(encoding="utf-8")
    except FileNotFoundError:
        return ""


def value(key):
    for line in config().splitlines():
        if line.startswith(key + "="):
            return line.split("=", 1)[1].strip('"')
    return None


UDP = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
CLIENT = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
CLIENT.settimeout(0.04)


def sample(phone, buttons=0, stick=(0, 0), right=(0, 0), gyro=(0, 0, 0), touch=True, sensorless=False):
    phone.seq += 1
    b = bytearray(80)
    struct.pack_into("<IBBbbIIQ", b, 0, 0x31504D50, 1, (0 if sensorless else 1) | 6 | (8 if touch else 0),
                     *stick, phone.session, phone.seq, 1_000_000 + phone.seq*4000)
    struct.pack_into("<4f", b, 24, 1, 0, 0, 0)
    struct.pack_into("<3f", b, 40, *gyro)
    struct.pack_into("<3f", b, 52, 0, 0, 0 if sensorless else 9.80665)
    struct.pack_into("<I", b, 64, buttons)
    b[69] = 90
    struct.pack_into("<bbHH", b, 72, *right, 32768, 32768)
    UDP.sendto(b, (HOST, PORT))


def pad_data(phone, expected, **kwargs):
    slot = phone.ok["slot"]
    payload = struct.pack("<IBB6s", 0x100002, 1, slot, bytes(6))
    request = bytearray(b"DSUC" + struct.pack("<HHII", 1001, len(payload), 0, 0xE2E) + payload)
    struct.pack_into("<I", request, 8, zlib.crc32(request))
    CLIENT.sendto(request, (HOST, DSU))
    until = time.monotonic() + 5
    while time.monotonic() < until:
        sample(phone, **kwargs)
        try:
            data, _ = CLIENT.recvfrom(256)
        except socket.timeout:
            continue
        if len(data) == 100 and data[:4] == b"DSUS" and data[20] == slot and expected(data):
            crc = struct.unpack_from("<I", data, 8)[0]
            check(crc == zlib.crc32(data[:8] + bytes(4) + data[12:]), "DSU checksum")
            return data
    raise AssertionError("Expected DSU sample did not arrive")


def main():
    phones = []
    original = CONFIG.read_bytes()
    try:
        p = Phone("Switch test one")
        phones.append(p)
        check("switch" in p.ok["modes"], "Switch capability")
        p.send(m="mode", mode="switch")
        p.receive(lambda m: m.get("m") == "mode" and m.get("mode") == "switch")
        echo = p.receive(lambda m: m.get("m") == "pad" and m.get("pad") == "pro")
        check(echo.get("half") is None and echo.get("side") is None, "Pro metadata")
        eventually(lambda: value("player_0_type") == "0" and value("player_0_type\\default") == "false"
                   and "engine:cemuhookudp" in (value("player_0_button_a") or ""), "Pro Controller configured")
        check(value("player_0_button_a") == f"button:8192,engine:cemuhookudp,guid:{GUID},pad:4,port:{DSU}", "Exact A binding")
        check(value("player_0_button_a\\default") == "false", "Qt default overridden")
        check(value("enable_udp_controller") == "true", "UDP controller enabled")
        check("10.0.0.5:25000" in value("udp_input_servers"), "Existing UDP server retained")
        check(CONFIG.with_suffix(".ini.pepomote.bak").read_bytes() == original, "Original backup byte for byte")
        written = CONFIG.read_bytes()
        check(written.startswith(b"[Audio]\r\nvolume=73\r\n\r\n"), "Audio and CRLF retained")
        check(written.endswith(b"[Renderer]\r\nbackend=1\r\n"), "Renderer retained")
        digital_bits = [0, 1, 2, 3, 4, 5, 6, 7, 19, 20, 21, 22, 23, 24, 25, 26]
        all_buttons = sum(1 << n for n in digital_bits) | (1 << 8) | (1 << 28)
        d = pad_data(p, lambda d: d[36:40] == bytes([255]*4), buttons=all_buttons)
        check(d[52:56] == bytes([255]*4), "Trigger and shoulder analog pressure")
        check(d[56] == 0 and d[62] == 0, "Switch never exposes GamePad touch")
        for bit, mask in [(0, 8192), (1, 16384), (19, 4096), (20, 32768)]:
            pad_data(p, lambda d, mask=mask: struct.unpack_from("<H", d, 36)[0] == mask, buttons=1 << bit)
            check(True, f"Face button bit {bit} matches Eden")
        pad_data(p, lambda d: d[36:40] == bytes(4), buttons=(1 << 17)|(1 << 18)|(1 << 27)|(1 << 9)|(1 << 10))
        check(True, "Wii-only and microphone buttons ignored")
        pad_data(p, lambda d: list(d[40:44]) == [127]*4)
        check(True, "Exact neutral stick center")
        pad_data(p, lambda d: list(d[40:44]) == [254, 0, 27, 227], stick=(127, -127), right=(-100, 100))
        check(True, "Both sticks retain sign and range")
        d = pad_data(p, lambda d: abs(struct.unpack_from("<f", d, 88)[0] - 312/(2*math.pi)) < 0.02, gyro=(1, 0, 0))
        check(abs(struct.unpack_from("<f", d, 80)[0] + 1) < 0.001, "Gravity is -1 g in DS4 frame")
        check(True, "Gyro 1 rad/s is 49.656 degrees in Eden DSU scale")
        q = Phone("Switch test two", token=p.ok.get("token"))
        phones.append(q)
        eventually(lambda: value("player_1_type") == "0", "Second Pro configured")
        for old in ["joycons", "joycon_side", "joycon_r"]:
            for phone, player in [(p, 1), (q, 2)]:
                echo = phone.pad(old, "pro")
                check(echo.get("pad") == "pro" and echo.get("player") == player
                      and echo.get("half") is None and echo.get("side") is None,
                      f"Legacy {old} becomes independent Pro player {player}")
        check(q.pad("wiimote", "pro")["pad"] == "pro", "Cemu pad rejected in Switch")
        check(value("player_0_type") == "0" and value("player_1_type") == "0", "Only Pro types configured")
        for player, slot in [(0, 4), (1, 5)]:
            for key in ["lstick", "rstick", "motionleft", "motionright"]:
                check(f"pad:{slot}," in value(f"player_{player}_{key}"), f"Player {player+1} owns {key}")
        # Stored choices from a previous build also migrate during hello.
        legacy = Phone("Legacy stored choice", token=p.ok.get("token"), pad="joycon_r")
        phones.append(legacy)
        check(legacy.ok.get("pad") == "pro" and legacy.ok.get("player") == 3
              and legacy.ok.get("half") is None and legacy.ok.get("side") is None, "Legacy hello is a third Pro")
        eventually(lambda: value("player_2_type") == "0", "Legacy hello configured as Pro")
        legacy.close()
        phones.remove(legacy)
        eventually(lambda: value("player_2_connected") == "false", "Departed legacy player disabled")
        q.close()
        phones.remove(q)
        eventually(lambda: value("player_1_connected") == "false", "Departed second player disabled")
        check(value("player_0_connected") == "true", "Remaining player connected")
        p.send(m="mode", mode="cemu")
        p.receive(lambda m: m.get("m") == "mode" and m.get("mode") == "cemu")
        p.receive(lambda m: m.get("m") == "pad" and m.get("pad") == "gamepad")
        pad_data(p, lambda d: d[37] == 64 and list(d[40:44]) == [128]*4, buttons=1)
        check(True, "Cemu retains A-to-Cross and center 128")
        # No quaternion, gyro or accelerometer is needed to keep buttons and
        # sticks live. Check both press and release through the actual receiver.
        for mode, face_mask in [("cemu", 64), ("dolphin", 64), ("switch", 32)]:
            p.send(m="mode", mode=mode)
            p.receive(lambda m: m.get("m") == "mode" and m.get("mode") == mode)
            d = pad_data(p, lambda d: d[37] == face_mask and d[40] == 228 if mode != "switch"
                         else d[37] == face_mask and d[40] == 227,
                         buttons=1, stick=(100, 0), sensorless=True, touch=False)
            check(struct.unpack_from("<3f", d, 88) == (0, 0, 0), f"{mode}: sensorless press and stick, zero gyro")
            center = 127 if mode == "switch" else 128
            pad_data(p, lambda d: d[36:39] == bytes(3) and d[40] == center,
                     sensorless=True, touch=False)
            check(True, f"{mode}: sensorless button release and centered stick")
        print(f"PASS: {COUNT} Switch integration checks", flush=True)
    finally:
        for phone in phones:
            phone.close()
        UDP.close()
        CLIENT.close()


if __name__ == "__main__":
    main()
