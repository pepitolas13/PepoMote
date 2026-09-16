# Jugar en RetroArch con PepoMote

El modo **RetroArch** convierte el móvil en un mando para RetroArch: un
**RetroPad** apaisado de dos sticks con todos los botones, un **mando de NES**
(el Mando de Wii de lado) para los juegos de dos botones, o una **pistola de
luz** (el Mando de Wii apuntando) para Duck Hunt y compañía. Usa dos cosas que
RetroArch trae de serie: el **mando en red** (Network Gamepad / Remote
RetroPad, UDP 55400 + jugador) para los botones y los sticks, y la **interfaz
de comandos de red** (UDP 55355) para el menú y las teclas rápidas (guardar y
cargar estado, avance rápido, rebobinar…). Sin driver de mando virtual y sin
DSU; funciona igual en Windows, Linux y macOS.

## Primera partida

1. Abre el receptor de PepoMote en el PC y conecta el móvil al mismo PC.
2. **Cierra RetroArch**. En el móvil toca **RetroArch**: se abre el RetroPad.
3. Espera el aviso de que RetroArch está listo. Abre RetroArch y juega.

PepoMote escribe en `retroarch.cfg` las claves que activan el mando en red
(`network_remote_enable`, `network_remote_enable_user_p1`…`p4`,
`network_remote_base_port`) y la interfaz de comandos (`network_cmd_enable`,
`network_cmd_port`), con una copia de seguridad en `retroarch.cfg.pepomote.bak`.
Si RetroArch está abierto, espera a que lo cierres: RetroArch reescribe su
configuración al salir y pisaría el cambio. Solo hace falta una vez: si las
claves ya están, PepoMote no toca nada aunque RetroArch esté abierto.

La ventana del receptor dice en todo momento si RetroArch responde, qué
versión es y qué está corriendo («jugando Super Mario World.sfc (snes9x)»),
y RetroArch enseña «PepoMote: mando del móvil conectado» en su pantalla al
enlazar.

## Los tres mandos

En el móvil, **En RetroArch soy:** elige el mando; se recuerda entre sesiones.

| Mando | Cómo se sostiene | Qué manda |
|---|---|---|
| **RetroPad** | apaisado | A/B/X/Y en las posiciones de la SNES, L/R, L2/R2, L3/R3, cruceta, dos sticks, Select/Start, **Menú** (abre el menú de RetroArch) y **Rápido** (avance rápido mientras se mantiene) |
| **Mando NES** | de lado | cruceta, **B** y **A** (los botones grandes), X (la A grande del Mando de Wii), Select/Start y Menú |
| **Pistola** | derecho, apuntando a la pantalla | el puntero mueve el ratón del PC (es la pistola de luz de RetroArch), **B dispara** (clic izquierdo) y **A recarga** fuera de pantalla (clic derecho); 1 y 2 son B y A del RetroPad, y la cruceta, − / + y Menú como en el mando de NES |

La cabecera plegable del móvil trae además las **teclas rápidas**: Guardar y
Cargar estado, Ranura − / +, Rebobinar (se mantiene pulsado), Pausa, Captura
y Reiniciar. Van por la interfaz de comandos de RetroArch, así que no
dependen de qué teclas tengas asignadas.

## Multijugador

Cada móvil es un jugador: el Jugador 1 usa el puerto 55400, el 2 el 55401, y
así hasta cuatro (PepoMote habilita los cuatro usuarios en `retroarch.cfg`).
En RetroArch, Ajustes → Entrada → Mando en red debe quedar activado (lo hace
PepoMote) y el mando de cada jugador aparece en su puerto sin configurar nada
más. Solo el Jugador 1 cambia el modo; el rol Nunchuk no tiene papel en
RetroArch.

## Cómo llega cada pulsación

RetroArch lee **un** datagrama del mando en red por jugador y por fotograma
emulado. Mandar más rápido acumula cola (y retardo); más lento deja
fotogramas sin nada, y en Windows un fotograma vacío puede poner el mando a
cero (es un despiste de RetroArch con `errno`). PepoMote no adivina la
velocidad: mantiene siempre **una sonda** en la interfaz de comandos y
RetroArch la contesta en el mismo sondeo en que lee el mando, así que cada
respuesta es un hueco para un datagrama por jugador, a 50, 60 o 300
fotogramas por segundo. En cada hueco va el flanco de botón más antiguo (una
pulsación breve son dos fotogramas y nunca se pierde), si no el eje que más
se ha movido, y si no hay nada nuevo un refresco de lo que sigue pulsado. Un
botón responde al fotograma siguiente; con los dos sticks moviéndose a la
vez, cada eje se actualiza cada cuatro fotogramas: perfecto para 2D y
suficiente para 3D. El giroscopio del móvil no llega a RetroArch: su mando en
red no lo transporta.

## Pistola de luz

En RetroArch la pistola es el ratón: en el núcleo, Menú rápido → Controles →
Puerto 1, elige el dispositivo de pistola (Zapper, Super Scope, Justifier,
GunCon…). PepoMote mueve el cursor con el puntero del móvil como en el modo
puntero (mantén la diana para recentrar; la tira de precisión ralentiza el
cursor) y B/A son los botones 1 y 2 del ratón, que son los enlaces por
defecto de RetroArch para disparar y recargar. En Linux, con el driver de
entrada `udev` la pistola puede no seguir al cursor: usa `x11` o `sdl2`.
Solo el Jugador 1 apunta (el PC tiene un cursor); un segundo móvil en
Pistola hace de mando de NES.

## Teclado

Toca **Teclado** en la cabecera para escribir en la ventana de RetroArch
(núcleos con teclado: DOSBox, ordenadores…). En Windows y macOS PepoMote
activa la ventana de RetroArch antes de teclear; en Linux déjala en primer
plano. Si el juego usa el teclado, activa el foco de juego en RetroArch
(Bloq Despl por defecto) para que las letras no se interpreten como botones.

## Restaurar retroarch.cfg

En el receptor, **Restaurar mi retroarch.cfg** devuelve las claves de
PepoMote a como estaban en la copia (o las quita si no existían), conserva
todo lo demás y desactiva **Configurar RetroArch automáticamente** para que
una reconexión no las vuelva a escribir. Puedes volver a activarlo en Ajustes
o usar **Configurar RetroArch** a mano. Con RetroArch abierto, la
restauración queda pendiente hasta que se cierre.

## Localización de retroarch.cfg

| Instalación | Archivo |
|---|---|
| Windows (zip oficial y Steam) | `retroarch.cfg` junto a `retroarch.exe` |
| Windows (instalador) | `%APPDATA%\RetroArch\retroarch.cfg` |
| Linux | `~/.config/retroarch/retroarch.cfg` (o `$XDG_CONFIG_HOME`), Flatpak `~/.var/app/org.libretro.RetroArch/config/retroarch/retroarch.cfg`, Snap `~/snap/retroarch/current/.config/retroarch/retroarch.cfg` |
| macOS | `~/Library/Application Support/RetroArch/config/retroarch.cfg` |

PepoMote escribe en todos los que existan (para que valga con una instalación
nativa y un Flatpak a la vez). En **Ajustes → Carpeta de RetroArch** puedes
indicar la carpeta del ejecutable, la que contiene `retroarch.cfg` o el
propio archivo; **Detectar** busca la versión de Steam, y el receptor aprende
la carpeta al ver RetroArch abierto (Windows).

## RetroArch en Android o en otro PC

El mando en red funciona en cualquier RetroArch con red, Android incluido,
pero PepoMote solo lo activa por sí mismo en el PC donde corre el receptor:
en otro equipo, activa a mano en RetroArch Ajustes → Red → **Mando en red**
(puerto base 55400) y **Comandos de red** (55355), y apunta el receptor a
ese equipo… que hoy no es posible: el receptor manda a `127.0.0.1`. El
servidor Android de PepoMote (dos Android) ofrece Dolphin y Eden; RetroArch
en Android no está incluido de momento.

## Problemas típicos

- **«Esperando a RetroArch»**: RetroArch no responde a los comandos de red.
  Ciérralo, pulsa **Configurar RetroArch** en el receptor y ábrelo de nuevo;
  comprueba en Ajustes → Red que Comandos de red esté activado (puerto 55355).
- **«RetroArch está abierto pero no responde a los comandos de red»**: el
  mando sigue funcionando en modo degradado (solo cambios, sin sincronizar
  con los fotogramas). Activa los comandos de red y reinicia RetroArch.
- **Un botón se queda pulsado un solo fotograma o se suelta solo** con otros
  programas que usan el mando en red: es el despiste de `errno` de RetroArch
  en Windows; PepoMote lo esquiva refrescando el mando cada fotograma. Si te
  pasa con PepoMote, mira que la ventana del receptor diga «responde».
- **El mando no aparece en el juego**: en RetroArch, Ajustes → Entrada →
  Mando en red activado y el jugador 1 (o el que sea) habilitado; Ajustes →
  Entrada → Usuarios máximos ≥ número de móviles. Los mandos en red no salen
  en la lista de mandos detectados: se suman al puerto del jugador.
- **Las teclas rápidas no hacen nada**: RetroArch las aplica al contenido en
  marcha; sin juego cargado, guardar estado no hace nada. Rebobinar necesita
  Ajustes → Fotogramas → Rebobinar activado.
- **Con un mando de verdad enchufado**: los dos controlan el mismo puerto;
  los sticks del móvil, si no están en el centro, mandan sobre los del mando.
- **Se acumula retardo**: no debería (ver «Cómo llega cada pulsación»). Si
  la ventana dice «sondeos/s» muy por debajo de la velocidad del juego,
  RetroArch no está contestando a cada fotograma: reinícialo.
