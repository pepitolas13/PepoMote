# PepoMote

**[Español](README.es.md)**

Turn your Android phone into a Wii-style motion pointer and controller for your PC.

- **Pointer mode**: aim your phone at the screen and drive the OS cursor — buttons, scroll, drag, media keys. Feels like a Wiimote, works over your LAN.
- **Dolphin mode**: PepoMote becomes a full-motion virtual controller (DSU/cemuhook server) so you can play real Wii games in the [Dolphin emulator](https://dolphin-emu.org/) with actual motion controls.

| Piece | Platform | Artifact |
|---|---|---|
| Sender | Android 8.0+ | `PepoMote.apk` |
| Receiver | Windows 10/11 | `PepoMote.exe` (single portable file) |
| Receiver | Linux (X11 & Wayland) | `PepoMote-x86_64.AppImage` |

## Status

Work in progress (pre-1.0). Milestones: link → fine pointer → Dolphin → polish → release.

## Build

- Receiver: `cd desktop && cargo build --release`
- Android: `cd android && ./gradlew assembleDebug`

## Legal

PepoMote is an original, independent project. It is not affiliated with, endorsed by, or sponsored by Nintendo. It contains no Nintendo assets, trademarks, fonts or sounds. "Wii" is used only nominatively to describe compatibility ("play Wii games via the Dolphin emulator"). See [docs/LEGAL.md](docs/LEGAL.md).

## License

GPL-3.0-or-later. Original visual/sound assets: CC-BY-SA 4.0.
