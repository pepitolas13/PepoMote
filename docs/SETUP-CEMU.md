# SETUP-CEMU — jugar a la Wii U con PepoMote

Modo **Wii U**: el móvil apaisado es un **Wii U GamePad** para el emulador
[Cemu](https://cemu.info/) (2.0 o más reciente, Windows y Linux): dos sticks,
A/B/X/Y, L/R/ZL/ZR, cruceta, −/+/Home, giroscopio, pantalla táctil, soplar al
micrófono y el botón TV↔Pad de Cemu. Cemu se configura solo.

## Se configura solo

1. En el móvil, tarjeta **Wii U** del menú principal (o el chip **Wii U** dentro
   del mando): se abre directamente el GamePad apaisado. La ventana del PC pasa
   a «Modo Wii U (Cemu)» y deja de mover el cursor: todo va al servidor DSU
   (`127.0.0.1:26760`).
2. Con **Cemu cerrado**, PepoMote escribe en la carpeta de configuración de Cemu
   un perfil de mando por móvil conectado:
   `controllerProfiles/controller0.xml` = Jugador 1 (**Wii U GamePad**, con
   movimiento y táctil), `controller1.xml` = Jugador 2 (**Pro Controller**), y
   así hasta cuatro. Cada perfil lee del pad DSU de su móvil (API
   `DSUController`, `127.0.0.1:26760`), así que en Cemu no hay que tocar nada.
3. Abre Cemu y juega. Si conectas el móvil después de abrir Cemu también vale:
   Cemu reintenta la conexión DSU solo.

Si Cemu estaba abierto cuando el móvil entró, la ventana de PepoMote (y el
móvil, en un aviso) te lo dice: cierra Cemu y pulsa **Configurar Cemu** (o
simplemente vuelve a abrirlo cuando el aviso desaparezca). Si en un índice
había ya un mando tuyo (un gamepad USB, por ejemplo), queda a salvo en
`controllerN.xml.pepomote.bak` y vuelve solo cuando ese jugador se va. El
automatismo se apaga en Ajustes («Configurar Cemu automáticamente»).

Dónde busca PepoMote la configuración (las mismas reglas que Cemu):

| Instalación | Carpeta |
|---|---|
| Normal (Windows, Cemu 2.0-89 o más nuevo) | `%APPDATA%\Cemu` (se crea si Cemu aún no se ha abierto nunca) |
| Portable (carpeta `portable\` junto a `Cemu.exe` / al AppImage) | esa carpeta `portable\` |
| Antigua (Windows, `settings.xml` junto a `Cemu.exe`) | la carpeta de `Cemu.exe` |
| Linux | `~/.config/Cemu` (o `$XDG_CONFIG_HOME/Cemu`) |
| Linux Flatpak | `~/.var/app/info.cemu.Cemu/config/Cemu` |

Para saber si tienes una instalación portable, PepoMote busca `Cemu.exe` (o el
AppImage) en los sitios habituales (Escritorio, Descargas, Documentos,
`C:\Cemu`, Archivos de programa…), y si Cemu está abierto, aprende su carpeta
y la guarda. Si vive en un sitio raro, escríbela en **Ajustes → Carpeta de
Cemu** o pulsa **Detectar**. Sin rastro de Cemu no se escribe nada.

## Mando Wii dentro de Cemu

Juegos de Wii U que se juegan con el mando de Wii (Wii Sports Club, Wii Party
U, Mario Party 10, Nintendo Land…): en el móvil, dentro del modo Wii U, el
selector **«En Cemu soy: GamePad / Mando de Wii»** (está en la pantalla del
GamePad, bajo la cabecera). Con **Mando de Wii** ese móvil pasa al layout de Wii
de siempre y en Cemu su perfil es un **Wiimote** emulado con MotionPlus; el
puntero se apunta como en modo puntero (recentra con la diana) y llega a Cemu
por el táctil DSU. Un segundo móvil en modo **Nunchuk** se le acopla igual que
en Dolphin (stick, C, Z). El mismo selector, marcando **GamePad**, vuelve al
GamePad / Pro Controller. La elección no se guarda: cada vez que entras en
Wii U eres GamePad.

Como el perfil de Cemu es global (no por juego), cambia de tipo con Cemu
cerrado, antes de arrancar el juego.

## Movimiento apaisado

Sostén el móvil como un GamePad: horizontal, con el borde superior del móvil a
la izquierda (la app Android lo detecta sola por la rotación de la pantalla; en
un móvil Linux se elige con el ajuste **Giro** de la pantalla del GamePad). Los
sensores se remapean para que Cemu reciba exactamente lo que enviaría un mando
tumbado: girar el móvil = girar el GamePad (Splatoon, Zelda, Mario Kart 8…).

## Manual (si prefieres mapear a mano)

Cemu → Opciones → **Configuración de mandos** → Controller 1 → emulated
controller **Wii U GamePad** → Controller API **DSUController** (IP `127.0.0.1`,
puerto `26760`) → Controller 1. Mapeo PepoMote: A = Cross, B = Circle, X =
Square, Y = Triangle, L = L, R = R, ZL = X-Trigger+, ZR = Y-Trigger+, + =
Options, − = Share, cruceta = Up/Down/Left/Right, click de sticks = Stick L /
Stick R, stick izquierdo = X/Y-Axis, stick derecho = X/Y-Rotation, Home =
Touch, soplar = ZL (bit L2), pantalla = ZR (bit R2). Marca **Use motion**.
`assets/cemu/controller0.xml` es exactamente ese perfil, por si quieres
copiarlo a `controllerProfiles/`.

## Problemas típicos

- **Controller 1 aparece desconectado en Cemu**: ¿modo Wii U activo en el
  móvil? ¿«Cemu (Wii U): 1 cliente(s) DSU» en la ventana del PC? El móvil tiene
  que estar conectado (Cemu reintenta solo, sin reiniciar).
- **«Cemu está abierto: ciérralo…»**: PepoMote no escribe con Cemu abierto
  (al salir, Cemu sobreescribiría la configuración). Ciérralo, pulsa
  **Configurar Cemu** y vuelve a abrirlo.
- **«No encuentro Cemu en este equipo»**: pon la carpeta de `Cemu.exe` en
  Ajustes → Carpeta de Cemu, o abre Cemu una vez (se aprende sola).
- **El giroscopio va al revés (móvil Linux)**: cambia el ajuste **Giro** de la
  pantalla del GamePad (izquierda ↔ derecha).
- **El juego no ve el mando tras cambiar GamePad ↔ Mando Wii**: el cambio se
  aplica con Cemu cerrado; reinicia Cemu.
