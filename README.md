<p align="center"><img src="assets/brand/logo.png" width="96" alt="PepoMote"></p>

<h1 align="center">PepoMote</h1>
<p align="center"><b>Point. Click. Play.</b> — Turn your Android phone into a Wii-style motion pointer and controller for your PC.</p>

**[Español](README.es.md)**

- **Pointer mode** — aim your phone at the screen and the cursor goes exactly there (world-anchored absolute pointing, roll-invariant, 250 Hz). Buttons, drag, scroll, media keys. Your real mouse keeps working whenever the phone is still.
- **Dolphin mode** — PepoMote becomes a full-motion virtual Wiimote (DSU/cemuhook server on `127.0.0.1:26760`). Play real Wii games — Wii Sports bowling included — in the [Dolphin emulator](https://dolphin-emu.org/).

| Piece | Platform | File |
|---|---|---|
| Sender | Android 8.0+ | `PepoMote.apk` |
| Receiver | Windows 10/11 | `PepoMote.exe` — single portable file |
| Receiver | Linux, X11 & Wayland | `PepoMote-x86_64.AppImage` |

## Install

**PC (Windows)** — download `PepoMote.exe` and run it. No installer. SmartScreen may warn because the binary is unsigned: *More info → Run anyway* (verify `SHA256SUMS.txt` if in doubt). Allow it on *private networks* when the firewall asks.

**PC (Linux)** — download the AppImage and, from the repo, run `packaging/linux/install.sh PepoMote-x86_64.AppImage` (installs the uinput udev rule — needed to move the cursor — and a launcher entry). Log out and back in once.

**Phone** — install `PepoMote.apk` (enable "install from unknown sources"). Open it, tap **Conectar**, scan the QR shown on your PC. Paired forever.

## Play Wii games

See [docs/SETUP-DOLPHIN.md](docs/SETUP-DOLPHIN.md) — two minutes of one-time Dolphin setup (DSU server + bundled controller profile), then: aim at the screen to point, hold the target button to recenter, swing to bowl.

## Nice touches

- Physical volume keys = A / B triggers (zero touch latency)
- Turn the phone sideways for a NES-style pad (2D games)
- Optional start-with-the-system (tray only, no window)
- One QR pairing; reconnects with one tap; auto-discovery on your LAN
- Synthesized UI sounds + haptics (both optional)

## Build from source

- Receiver: `cd desktop && cargo build --release`
- Android: `cd android && ./gradlew assembleDebug`
- Protocol spec: [protocol/PROTOCOL.md](protocol/PROTOCOL.md) · DSU notes: [protocol/DSU.md](protocol/DSU.md)

## Legal

PepoMote is an original, independent project — not affiliated with, endorsed by, or sponsored by Nintendo. It contains no Nintendo assets, trademarks, fonts or sounds; "Wii" is used only nominatively to describe compatibility with the Dolphin emulator. It does not distribute Dolphin or any games. See [docs/LEGAL.md](docs/LEGAL.md).

## License

GPL-3.0-or-later. Original visual and sound assets: CC-BY-SA 4.0. Font: Nunito (SIL OFL 1.1).

---

Made by [PepoTech](https://www.youtube.com/@PepoTech).
