<p align="center"><img src="assets/brand/logo.png" width="96" alt="PepoMote"></p>

<h1 align="center">PepoMote</h1>
<p align="center"><b>Apunta. Haz clic. Juega.</b> — Convierte tu móvil Android en un puntero y mando con movimiento estilo Wii para tu PC.</p>

**[English](README.md)**

- **Modo puntero** — apuntas con el móvil y el cursor va exactamente ahí (apuntado absoluto anclado al mundo, inmune al roll, 250 Hz). Botones, arrastre, scroll, teclas multimedia. Tu ratón de verdad sigue funcionando siempre que el móvil esté quieto.
- **Modo Dolphin** — PepoMote se convierte en un Wiimote virtual con movimiento completo (servidor DSU/cemuhook en `127.0.0.1:26760`). Juega juegos de Wii reales — bolos de Wii Sports incluidos — en el [emulador Dolphin](https://es.dolphin-emu.org/).
- **Modo Wii U** — pon el móvil apaisado y es un Wii U GamePad para el [emulador Cemu](https://cemu.info/): dos sticks, A/B/X/Y, L/R/ZL/ZR, giroscopio, pantalla táctil. Los perfiles de mando de Cemu se escriben solos; un segundo móvil es un Pro Controller, y cualquiera puede ser un Mando de Wii (con Nunchuk) para los juegos de Wii U que se juegan así. **Con doble pantalla**: la pantalla del GamePad (mapa, inventario, jugar sin TV) llega de Cemu al móvil (y se esconde en el PC), tocarla es tocar la pantalla del GamePad, y el teclado del móvil escribe en el teclado en pantalla de Cemu (el nombre del jugador y demás).

| Pieza | Plataforma | Archivo |
|---|---|---|
| Emisor | Android 8.0+ | `PepoMote.apk` |
| Emisor | iPhone / iPad, iOS 15+ (hasta iPadOS 26) | `PepoMote.ipa` — se instala con SideStore o AltStore (fuente más abajo) o Sideloadly, ver [docs/IOS.md](docs/IOS.md) |
| Emisor | Linux móvil: Mobian, postmarketOS… (aarch64) | `pepomote-mobile_*_arm64.deb` (Mobian: tocar e Instalar) · `PepoMote-Mobile-aarch64.AppImage` (glibc) · `PepoMote-Mobile-aarch64-musl.tar.gz` (postmarketOS) |
| Receptor | Windows 10/11 | `PepoMote.exe` — un solo archivo portable |
| Receptor | Linux, X11 y Wayland | `PepoMote-x86_64.AppImage` |

## Instalar

**PC (Windows)** — descarga `PepoMote.exe` y ábrelo. Sin instalador. SmartScreen puede avisar porque el binario no está firmado: *Más información → Ejecutar de todas formas* (verifica `SHA256SUMS.txt` si dudas). Cuando el firewall pregunte, permite en *redes privadas*.

**PC (Linux)** — descarga el AppImage, dale permiso de ejecución y ábrelo. En Sway, Hyprland, MangoWC, river, labwc, niri y demás compositores wlroots no hace falta nada más: el cursor es un puntero virtual de Wayland. En GNOME, KDE o X11 el cursor va por uinput: si eso (o un firewall descartando en silencio el tráfico del móvil — muchas distros traen uno activado) necesita configuración, PepoMote lo detecta y te pide la contraseña de administrador **una sola vez** en el diálogo del sistema, y lo arregla él solo; si tu sesión no tiene diálogo de contraseña, la ventana enseña el comando de una línea para pegar en un terminal. ¿Prefieres instalación con lanzador? `packaging/linux/install.sh PepoMote-x86_64.AppImage` hace la misma configuración de golpe. ¿Algo raro? `./PepoMote-x86_64.AppImage --diag` imprime un informe para pegar en un issue.

**Móvil** — instala `PepoMote.apk` (permite "orígenes desconocidos"). Ábrela, toca **Conectar** y escanea el QR del PC. Emparejado para siempre.

**iPhone / iPad** — no está en la App Store: instálala con [SideStore](https://sidestore.io) (recomendado: se renueva sola en el propio iPad cada 7 días, sin PC encendido) o con [AltStore](https://altstore.io) (necesita AltServer en tu PC para renovar; sus autores trabajan en quitar ese requisito). En cualquiera de las dos: Sources → **+** → añade `https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json` → Browse → PepoMote → Instalar; las actualizaciones te salen ahí mismo. Enlaces de un toque para una web o un chat: `sidestore://source?url=https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json` y `altstore://source?url=https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json`. Sideloadly con `PepoMote.ipa` también vale. Paso a paso, permisos y diferencias con Android en [docs/IOS.md](docs/IOS.md).

**Móvil con Linux** (Mobian, postmarketOS…) — Mobian: descarga `pepomote-mobile_*_arm64.deb` en el móvil, tócalo y pulsa **Instalar**. Cualquier otra distro, un comando en el terminal: `wget -qO- https://raw.githubusercontent.com/pepitolas13/PepoMote/main/packaging/linux-mobile/install.sh | sh`. Después ábrela como una app más: **Conectar** → tu PC en la lista → el código de 4 dígitos que hay bajo el QR. Ver [docs/MOBILE-LINUX.md](docs/MOBILE-LINUX.md).

## Jugar a la Wii

Mira [docs/SETUP-DOLPHIN.md](docs/SETUP-DOLPHIN.md) — dos minutos de configuración de Dolphin una sola vez (servidor DSU + perfil de mando incluido), y después: apunta a la pantalla, mantén la diana para recentrar, balancea para lanzar la bola.

## Jugar a la Wii U

Mira [docs/SETUP-CEMU.md](docs/SETUP-CEMU.md) — toca **Wii U** en la app, sostén el móvil apaisado y abre Cemu. Nada que configurar: PepoMote escribe el perfil de mando de Cemu (GamePad desde el pad DSU de tu móvil, con movimiento y táctil) con Cemu cerrado.

## Detalles finos

- **Multijugador local**: hasta 4 móviles en el mismo PC — escanean el mismo QR y cada uno es su propio Wiimote en Dolphin, con la configuración de mandos de Dolphin escrita sola
- **Nunchuk**: un segundo móvil en la otra mano (toca **Nunchuk** en la app): stick, C, Z y su propio acelerómetro alimentan el Nunchuk emulado de tu Wiimote, configurado solo en Dolphin
- Botones físicos de volumen = A / B (latencia táctil cero)
- Gira el móvil y tienes un mando estilo NES (juegos 2D) — o un Wii U GamePad completo en modo Wii U
- Arranque con el sistema opcional (solo bandeja, sin ventana)
- Emparejado por QR una vez; reconexión de un toque; autodescubrimiento en tu red
- Sonidos UI sintetizados + háptica (ambos opcionales)
- **Modo automático** (1.4): abre Dolphin o Cemu y el receptor cambia de modo solo; ciérralo y vuelve el puntero
- **Precisión**: mantén la tira de la mirilla y el cursor va al 40 %, sin salto al soltar; sigue aunque el dedo se salga de la tira
- **Atrás/adelante y volumen que repite**: en modo puntero la cruceta ← / → va atrás / adelante en el navegador (↑ / ↓ siguen siendo flechas) y mantener − / + o el 🔉 / 🔊 de multimedia sigue bajando o subiendo el volumen
- **iPhone y iPad** (1.5): el mismo emisor en Swift, con la doble pantalla del GamePad de Wii U y todo lo demás; se instala con SideStore o AltStore desde una fuente de un toque, y lo compila y prueba la CI en macOS
- **Varios PCs y reconexión automática**: la app guarda todos tus PCs y, si se cae la Wi-Fi o reinicias el receptor, vuelve sola sin perder la pantalla ni el modo
- **Tema oscuro e inglés**: siguen al sistema; ES/EN arriba a la derecha en las tres apps
- **Campanita por jugador, bandeja que cuenta el estado, latido con el RTT** en la ventana del receptor; en Android icono temático, accesos directos y tile de Ajustes rápidos

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
