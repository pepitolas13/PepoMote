<p align="center"><img src="assets/brand/logo.png" width="96" alt="PepoMote"></p>

<h1 align="center">PepoMote</h1>
<p align="center"><b>Point. Click. Play.</b> — Turn your Android phone into a Wii-style motion pointer and controller for your PC.</p>

**[Español](README.es.md)**

- **Pointer mode** — aim your phone at the screen and the cursor goes exactly there (world-anchored absolute pointing, roll-invariant, 250 Hz). Buttons, drag, scroll, media keys. Your real mouse keeps working whenever the phone is still.
- **Dolphin mode** — PepoMote becomes a full-motion virtual Wiimote (DSU/cemuhook server on `127.0.0.1:26760`). Play real Wii games — Wii Sports bowling included — in the [Dolphin emulator](https://dolphin-emu.org/).
- **Wii U mode** — turn the phone sideways and it is a Wii U GamePad for the [Cemu emulator](https://cemu.info/): two sticks, A/B/X/Y, L/R/ZL/ZR, gyro, touch screen. Cemu's controller profiles are written for you; a second phone can be a Pro Controller, and any phone can be a Wii Remote (with Nunchuk) for Wii-style Wii U games. **Second screen included**: the GamePad's own screen (map, inventory, off-TV play) is streamed from Cemu to the phone (and hidden on the PC), touching it touches the GamePad screen, and the phone's keyboard types into Cemu's on-screen keyboard (player names and the like).

| Piece | Platform | File |
|---|---|---|
| Sender | Android 8.0+ | `PepoMote.apk` |
| Sender | iPhone / iPad, iOS 15+ (up to iPadOS 26) | `PepoMote.ipa` — sideloaded with AltStore or Sideloadly, see [docs/IOS.md](docs/IOS.md) |
| Sender | Linux phones: Mobian, postmarketOS… (aarch64) | `pepomote-mobile_*_arm64.deb` (Mobian: tap to install) · `PepoMote-Mobile-aarch64.AppImage` (glibc) · `PepoMote-Mobile-aarch64-musl.tar.gz` (postmarketOS) |
| Receiver | Windows 10/11 | `PepoMote.exe` — single portable file |
| Receiver | Linux, X11 & Wayland | `PepoMote-x86_64.AppImage` |

## Install

**PC (Windows)** — download `PepoMote.exe` and run it. No installer. SmartScreen may warn because the binary is unsigned: *More info → Run anyway* (verify `SHA256SUMS.txt` if in doubt). Allow it on *private networks* when the firewall asks.

**PC (Linux)** — download the AppImage, make it executable and run it. On Sway, Hyprland, MangoWC, river, labwc, niri and other wlroots compositors nothing else is needed: the cursor is a Wayland virtual pointer. On GNOME, KDE or X11 the cursor goes through uinput: if that (or a firewall silently dropping the phone's traffic — many distros ship one enabled) needs setup, PepoMote detects it and asks for your admin password **once** in the system dialog, then fixes it by itself; if your session has no password dialog, the window shows the one-line command to paste in a terminal. Prefer a scripted install with a launcher entry? `packaging/linux/install.sh PepoMote-x86_64.AppImage` does the same setup non-interactively. Something odd? `./PepoMote-x86_64.AppImage --diag` prints a report to paste in an issue.

**Phone** — install `PepoMote.apk` (enable "install from unknown sources"). Open it, tap **Conectar**, scan the QR shown on your PC. Paired forever.

**iPhone / iPad** — not on the App Store: install `PepoMote.ipa` with [AltStore](https://altstore.io) (add the source `https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json` and tap Install; it re-signs itself every 7 days) or with Sideloadly. Step by step, permissions and what differs from Android in [docs/IOS.md](docs/IOS.md).

**Linux phone** (Mobian, postmarketOS…) — Mobian: download `pepomote-mobile_*_arm64.deb` on the phone, tap it and press **Install**. Any other distro, one command in the terminal: `wget -qO- https://raw.githubusercontent.com/pepitolas13/PepoMote/main/packaging/linux-mobile/install.sh | sh`. Then open it like any app: **Conectar** → pick your PC → type the 4-digit code shown under the QR. See [docs/MOBILE-LINUX.md](docs/MOBILE-LINUX.md).

## Play Wii games

See [docs/SETUP-DOLPHIN.md](docs/SETUP-DOLPHIN.md) — two minutes of one-time Dolphin setup (DSU server + bundled controller profile), then: aim at the screen to point, hold the target button to recenter, swing to bowl.

## Play Wii U games

See [docs/SETUP-CEMU.md](docs/SETUP-CEMU.md) — tap **Wii U** in the app, hold the phone sideways, open Cemu. Nothing to configure: PepoMote writes Cemu's controller profile (GamePad from your phone's DSU pad, motion and touch included) while Cemu is closed.

## Nice touches

- **Local multiplayer**: up to 4 phones on one PC — scan the same QR and each phone becomes its own Wiimote in Dolphin, with Dolphin's controller config written for you automatically
- **Nunchuk**: a second phone in your other hand (tap **Nunchuk** in the app): stick, C, Z and its own accelerometer feed the emulated Nunchuk of your Wiimote, configured in Dolphin for you
- Physical volume keys = A / B triggers (zero touch latency)
- Turn the phone sideways for a NES-style pad (2D games) — or a full Wii U GamePad in Wii U mode
- Optional start-with-the-system (tray only, no window)
- One QR pairing; reconnects with one tap; auto-discovery on your LAN
- Synthesized UI sounds + haptics (both optional)
- **Automatic mode** (1.4): open Dolphin or Cemu and the receiver switches mode by itself; close it and the pointer is back
- **Precision**: hold the magnifier strip and the cursor moves at 40 %, with no jump when you let go
- **Several PCs and automatic reconnection**: the app keeps all your PCs and, if the Wi-Fi drops or the receiver restarts, it comes back by itself without losing the screen or the mode
- **Dark theme and English**: both follow the system; ES/EN at the top right of every app
- **A chime per player, a tray icon that tells the state, a heartbeat with the RTT** in the receiver window; on Android a themed icon, launcher shortcuts and a Quick Settings tile

## Build from source

- Receiver: `cd desktop && cargo build --release`
- Android: `cd android && ./gradlew assembleDebug`
- iOS (Mac): `brew install xcodegen && cd ios && xcodegen generate && xcodebuild -scheme PepoMote build` (the CI builds the unsigned IPA on macOS runners)
- Protocol spec: [protocol/PROTOCOL.md](protocol/PROTOCOL.md) · DSU notes: [protocol/DSU.md](protocol/DSU.md)

## Legal

PepoMote is an original, independent project — not affiliated with, endorsed by, or sponsored by Nintendo. It contains no Nintendo assets, trademarks, fonts or sounds; "Wii" is used only nominatively to describe compatibility with the Dolphin emulator. "Wii U" likewise describes compatibility with the Cemu emulator (open source, MPL-2.0). It does not distribute Dolphin, Cemu or any games. See [docs/LEGAL.md](docs/LEGAL.md).

## License

GPL-3.0-or-later. Original visual and sound assets: CC-BY-SA 4.0. Font: Nunito (SIL OFL 1.1).

---

Made by [PepoTech](https://www.youtube.com/@PepoTech).
