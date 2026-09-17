<p align="center"><img src="assets/brand/logo.png" width="96" alt="PepoMote"></p>

<h1 align="center">PepoMote</h1>
<p align="center"><b>Point. Click. Play.</b> — Turn your phone into a motion pointer and controller for your PC, or connect two Android devices to play.</p>

**[Español](README.es.md)**

**PepoMote 1.10: RetroArch is here.** Your phone takes the shape of the controller for your game: NES, SNES, Mega Drive, N64, PlayStation and more, with their own buttons and labels. Play on a PC or use **another Android device as the server**, with up to four players and shortcuts to save a state, rewind or open the menu. This is a big update, and the beginning of what I want to build with PepoMote. [What's new in 1.10](docs/releases/v1.10.0.md) · [Published downloads](https://github.com/pepitolas13/PepoMote/releases).

**Enjoying the project? Give it a star at the top of GitHub ⭐.** It helps more people discover PepoMote and encourages me to keep improving it. I want it to become the best virtual controller for playing your favourite consoles, and every star helps the project reach more people.

A control not working as expected? Tell me in [my Discord](https://discord.gg/Vx3MPuMPxb), including your device and game, and I'll fix it as soon as I can. I haven't been able to try every control and combination yet.

- **Pointer mode** — aim your phone at the screen and the cursor goes exactly there (world-anchored absolute pointing, roll-invariant, 250 Hz). Buttons, drag, scroll, media keys. Your real mouse keeps working whenever the phone is still. Needs a gyroscope: a phone without a real one (many budget phones) is told so and points by tilting instead, with the sensor selectable in Settings.
- **Dolphin mode** — PepoMote becomes a full-motion virtual Wiimote (DSU/cemuhook server on `127.0.0.1:26760`). Play real Wii games — Wii Sports bowling included — in the [Dolphin emulator](https://dolphin-emu.org/).
- **Wii U mode** — turn the phone sideways and it is a Wii U GamePad for the [Cemu emulator](https://cemu.info/): two sticks, A/B/X/Y, L/R/ZL/ZR, gyro, touch screen. Cemu's controller profiles are written for you; a second phone can be a Pro Controller, and any phone (or all of them at once) can be a Wii Remote (with Nunchuk) for Wii-style Wii U games. **Second screen included**: the GamePad's own screen (map, inventory, off-TV play) is streamed from Cemu to the phone (and hidden on the PC), touching it touches the GamePad screen, and the phone's keyboard types into Cemu's on-screen keyboard (player names and the like). Or, with a real controller on the PC, the phone can be the GamePad's touch screen only, full screen (**GamePad screen full screen** in Settings).

- **Switch mode** — Pro Controller for Eden, with one independent player per phone. Buttons, both sticks, motion when a gyroscope is available, Capture and keyboard; automatic setup with backup and controller restoration. [Switch setup guide](docs/SETUP-SWITCH.md).
- **RetroArch mode** — the phone is a controller for [RetroArch](https://www.retroarch.com/): **the pad of the console RetroArch just loaded** (NES, SNES, Mega Drive, N64, PlayStation… with their own buttons and labels, or the full two-stick **RetroPad**), a **sideways Wii Remote** or, with a PC receiver, a **light gun** (aim with the phone: B fires, A reloads). It uses RetroArch's own network gamepad and network commands, without a driver. On Windows, Linux and macOS, PepoMote configures `retroarch.cfg` while RetroArch is closed; the **Android server** guides you through enabling those options in RetroArch. Home opens the menu, a button fast-forwards while held, and the header brings save/load state, slot, rewind, pause, screenshot and reset. One player per phone, up to four. The Android receiver supports console pads and the sideways controller; light gun and keyboard need a PC receiver. [RetroArch setup guide](docs/SETUP-RETROARCH.md).

| Piece | Platform | File |
|---|---|---|
| Sender and Dolphin/Eden/RetroArch server | Android 8.0+ (the server must also meet the emulator requirements) | `PepoMote.apk` |
| Sender | iPhone / iPad, iOS 15+ (up to iPadOS 26) | `PepoMote.ipa` — installed with SideStore (my recommendation), AltStore or Sideloadly, see [docs/IOS.md](docs/IOS.md) |
| Sender | Linux phones: Mobian, postmarketOS… (aarch64) | `pepomote-mobile_*_arm64.deb` (Mobian: tap to install) · `PepoMote-Mobile-aarch64.AppImage` (glibc) · `PepoMote-Mobile-aarch64-musl.tar.gz` (postmarketOS) |
| Receiver | Windows 10/11 | `PepoMote.exe` — single portable file |
| Receiver | Linux, X11 & Wayland | `PepoMote-x86_64.AppImage` · `PepoMote-linux-x86_64.tar.gz` (binary + launcher + `install.sh`) |
| Receiver | macOS 13+ on Apple Silicon (M1 or later) — **beta** | `PepoMote-macOS.dmg`, see [docs/MACOS.md](docs/MACOS.md) |

## Install

**PC (Windows)** — download `PepoMote.exe` and run it. No installer. SmartScreen may warn because the binary is unsigned: *More info → Run anyway* (verify `SHA256SUMS.txt` if in doubt). Allow it on *private networks* when the firewall asks.

**PC (Linux)** — download the AppImage, make it executable and run it (or the `tar.gz`: unpack it and run `./install.sh`, which installs the binary, the launcher and the uinput rule). On Sway, Hyprland, MangoWC, river, labwc, niri and other wlroots compositors nothing else is needed: the cursor is a Wayland virtual pointer. On GNOME, KDE or X11 the cursor goes through uinput: if that (or a firewall silently dropping the phone's traffic — many distros ship one enabled) needs setup, PepoMote detects it and asks for your admin password **once** in the system dialog, then fixes it by itself; if your session has no password dialog, the window shows the one-line command to paste in a terminal. Prefer a scripted install with a launcher entry? `packaging/linux/install.sh PepoMote-x86_64.AppImage` does the same setup non-interactively. If the window does not open, the receiver retries by itself with software rendering and with X11, and everything is written to `~/.config/pepomote/receptor.log`. Something odd? `./PepoMote-x86_64.AppImage --diag` prints a report to paste in an issue. Details in [docs/LINUX.md](docs/LINUX.md).

**PC (macOS, beta)** — download `PepoMote-macOS.dmg`, open it and drag PepoMote to Applications. It is not notarized by Apple (that needs a paid developer account), so the first launch of each version takes one extra step: on macOS 15/26, System Settings → Privacy & Security → **Open Anyway** (on 13/14: Control-click → Open). Then allow **Local Network** and, from the card the window shows, **Accessibility** (moves the cursor; no restart needed) and, only for Cemu's second screen, **Screen Recording**. Apple Silicon only, macOS 13 or later. It is built and tested by the CI on a Mac, but I could not try it on a real one yet: details, permissions and troubleshooting in [docs/MACOS.md](docs/MACOS.md).

**Phone** — install `PepoMote.apk` (enable "install from unknown sources"). Open it, tap **Conectar**, scan the QR shown on your PC. Paired forever.

**Two Android phones** — install the same APK on both. On the phone running the games, open **Server**; on the other, tap **Connect** and scan its QR. The controller recognizes the Android server and offers Dolphin, Eden and RetroArch (each console's pad from RetroArch's history; RetroArch's network gamepad is switched on once from its own menu, with a guided check). Guided controller setup explains each tap and remembers the permission for next time. See the [Android server and VPN guide (Spanish)](docs/ANDROID-SERVER.md).

**iPhone / iPad** — not on the App Store. My personal recommendation is [SideStore](https://sidestore.io) (it re-signs itself on the device every 7 days, no PC running; it is what I use), but [AltStore](https://altstore.io) (needs AltServer on your PC to renew; its authors are working on removing that) and Sideloadly (by hand from the PC with `PepoMote.ipa`) work too. In SideStore or AltStore: Sources → **+** → add `https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json` → Browse → PepoMote → Install; updates show up right there. One-tap links for a web page or a chat: `sidestore://source?url=https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json` and `altstore://source?url=https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json`. Step by step, permissions and what differs from Android in [docs/IOS.md](docs/IOS.md).

**Linux phone** (Mobian, postmarketOS…) — Mobian: download `pepomote-mobile_*_arm64.deb` on the phone, tap it and press **Install**. Any other distro, one command in the terminal: `wget -qO- https://raw.githubusercontent.com/pepitolas13/PepoMote/main/packaging/linux-mobile/install.sh | sh`. Then open it like any app: **Conectar** → pick your PC → type the 4-digit code shown under the QR. See [docs/MOBILE-LINUX.md](docs/MOBILE-LINUX.md).

## Play Wii games

See [docs/SETUP-DOLPHIN.md](docs/SETUP-DOLPHIN.md) — two minutes of one-time Dolphin setup (DSU server + bundled controller profile), then: aim at the screen to point, hold the target button to recenter, swing to bowl. Games that ask you to move the remote closer to the screen (WarioWare: Smooth Moves): hold **Closer** on the phone (Dolphin 2407+, where PepoMote generates the IR dots itself, so the cursor also rotates with the phone).

## Play Wii U games

See [docs/SETUP-CEMU.md](docs/SETUP-CEMU.md) — tap **Wii U** in the app, hold the phone sideways, open Cemu. Nothing to configure: PepoMote writes Cemu's controller profile (GamePad from your phone's DSU pad, motion and touch included) while Cemu is closed.

## Play Switch games

Close Eden, tap **Switch** on the phone, wait for setup confirmation. Open Eden and play. Setup, restoration and multiplayer: [docs/SETUP-SWITCH.md](docs/SETUP-SWITCH.md). This mode integrates Eden; Ryujinx is not included yet.

## Play in RetroArch

**With a PC receiver:** close RetroArch, tap **RetroArch** on the phone and wait for setup confirmation. Open RetroArch and load a game: the controller follows the console detected by the receiver. Pick another console, the full RetroPad, **Sideways Wii** or **Light gun** in the header. A second phone takes player 2, up to four players.

**With an Android server:** open **Server → RetroArch** on the device that will run the games and follow the guide to enable network commands and each player's network RetroPad. Save the configuration and check the connection. You can grant access to the RetroArch folder to detect the console from its history, or choose a pad manually. The check confirms the command connection: try each player's controls inside a game. [Setup and limitations](docs/SETUP-RETROARCH.md).

## Nice touches

- **Local multiplayer**: up to 4 phones on one PC — scan the same QR and each phone becomes its own Wiimote in Dolphin, with Dolphin's controller config written for you automatically
- **Nunchuk on the same phone** (1.5.5): games that ask for a Nunchuk (Super Mario Galaxy, Zelda, Metroid Prime…) just work with one phone: the controller turns sideways by itself and you get the Nunchuk under your left hand (stick, C, Z) and the Wii Remote under your right (A, B, D-pad, −/+/Home, 1/2), still pointing with the phone. Off by default: turn it on with the **Nunchuk** chip or in Settings (remembered); off, sideways is the NES pad. Details in [docs/SETUP-DOLPHIN.md](docs/SETUP-DOLPHIN.md)
- **Nunchuk on a second phone**: tap **Nunchuk** on the other phone: stick, C, Z and its own accelerometer feed the emulated Nunchuk of your Wiimote, configured in Dolphin for you
- Physical volume keys = A / B triggers. On Android, quick presses are preserved even when several events arrive together
- Turn the phone sideways for a NES-style pad (2D games) — or a full Wii U GamePad in Wii U mode
- Optional start-with-the-system (tray only, no window)
- One QR pairing; reconnects with one tap; auto-discovery on your LAN
- Synthesized UI sounds + haptics (both optional)
- **Automatic mode** (1.4): open Dolphin, Cemu, Eden or RetroArch and the receiver switches mode by itself; closing it keeps the current mode. Enable **Return to pointer when an emulator closes** in Settings for automatic return (off by default)
- **Precision**: hold the crosshair strip and the cursor moves at 40 %, with no jump when you let go; it keeps working even if your finger drifts off the strip
- **Phones without a gyroscope**: the app detects a missing or software-only gyroscope (Moto G04s and other Unisoc phones), warns you once, and the pointer switches to tilting: sideways and up/down, from the accelerometer alone, drift-free because it comes from gravity. **Settings → Pointer sensor** lets you pick Gyroscope or Accelerometer yourself. Needs the updated receiver
- **Several Wii Remotes in Wii U mode** (1.8.5): Cemu's Controller 1 is always a Wii U GamePad. If every phone chooses Wii Remote (Mario Party 10 with two to four phones), PepoMote leaves a GamePad driven by the PC keyboard there (or your own real one) and the Wii Remotes take Controllers 2 and up, so the game boots and reads all of them
- **Target on the sideways controller** (1.8.5): holding the target between − and + recenters the cursor or the pointer sideways too (pointer mode, Dolphin, Wii Remote in Cemu)
- **Home on the Wii Remote** (1.8.5): between 1 and 2 on the upright controller and next to A sideways, in Dolphin, Wii U and Switch modes. Mario Party 10 needs it to pair each emulated Wii Remote in Cemu
- **Browser back/forward and volume that repeats**: in pointer mode the D-pad ← / → go back / forward in the browser (↑ / ↓ stay arrow keys), and holding − / + or the media 🔉 / 🔊 keeps stepping the volume
- **iPhone and iPad** (1.5): the same sender in Swift, with the Wii U GamePad's second screen and everything else; installed with SideStore or AltStore from a one-tap source, built and tested by the CI on macOS
- **Several PCs and automatic reconnection**: the app keeps all your PCs and, if the Wi-Fi drops or the receiver restarts, it comes back by itself without losing the screen or the mode
- **macOS** (1.5.5, beta): the same receiver on Apple Silicon — cursor and keys through Accessibility, Cemu's second screen, an icon in the menu bar, start with the system, a signed `.app` in a DMG
- **New-version notice** (1.5.5): every app tells you when a new release is out, with the link to it. The check is one request to GitHub once a day (only the latest-release page; nothing about you is sent) and it can be switched off in Settings. Controls travel directly between your devices over your local network or configured VPN; PepoMote does not run its own relay service.
- **iPad and tablets** (1.5.5): the remote, the NES pad, the Nunchuk and the Wii U GamePad grow to fill the screen (same rule on iOS and Android; phones stay exactly as they were). And Settings gains **GamePad without touch screen**: it removes the Wii U GamePad's screen (and the second screen) so the sticks, the D-pad and A/B/X/Y get much bigger — on a phone they go side by side, like a Pro Controller
- **GamePad screen full screen** (1.6): with a real controller plugged into the PC, the phone shows only Cemu's GamePad screen (touch included), like the Wii U's own; the receiver merges the phone into your Cemu profile without replacing your controller. Optional keyboard button at the top right
- **Wii Remote + Nunchuk side** (1.6): the first time the sideways Wii Remote + Nunchuk appears, the app asks whether it is the right way up ("Flip it" / "Keep it like this") and the side stays fixed for good (some phones' sensors flipped it on their own); changeable in Settings. The Wii U GamePad has its own side, separately
- **One-piece D-pad** (1.6): like the Wii Remote's, on every controller of the three apps: sliding the thumb changes direction without lifting it, the corners between arms are diagonals and the dead centre is smaller
- **Press modes** (1.9): two new settings on Android, iPhone/iPad and mobile Linux. **Keep pressed when leaving the button** keeps a pressed button pressed until you lift your finger, even if your thumb drifts off it — now **on by default on Android**, which is what fixes 2 letting go in Mario Kart when the thumb slides off. **Slide to press** (off by default) presses the button your finger slides over and releases the previous one. The D-pad, the sticks, the GamePad's touch screen and the header chips stay as they were
- **Collapsible header on the sideways controllers** (1.9): on Android and iPhone/iPad (on mobile Linux the header stays as it was), on the sideways Wii Remote, on the Wii Remote + Nunchuk and on the Wii U / Switch controller the top is now just a pill with the current mode, centred and away from B and from Z/C. Tap it and a card drops down, and it folds back by itself after 4 seconds: on the Wii Remotes the card holds the mode chips, the Nunchuk chip, Keyboard and Exit; on the Wii U / Switch controller it holds the PC name, the player and the pad, the screen fps, the mode chips and Exit, while **Keyboard** stays right beside the pill, always visible and one tap away while you play
- **Dark theme and English**: both follow the system; ES/EN at the top right of every app
- **A chime per player, a tray icon that tells the state, a heartbeat with the RTT** in the receiver window; on Android a themed icon, launcher shortcuts and a Quick Settings tile

## Build from source

- Receiver: `cd desktop && cargo build --release` (macOS: then `bash packaging/macos/bundle.sh` for the `.app` and the DMG)
- Android: `cd android && ./gradlew assembleDebug`
- iOS (Mac): `brew install xcodegen && cd ios && xcodegen generate && xcodebuild -scheme PepoMote build` (the CI builds the unsigned IPA on macOS runners)
- Protocol spec: [protocol/PROTOCOL.md](protocol/PROTOCOL.md) · DSU notes: [protocol/DSU.md](protocol/DSU.md)

## Legal

PepoMote is an original, independent project — not affiliated with, endorsed by, or sponsored by Nintendo. It contains no Nintendo assets, trademarks, fonts or sounds; "Wii" is used only nominatively to describe compatibility with the Dolphin emulator. "Wii U" likewise describes compatibility with the Cemu emulator (open source, MPL-2.0), "Switch" with Eden, and "RetroArch" with the RetroArch frontend (open source, GPL-3.0), whose network gamepad and command interface PepoMote talks to. It does not distribute Dolphin, Cemu, Eden, RetroArch or any games. See [docs/LEGAL.md](docs/LEGAL.md).

## License

GPL-3.0-or-later. Original visual and sound assets: CC-BY-SA 4.0. Font: Nunito (SIL OFL 1.1).

---

Made by [PepoTech](https://www.youtube.com/@PepoTech).
