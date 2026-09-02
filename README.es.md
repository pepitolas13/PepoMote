<p align="center"><img src="assets/brand/logo.png" width="96" alt="PepoMote"></p>

<h1 align="center">PepoMote</h1>
<p align="center"><b>Apunta. Haz clic. Juega.</b> — Convierte tu móvil Android en un puntero y mando con movimiento estilo Wii para tu PC.</p>

**[English](README.md)**

- **Modo puntero** — apuntas con el móvil y el cursor va exactamente ahí (apuntado absoluto anclado al mundo, inmune al roll, 250 Hz). Botones, arrastre, scroll, teclas multimedia. Tu ratón de verdad sigue funcionando siempre que el móvil esté quieto.
- **Modo Dolphin** — PepoMote se convierte en un Wiimote virtual con movimiento completo (servidor DSU/cemuhook en `127.0.0.1:26760`). Juega juegos de Wii reales — bolos de Wii Sports incluidos — en el [emulador Dolphin](https://es.dolphin-emu.org/).

| Pieza | Plataforma | Archivo |
|---|---|---|
| Emisor | Android 8.0+ | `PepoMote.apk` |
| Emisor | Linux móvil: Mobian, postmarketOS… (aarch64) | `pepomote-mobile_*_arm64.deb` (Mobian: tocar e Instalar) · `PepoMote-Mobile-aarch64.flatpak` (postmarketOS y tiendas: tocar e Instalar) · `PepoMote-Mobile-aarch64.AppImage` (glibc) · `PepoMote-Mobile-aarch64-musl.tar.gz` (postmarketOS) |
| Receptor | Windows 10/11 | `PepoMote.exe` — un solo archivo portable |
| Receptor | Linux, X11 y Wayland | `PepoMote-x86_64.AppImage` |

## Instalar

**PC (Windows)** — descarga `PepoMote.exe` y ábrelo. Sin instalador. SmartScreen puede avisar porque el binario no está firmado: *Más información → Ejecutar de todas formas* (verifica `SHA256SUMS.txt` si dudas). Cuando el firewall pregunte, permite en *redes privadas*.

**PC (Linux)** — descarga el AppImage y, desde el repo, ejecuta `packaging/linux/install.sh PepoMote-x86_64.AppImage` (instala la regla udev de uinput — necesaria para mover el cursor — y el lanzador). Cierra sesión y vuelve a entrar una vez.

**Móvil** — instala `PepoMote.apk` (permite "orígenes desconocidos"). Ábrela, toca **Conectar** y escanea el QR del PC. Emparejado para siempre.

**Móvil con Linux** (Mobian, postmarketOS…) — `packaging/linux-mobile/install.sh <paquete>` y ábrela como una app más: **Conectar** → tu PC en la lista → el código de 4 dígitos que hay bajo el QR. Ver [docs/MOBILE-LINUX.md](docs/MOBILE-LINUX.md).

## Jugar a la Wii

Mira [docs/SETUP-DOLPHIN.md](docs/SETUP-DOLPHIN.md) — dos minutos de configuración de Dolphin una sola vez (servidor DSU + perfil de mando incluido), y después: apunta a la pantalla, mantén la diana para recentrar, balancea para lanzar la bola.

## Detalles finos

- **Multijugador local**: hasta 4 móviles en el mismo PC — escanean el mismo QR y cada uno es su propio Wiimote en Dolphin, con la configuración de mandos de Dolphin escrita sola
- Botones físicos de volumen = A / B (latencia táctil cero)
- Gira el móvil y tienes un mando estilo NES (juegos 2D)
- Arranque con el sistema opcional (solo bandeja, sin ventana)
- Emparejado por QR una vez; reconexión de un toque; autodescubrimiento en tu red
- Sonidos UI sintetizados + háptica (ambos opcionales)

## Compilar desde el código

- Receptor: `cd desktop && cargo build --release`
- Android: `cd android && ./gradlew assembleDebug`
- Spec del protocolo: [protocol/PROTOCOL.md](protocol/PROTOCOL.md) · notas DSU: [protocol/DSU.md](protocol/DSU.md)

## Legal

PepoMote es un proyecto original e independiente — sin afiliación, respaldo ni patrocinio de Nintendo. No contiene assets, marcas, tipografías ni sonidos de Nintendo; "Wii" se usa solo de forma nominativa para describir compatibilidad con el emulador Dolphin. No distribuye Dolphin ni juegos. Ver [docs/LEGAL.md](docs/LEGAL.md).

## Licencia

GPL-3.0-or-later. Assets visuales y sonoros originales: CC-BY-SA 4.0. Tipografía: Nunito (SIL OFL 1.1).

---

Hecho por [PepoTech](https://www.youtube.com/@PepoTech).
