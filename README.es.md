<p align="center"><img src="assets/brand/logo.png" width="96" alt="PepoMote"></p>

<h1 align="center">PepoMote</h1>
<p align="center"><b>Apunta. Haz clic. Juega.</b> — Convierte tu móvil en un puntero y mando con movimiento para tu PC, o conecta dos Android para jugar.</p>

**[English](README.md)**

**PepoMote 1.9:** botones que responden como esperas. **Mantener al salir del botón** (en Android viene encendido: el 2 ya no se suelta solo en Mario Kart) y **Pulsar deslizando**, dos ajustes nuevos en Android, iPhone/iPad y Linux móvil; una **cabecera plegable** en los mandos apaisados, incluido el de Wii U y Switch, que deja los botones despejados y el Teclado siempre a mano; el selector de sensor cambia al momento y **Conectar** enseña al instante el PC con el que ya hay sesión. Incluye las novedades de la 1.8.5: varios Mandos de Wii a la vez en Wii U, apuntado por inclinación sin giroscopio, Home y **Acercar** en Dolphin, y las de la 1.8: servidor Android para Dolphin y Eden, configuración guiada y conexión por código o entre redes. [Descargas y novedades](https://github.com/pepitolas13/PepoMote/releases/tag/v1.9.0).

- **Modo puntero** — apuntas con el móvil y el cursor va exactamente ahí (apuntado absoluto anclado al mundo, inmune al roll, 250 Hz). Botones, arrastre, scroll, teclas multimedia. Tu ratón de verdad sigue funcionando siempre que el móvil esté quieto. Necesita giroscopio: un móvil sin giroscopio real (muchos de gama baja) lo avisa y apunta por inclinación, con el sensor a elegir en Ajustes.
- **Modo Dolphin** — PepoMote se convierte en un Wiimote virtual con movimiento completo (servidor DSU/cemuhook en `127.0.0.1:26760`). Juega juegos de Wii reales — bolos de Wii Sports incluidos — en el [emulador Dolphin](https://es.dolphin-emu.org/).
- **Modo Wii U** — pon el móvil apaisado y es un Wii U GamePad para el [emulador Cemu](https://cemu.info/): dos sticks, A/B/X/Y, L/R/ZL/ZR, giroscopio, pantalla táctil. Los perfiles de mando de Cemu se escriben solos; un segundo móvil es un Pro Controller, y cualquiera (o todos a la vez) puede ser un Mando de Wii (con Nunchuk) para los juegos de Wii U que se juegan así. **Con doble pantalla**: la pantalla del GamePad (mapa, inventario, jugar sin TV) llega de Cemu al móvil (y se esconde en el PC), tocarla es tocar la pantalla del GamePad, y el teclado del móvil escribe en el teclado en pantalla de Cemu (el nombre del jugador y demás). O, con un mando de verdad en el PC, el móvil como pantalla táctil del GamePad y nada más, a pantalla completa (**Pantalla del GamePad a pantalla completa** en Ajustes).

- **Modo Switch** — Pro Controller para Eden, con un jugador independiente por móvil. Botones, ambos sticks, movimiento si hay giroscopio, Capturar y teclado; configuración automática con copia y restauración de tus mandos. [Guía de Switch](docs/SETUP-SWITCH.md).
- **Modo RetroArch** — el móvil es un mando para [RetroArch](https://www.retroarch.com/): **el mando de la consola del juego que RetroArch acaba de cargar** (NES, SNES, Mega Drive, N64, PlayStation… con sus botones y sus etiquetas, o el **RetroPad** completo de dos sticks), el **Mando de Wii de lado** o una **pistola de luz** (apuntas con el móvil: B dispara, A recarga). Usa el mando en red y los comandos de red del propio RetroArch, así que no hay driver y va igual en Windows, Linux y macOS; PepoMote activa las dos cosas en `retroarch.cfg` con RetroArch cerrado. Home abre el menú de RetroArch, un botón hace avance rápido mientras se mantiene, y la cabecera del móvil trae guardar y cargar estado, ranura, rebobinar, pausa, captura y reiniciar. Un jugador por móvil, hasta cuatro. [Guía de RetroArch](docs/SETUP-RETROARCH.md).

| Pieza | Plataforma | Archivo |
|---|---|---|
| Emisor y servidor para Dolphin/Eden | Android 8.0+ (el servidor también debe cumplir los requisitos del emulador) | `PepoMote.apk` |
| Emisor | iPhone / iPad, iOS 15+ (hasta iPadOS 26) | `PepoMote.ipa` — se instala con SideStore (mi recomendación), AltStore o Sideloadly, ver [docs/IOS.md](docs/IOS.md) |
| Emisor | Linux móvil: Mobian, postmarketOS… (aarch64) | `pepomote-mobile_*_arm64.deb` (Mobian: tocar e Instalar) · `PepoMote-Mobile-aarch64.AppImage` (glibc) · `PepoMote-Mobile-aarch64-musl.tar.gz` (postmarketOS) |
| Receptor | Windows 10/11 | `PepoMote.exe` — un solo archivo portable |
| Receptor | Linux, X11 y Wayland | `PepoMote-x86_64.AppImage` · `PepoMote-linux-x86_64.tar.gz` (binario + lanzador + `install.sh`) |
| Receptor | macOS 13+ con chip Apple (M1 o posterior) — **beta** | `PepoMote-macOS.dmg`, ver [docs/MACOS.md](docs/MACOS.md) |

## Instalar

**PC (Windows)** — descarga `PepoMote.exe` y ábrelo. Sin instalador. SmartScreen puede avisar porque el binario no está firmado: *Más información → Ejecutar de todas formas* (verifica `SHA256SUMS.txt` si dudas). Cuando el firewall pregunte, permite en *redes privadas*.

**PC (Linux)** — descarga el AppImage, dale permiso de ejecución y ábrelo (o el `tar.gz`: descomprímelo y ejecuta `./install.sh`, que instala el binario, el lanzador y la regla de uinput). En Sway, Hyprland, MangoWC, river, labwc, niri y demás compositores wlroots no hace falta nada más: el cursor es un puntero virtual de Wayland. En GNOME, KDE o X11 el cursor va por uinput: si eso (o un firewall descartando en silencio el tráfico del móvil — muchas distros traen uno activado) necesita configuración, PepoMote lo detecta y te pide la contraseña de administrador **una sola vez** en el diálogo del sistema, y lo arregla él solo; si tu sesión no tiene diálogo de contraseña, la ventana enseña el comando de una línea para pegar en un terminal. ¿Prefieres instalación con lanzador? `packaging/linux/install.sh PepoMote-x86_64.AppImage` hace la misma configuración de golpe. Si la ventana no abre, el receptor reintenta solo con render por software y con X11, y todo queda en `~/.config/pepomote/receptor.log`. ¿Algo raro? `./PepoMote-x86_64.AppImage --diag` imprime un informe para pegar en un issue. Detalles en [docs/LINUX.md](docs/LINUX.md).

**PC (macOS, beta)** — descarga `PepoMote-macOS.dmg`, ábrelo y arrastra PepoMote a Aplicaciones. No está notarizado por Apple (eso exige una cuenta de desarrollador de pago), así que la primera apertura de cada versión lleva un paso más: en macOS 15/26, Ajustes del Sistema → Privacidad y seguridad → **Abrir igualmente** (en 13/14: Control-clic → Abrir). Después permite **Red local** y, desde la tarjeta que enseña la ventana, **Accesibilidad** (mueve el cursor; sin reiniciar) y, solo para la doble pantalla de Cemu, **Grabación de pantalla**. Solo Mac con chip Apple y macOS 13 o superior. Lo compila y prueba la CI en un Mac, pero aún no lo he podido probar en uno de verdad: detalles, permisos y problemas en [docs/MACOS.md](docs/MACOS.md).

**Móvil** — instala `PepoMote.apk` (permite "orígenes desconocidos"). Ábrela, toca **Conectar** y escanea el QR del PC. Emparejado para siempre.

**Dos móviles Android** — instala la misma APK en ambos. En el que ejecutará los juegos, abre **Servidor**; en el otro, **Conectar** y escanea su QR. El mando reconoce el servidor Android y ofrece Dolphin, Eden y RetroArch (el mando de cada consola desde el historial de RetroArch; su mando en red se activa una vez desde su propio menú, con comprobación guiada). La configuración de controles te guía paso a paso y guarda el permiso para las siguientes veces. [Guía del servidor Android y conexión entre redes](docs/ANDROID-SERVER.md).

**iPhone / iPad** — no está en la App Store. Yo recomiendo personalmente [SideStore](https://sidestore.io) (se renueva sola en el propio iPad cada 7 días, sin PC encendido; es la que uso), aunque también se puede con [AltStore](https://altstore.io) (necesita AltServer en tu PC para renovar; sus autores trabajan en quitar ese requisito) y con Sideloadly (a mano desde el PC con `PepoMote.ipa`). En SideStore o AltStore: Sources → **+** → añade `https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json` → Browse → PepoMote → Instalar; las actualizaciones te salen ahí mismo. Enlaces de un toque para una web o un chat: `sidestore://source?url=https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json` y `altstore://source?url=https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json`. Paso a paso, permisos y diferencias con Android en [docs/IOS.md](docs/IOS.md).

**Móvil con Linux** (Mobian, postmarketOS…) — Mobian: descarga `pepomote-mobile_*_arm64.deb` en el móvil, tócalo y pulsa **Instalar**. Cualquier otra distro, un comando en el terminal: `wget -qO- https://raw.githubusercontent.com/pepitolas13/PepoMote/main/packaging/linux-mobile/install.sh | sh`. Después ábrela como una app más: **Conectar** → tu PC en la lista → el código de 4 dígitos que hay bajo el QR. Ver [docs/MOBILE-LINUX.md](docs/MOBILE-LINUX.md).

## Jugar a la Wii

Mira [docs/SETUP-DOLPHIN.md](docs/SETUP-DOLPHIN.md) — dos minutos de configuración de Dolphin una sola vez (servidor DSU + perfil de mando incluido), y después: apunta a la pantalla, mantén la diana para recentrar, balancea para lanzar la bola. Los juegos que piden acercar el mando a la pantalla (WarioWare: Smooth Moves): mantén **Acercar** en el móvil (Dolphin 2407+, donde PepoMote genera él mismo los puntos IR, así que el cursor también gira con el móvil).

## Jugar a la Wii U

Mira [docs/SETUP-CEMU.md](docs/SETUP-CEMU.md) — toca **Wii U** en la app, sostén el móvil apaisado y abre Cemu. Nada que configurar: PepoMote escribe el perfil de mando de Cemu (GamePad desde el pad DSU de tu móvil, con movimiento y táctil) con Cemu cerrado.

## Jugar a la Switch

Cierra Eden, toca **Switch** en el móvil, espera el aviso de configuración. Abre Eden y juega. Instrucciones, restauración y multijugador en [docs/SETUP-SWITCH.md](docs/SETUP-SWITCH.md). Este modo integra Eden; Ryujinx aún no está incluido.

## Jugar en RetroArch

Cierra RetroArch, toca **RetroArch** en el móvil, espera el aviso de configuración. Abre RetroArch y juega: el móvil es un RetroPad en el jugador 1 (puerto 55400), un segundo móvil en el jugador 2, y así. Elige **Mando NES** o **Pistola** en la cabecera del móvil para los juegos de 8 bits y para los núcleos con Zapper/GunCon. Cómo se dosifica la entrada a los fotogramas de RetroArch, la pistola, el teclado y dónde vive `retroarch.cfg`: [docs/SETUP-RETROARCH.md](docs/SETUP-RETROARCH.md).

## Detalles finos

- **Multijugador local**: hasta 4 móviles en el mismo PC — escanean el mismo QR y cada uno es su propio Wiimote en Dolphin, con la configuración de mandos de Dolphin escrita sola
- **Nunchuk en el mismo móvil** (1.5.5): los juegos que piden Nunchuk (Super Mario Galaxy, Zelda, Metroid Prime…) funcionan con un solo móvil: el mando se pone solo de lado y tienes el Nunchuk en la mano izquierda (stick, C, Z) y el Mando de Wii en la derecha (A, B, cruceta, −/+/Home, 1/2), apuntando con el móvil como siempre. Viene apagado: enciéndelo con el chip **Nunchuk** o en Ajustes (se recuerda); apagado, de lado es el mando NES. Detalles en [docs/SETUP-DOLPHIN.md](docs/SETUP-DOLPHIN.md)
- **Nunchuk en un segundo móvil**: toca **Nunchuk** en el otro móvil: stick, C, Z y su propio acelerómetro alimentan el Nunchuk emulado de tu Wiimote, configurado solo en Dolphin
- Botones físicos de volumen = A / B. En Android se conservan las pulsaciones rápidas aunque varios eventos lleguen juntos
- Gira el móvil y tienes un mando estilo NES (juegos 2D) — o un Wii U GamePad completo en modo Wii U
- Arranque con el sistema opcional (solo bandeja, sin ventana)
- Emparejado por QR una vez; reconexión de un toque; autodescubrimiento en tu red
- Sonidos UI sintetizados + háptica (ambos opcionales)
- **Modo automático** (1.4): abre Dolphin, Cemu o Eden y el receptor cambia de modo solo; al cerrarlo conserva el modo. Activa **Volver al puntero al cerrar un emulador** en Ajustes si prefieres el retorno automático (desactivado por defecto)
- **Precisión**: mantén la tira de la mirilla y el cursor va al 40 %, sin salto al soltar; sigue aunque el dedo se salga de la tira
- **Móviles sin giroscopio**: la app detecta que falta o que es solo software (Moto G04s y otros Unisoc), te avisa una vez y el puntero pasa a inclinación: a los lados y arriba/abajo, solo con el acelerómetro, sin deriva porque sale de la gravedad. En **Ajustes → Sensor del puntero** eliges tú Giroscopio o Acelerómetro. Necesita el receptor actualizado
- **Varios Mandos de Wii en Wii U** (1.8.5): el mando 1 de Cemu es siempre un Wii U GamePad. Si todos los móviles eligen Mando de Wii (Mario Party 10 con dos, tres o cuatro móviles), PepoMote deja ahí un GamePad manejado por el teclado del PC (o tu mando real) y los Mandos de Wii van a los mandos 2 en adelante: el juego arranca y los lee todos
- **Diana en el mando apaisado** (1.8.5): mantener la diana entre − y + recentra el cursor o el puntero también de lado (puntero, Dolphin y Mando de Wii en Cemu)
- **Home en el Mando de Wii** (1.8.5): entre 1 y 2 en el mando vertical y junto a A en el apaisado, en Dolphin, Wii U y Switch. Mario Party 10 lo pide para dar por emparejado cada Mando de Wii emulado en Cemu
- **Atrás/adelante y volumen que repite**: en modo puntero la cruceta ← / → va atrás / adelante en el navegador (↑ / ↓ siguen siendo flechas) y mantener − / + o el 🔉 / 🔊 de multimedia sigue bajando o subiendo el volumen
- **iPhone y iPad** (1.5): el mismo emisor en Swift, con la doble pantalla del GamePad de Wii U y todo lo demás; se instala con SideStore o AltStore desde una fuente de un toque, y lo compila y prueba la CI en macOS
- **Varios PCs y reconexión automática**: la app guarda todos tus PCs y, si se cae la Wi-Fi o reinicias el receptor, vuelve sola sin perder la pantalla ni el modo
- **macOS** (1.5.5, beta): el mismo receptor en Mac con chip Apple — cursor y teclas por Accesibilidad, doble pantalla de Cemu, icono en la barra de menús, arranque con el sistema, `.app` firmado en un DMG
- **Aviso de versión nueva** (1.5.5): todas las apps avisan cuando sale una versión, con el enlace a la release. La comprobación es una consulta a GitHub una vez al día (solo la página de la última versión; no se envía nada tuyo) y se apaga en Ajustes. Los controles viajan directamente entre tus dispositivos, por la red local o por la VPN que hayas configurado; PepoMote no tiene un servidor de retransmisión propio.
- **iPad y tablets** (1.5.5): el mando, el pad NES, el Nunchuk y el GamePad de Wii U crecen hasta llenar la pantalla (misma regla en iOS y Android; los móviles quedan exactamente como estaban). Y en Ajustes está **GamePad sin pantalla táctil**: quita la pantalla del GamePad de Wii U (y la doble pantalla) para que sticks, cruceta y A/B/X/Y sean mucho más grandes; en un móvil pasan a ir uno al lado del otro, como en un Pro Controller
- **Pantalla del GamePad a pantalla completa** (1.6): con un mando de verdad conectado al PC, el móvil enseña solo la pantalla del GamePad de Cemu (táctil incluido), como la de la Wii U; el receptor añade el móvil a tu perfil de Cemu sin pisar tu mando. Botón de teclado opcional arriba a la derecha
- **Lado del mando + Nunchuk** (1.6): la primera vez que sale el mando + Nunchuk apaisado, la app pregunta si está bien girado («Darle la vuelta» / «Así lo quiero») y el lado se queda fijo para siempre (el sensor de algunos móviles le daba la vuelta solo); en Ajustes se cambia. El GamePad de Wii U tiene su propio lado, aparte
- **Cruceta de una pieza** (1.6): como la del Mando de Wii, en todos los mandos de las tres apps: deslizar el pulgar cambia de dirección sin levantarlo, las esquinas entre brazos son diagonales y el centro muerto es más pequeño
- **Modos de pulsación** (1.9): dos ajustes nuevos en Android, iPhone/iPad y Linux móvil. **Mantener al salir del botón** deja pulsado el botón mientras no levantes el dedo, aunque el pulgar se salga de él — en Android ahora viene **encendido**, que es lo que arregla que el 2 se soltara solo en Mario Kart al correrse el pulgar. **Pulsar deslizando** (apagado de serie) pulsa el botón por el que pasa el dedo y suelta el anterior. La cruceta, los sticks, la pantalla táctil del GamePad y los chips de la cabecera siguen como estaban
- **Cabecera plegable en los mandos apaisados** (1.9): en Android y en iPhone/iPad (en Linux móvil la cabecera sigue como estaba), en el mando de lado, en el mando + Nunchuk y en el mando de Wii U / Switch arriba solo queda una pastilla con el modo, en el centro y lejos de B y de Z/C. Tócala y baja una tarjeta que se pliega sola a los 4 segundos: en los mandos de Wii lleva los chips de modo, el chip Nunchuk, Teclado y Salir; en el mando de Wii U / Switch lleva el nombre del PC, el jugador y el pad, los fps de la pantalla, los chips de modo y Salir, y **Teclado** se queda justo al lado de la pastilla, siempre a la vista y a un toque mientras juegas
- **Tema oscuro e inglés**: siguen al sistema; ES/EN arriba a la derecha en todas las apps
- **Campanita por jugador, bandeja que cuenta el estado, latido con el RTT** en la ventana del receptor; en Android icono temático, accesos directos y tile de Ajustes rápidos

## Compilar desde el código

- Receptor: `cd desktop && cargo build --release` (macOS: después `bash packaging/macos/bundle.sh` para el `.app` y el DMG)
- Android: `cd android && ./gradlew assembleDebug`
- iOS (Mac): `brew install xcodegen && cd ios && xcodegen generate && xcodebuild -scheme PepoMote build` (la CI compila el IPA sin firmar en runners de macOS)
- Spec del protocolo: [protocol/PROTOCOL.md](protocol/PROTOCOL.md) · notas DSU: [protocol/DSU.md](protocol/DSU.md)

## Legal

PepoMote es un proyecto original e independiente — sin afiliación, respaldo ni patrocinio de Nintendo. No contiene assets, marcas, tipografías ni sonidos de Nintendo; "Wii" se usa solo de forma nominativa para describir compatibilidad con el emulador Dolphin, "Wii U" con Cemu, "Switch" con Eden y "RetroArch" con el frontend RetroArch (código abierto, GPL-3.0), con cuyo mando en red e interfaz de comandos habla PepoMote. No distribuye Dolphin, Cemu, Eden, RetroArch ni juegos. Ver [docs/LEGAL.md](docs/LEGAL.md).

## Licencia

GPL-3.0-or-later. Assets visuales y sonoros originales: CC-BY-SA 4.0. Tipografía: Nunito (SIL OFL 1.1).

---

Hecho por [PepoTech](https://www.youtube.com/@PepoTech).
