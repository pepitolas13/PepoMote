# PepoMote

**[English](README.md)**

Convierte tu móvil Android en un puntero y mando con movimiento estilo Wii para tu PC.

- **Modo puntero**: apunta con el móvil a la pantalla y mueve el cursor del sistema — botones, scroll, arrastre, teclas multimedia. Se siente como un Wiimote y va por tu red local.
- **Modo Dolphin**: PepoMote se convierte en un mando virtual con movimiento completo (servidor DSU/cemuhook) para jugar juegos de Wii reales en el [emulador Dolphin](https://es.dolphin-emu.org/) con controles de movimiento de verdad.

| Pieza | Plataforma | Artefacto |
|---|---|---|
| Emisor | Android 8.0+ | `PepoMote.apk` |
| Receptor | Windows 10/11 | `PepoMote.exe` (un solo archivo portable) |
| Receptor | Linux (X11 y Wayland) | `PepoMote-x86_64.AppImage` |

## Estado

En desarrollo (pre-1.0). Hitos: enlace → puntero fino → Dolphin → pulido → release.

## Compilar

- Receptor: `cd desktop && cargo build --release`
- Android: `cd android && ./gradlew assembleDebug`

## Legal

PepoMote es un proyecto original e independiente. No está afiliado, respaldado ni patrocinado por Nintendo. No contiene assets, marcas, tipografías ni sonidos de Nintendo. "Wii" se usa solo de forma nominativa para describir compatibilidad ("jugar juegos de Wii vía el emulador Dolphin"). Ver [docs/LEGAL.md](docs/LEGAL.md).

## Licencia

GPL-3.0-or-later. Assets visuales/sonoros originales: CC-BY-SA 4.0.
