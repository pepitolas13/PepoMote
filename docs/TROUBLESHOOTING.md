# TROUBLESHOOTING

Se completa con cada hito. Esqueleto:

## El móvil no encuentra el PC

1. Móvil y PC en la misma red (el Wi-Fi de invitados NO vale: aísla clientes).
2. Firewall de Windows: permite PepoMote en redes privadas (pregunta en el primer arranque).
3. Firewall de Linux: muchas distros (CachyOS, Fedora, openSUSE…) traen ufw o
   firewalld activados y descartan TODO lo entrante — el receptor arranca
   perfecto pero ningún paquete del móvil llega. El receptor lo detecta solo:
   en el primer arranque pide tu contraseña en el diálogo del sistema y abre
   el puerto él mismo; si cancelaste, queda el botón «Reparar ahora» en la
   ventana. `packaging/linux/install.sh` hace lo mismo por script. A mano:
   - ufw: `sudo ufw allow 26761/tcp && sudo ufw allow 26761/udp`
   - firewalld: `sudo firewall-cmd --permanent --add-port=26761/tcp --add-port=26761/udp && sudo firewall-cmd --reload`
   Si el diálogo de contraseña no aparece (sesión sin agente de polkit), usa
   estos comandos: la ventana te lo dirá.
4. Si el mDNS está roto en tu router, PepoMote prueba solo el broadcast; si tampoco, teclea la IP:puerto que muestra el receptor bajo el QR.
5. Último recurso: hotspot del móvil + PC conectado a él. Funciona siempre.

## «Vuelve a escanear el QR» al darle a Conectar

El PC ya no reconoce el emparejamiento que guarda el móvil. El secreto del QR
vive en `token.txt`, en la carpeta de configuración del receptor (Windows:
`%APPDATA%\pepotech\PepoMote\config`; Linux: `~/.config/pepomote`), y ha
cambiado: PepoMote reinstalado o restablecido, carpeta borrada, o el receptor
lanzado con otra carpeta de configuración (`PEPOMOTE_CONFIG_DIR`). No hay nada
que reparar: escanea el QR otra vez.

- Android (desde 1.3.1): al fallar, la app abre ella misma la pantalla Conectar
  con la explicación y el botón «Escanear QR del PC»; tras escanear sigue en lo
  que ibas a abrir (Puntero, Dolphin, Wii U o Nunchuk). Antes había que ir a
  Ajustes → «Vincular con otro PC».
- Linux móvil: lo mismo con la pantalla Conectar (elige tu PC y teclea el
  código nuevo de 4 dígitos que hay bajo el QR).
- En el PC, la ventana de PepoMote dice qué móvil ha llegado con un QR antiguo.

## Linux: «Sin permiso para mover el cursor (/dev/uinput)» o «el módulo uinput no está cargado»

Solo pasa donde el cursor va por uinput: GNOME, KDE Plasma y cualquier
sesión X11. En Sway, Hyprland, MangoWC, river, labwc, niri y demás
compositores wlroots el cursor va por el puntero virtual de Wayland, no
necesita ningún permiso y el pie de la ventana dice «Inyección: Wayland
(puntero virtual)»: si ves ese pie, este apartado no es para ti.

1. Pulsa **«Reparar ahora»** en la ventana del receptor (o deja que el
   diálogo del primer arranque haga lo suyo): carga el módulo, instala la
   regla udev y da acceso al instante, sin cerrar sesión (regla `uaccess`:
   acceso para el usuario de la sesión activa, sin grupos ni root).
2. Si no hay botón (no hay `pkexec`) o el diálogo de contraseña nunca
   aparece (sesión sin agente de polkit, típico de un gestor de ventanas «a
   pelo»), la ventana lo dice y enseña el comando manual con un botón
   **«Copiar comando»**. Es este, para pegar en un terminal (pide tu
   contraseña una vez):

   ```
   sudo sh -c 'modprobe uinput; printf "uinput\n" > /etc/modules-load.d/pepomote.conf; printf "KERNEL==\"uinput\", SUBSYSTEM==\"misc\", TAG+=\"uaccess\", OPTIONS+=\"static_node=uinput\"\n" > /etc/udev/rules.d/99-pepomote.rules; udevadm control --reload-rules; udevadm trigger /dev/uinput; setfacl -m "u:${SUDO_UID:-$(id -u)}:rw" /dev/uinput'
   ```

3. «El módulo uinput no está cargado»: el kernel no tiene `/dev/uinput`
   hasta que se carga el módulo; el comando de arriba lo carga ahora y lo
   deja cargado en cada arranque (`/etc/modules-load.d/pepomote.conf`).
4. Si después de todo sigue sin permiso, cierra sesión y vuelve a entrar (la
   regla `uaccess` se aplica al iniciar sesión). `packaging/linux/install.sh`
   hace lo mismo por script.
5. Para forzar un backend: `PEPOMOTE_INJECT=uinput` o `PEPOMOTE_INJECT=wayland`
   al lanzar el receptor.

## El receptor se cierra al conectar el móvil (Linux)

Síntoma (1.3.0 y anteriores): nada más escanear el QR el móvil pierde la
conexión y la ventana del receptor desaparece. La campanita de conexión
abre el audio por ALSA; con el plugin ALSA de PipeWire/PulseAudio la
biblioteca de audio entraba en pánico en su propio hilo y el receptor
estaba compilado para abortar con cualquier pánico.

Arreglado en 1.3.1: el sonido va aislado (si falla, se desactiva solo y
todo lo demás sigue) y un pánico en un hilo secundario ya no cierra el
receptor: queda en el log y en la ventana. Si te pasa algo parecido:

- El log: `~/.config/pepomote/receptor.log` (Windows:
  `%APPDATA%\pepotech\PepoMote\config\receptor.log`). Arranque, sesión,
  inyección elegida, móviles que entran y salen, firewall, puertos y pánicos
  (hilo, mensaje y archivo:línea).
- `./PepoMote-x86_64.AppImage --diag` (Windows: `PepoMote.exe --diag > diag.txt`)
  imprime un informe: sistema, sesión (Wayland/X11, compositor), qué ofrece el
  compositor, si `/dev/uinput` se abre, prueba de audio (con el pánico
  capturado, si lo hay), firewall, puertos y las últimas líneas del log.
  Pégalo en el issue.

## Wayland: qué compositores no necesitan permisos

| Escritorio | Cómo se mueve el cursor | Qué hace falta |
|---|---|---|
| Sway, Hyprland, MangoWC, river, labwc, niri, Wayfire y cualquier compositor que anuncie `zwlr_virtual_pointer_manager_v1` (+ `zwp_virtual_keyboard_manager_v1` para las teclas) | Puntero y teclado virtuales de Wayland | Nada |
| GNOME (Mutter), KDE Plasma (KWin) | `/dev/uinput` | «Reparar ahora» o el comando manual (una vez) |
| Cualquier sesión X11 | `/dev/uinput` | Igual |

El pie de la ventana dice cuál está en uso («Inyección: …») y `--diag` lo
explica. Con el teclado virtual el receptor usa su propio keymap: el texto
del móvil (teclado en pantalla de Cemu) sale igual en cualquier idioma.

## El cursor no va donde apunto / se mueve "acumulando"

Mira en Ajustes de la ventana del PC si "Apuntado absoluto" está desactivado:
en modo relativo el cursor se desplaza con el giro en vez de ir a donde
apuntas (pensado para juegos). Actívalo para el uso normal.

## Varios monitores: a qué pantallas apunta

Por defecto ("Todas las pantallas" en Ajustes), el apuntado absoluto cubre
TODO el escritorio: el cursor llega a cualquiera de tus monitores según hacia
dónde apuntes. Al recentrar, el cursor vuelve al centro del conjunto.

Si juegas en un monitor y quieres apuntado más preciso, en Ajustes →
"Apuntado absoluto en" elige *Solo <ese monitor>*: el rango de giro se dedica
entero a esa pantalla (y el cursor se queda en ella). La app detecta los
monitores sola (se conecten o desconecten en caliente) y usa la disposición
real del escritorio (Wayland xdg-output o X11 xrandr).

Nota: con los monitores en "L" o "T" quedan zonas sin pantalla dentro del
rectángulo que las envuelve; si apuntas a una de esas zonas, el cursor cae en
el borde de la pantalla más próxima (lo coloca el escritorio, no PepoMote).

## En Dolphin el móvil entra como "Jugador 2" y no puedo activar Dolphin

Pasaba al reconectar (Wi-Fi que cae, app al segundo plano) con la sesión
anterior aún "viva" en el PC: el móvil recuperaba plaza como Jugador 2 y el
modo Dolphin solo lo activa el Jugador 1. Ya se arregla solo: al reconectar,
el receptor desaloja la sesión fantasma del mismo móvil y le devuelve su
plaza. Si lo ves en una versión vieja, reinicia el receptor del PC.

## El juego pide un Nunchuk («Conecta un Nunchuk al Mando de Wii del Jugador 1»)

Super Mario Galaxy, Zelda, Metroid Prime y otros exigen el Nunchuk. Con un
solo móvil: enciende **Nunchuk en el mismo móvil** (Ajustes de la app, o el
chip **Nunchuk** de la cabecera del mando en Dolphin; se recuerda): el mando pasa
solo a apaisado, con stick, C y Z en la mano izquierda.
Wiimote emulado se configura con Dolphin cerrado, así que cierra Dolphin,
espera al aviso «Dolphin configurado» del receptor y vuelve a abrirlo; y
comprueba en la ventana del PC que el móvil aparece como «J1 · Mando +
Nunchuk» (con un receptor anterior a 1.5.5 no hay Nunchuk propio). Con un
segundo móvil como Nunchuk, el propio queda sin efecto para ese jugador si
lo apagas. Ver `docs/SETUP-DOLPHIN.md`.

## De lado, la cruceta va al revés, o la app no gira con el móvil

Desde 1.5.5 el mando de lado (NES) manda la cruceta de un mando girado con el
extremo IR a la izquierda, que es lo que esperan los juegos 2D: ▶ mueve a la
derecha. Si te va al revés, actualiza la app. La app solo gira si el giro
automático del sistema está activo: con el bloqueo de giro puesto se queda
como está (quítalo para el mando de lado). Solo el GamePad y el mando +
Nunchuk se ponen solos en apaisado, con o sin bloqueo. En Dolphin, con el
Nunchuk encendido, sale el mando + Nunchuk en vez del NES: apaga el chip
«Nunchuk» para los juegos de lado (y reabre Dolphin).

## «El puerto 26760/26761 está ocupado»

Otro programa tenía abierto el puerto del servidor DSU (26760: DS4Windows,
BetterJoy, otro servidor cemuhook, o un PepoMote antiguo que se quedó
colgado) o el del móvil (26761). Desde 1.3 PepoMote averigua qué proceso lo
tiene, lo cierra y recupera el puerto; la ventana dice a quién ha cerrado
(«Puerto UDP 26760 estaba ocupado por DS4Windows.exe: lo he cerrado…»). Si
no puede (el otro programa va como administrador), te lo dice con su nombre:
ciérralo tú y vuelve a abrir PepoMote. Si el mando iba «a la vez» a otro
servidor DSU, Dolphin puede haber estado hablando con ese otro y no con
PepoMote: al recuperar el puerto, reinicia Dolphin.

## Dolphin enseña el mando desconectado aunque el PC diga «1 cliente(s) DSU»

El DSU llega, pero el Wiimote emulado no está activo en ese Dolphin. Casi
siempre es una de estas: el móvil no está en modo Dolphin; Dolphin estaba
abierto cuando se configuró (ciérralo y ábrelo: desde 1.3 lo pendiente se
escribe solo al cerrarse); es un juego de GameCube; o ese Dolphin es portable
y guarda su configuración junto al exe: compara la carpeta que dice la
ventana de PepoMote con Dolphin → Archivo → Abrir carpeta de usuario, y si no
coinciden, Ajustes → Carpeta de Dolphin → Detectar. Detalle en
`docs/SETUP-DOLPHIN.md`.

## Cemu no ve el mando (Wii U)

Con el modo Wii U activo en el móvil, la ventana del PC debe decir «Cemu
(Wii U): 1 cliente(s) DSU» en cuanto Cemu arranca: si no, Cemu no está
leyendo nuestro perfil. Causas: PepoMote no encontró Cemu («No encuentro Cemu
en este equipo»: Ajustes → Carpeta de Cemu → Detectar, o abre Cemu una vez y
la aprende); Cemu estaba abierto cuando entró el móvil («Cemu está abierto:
ciérralo…»: ciérralo, pulsa Configurar Cemu y vuelve a abrirlo); o el perfil
lo pisó otro programa (mira Opciones → Configuración de mandos: Controller 1
debe ser Wii U GamePad con DSUController «Controller 1»). Detalles en
`docs/SETUP-CEMU.md`.

## «Cemu (Wii U): 0 cliente(s) DSU» con Cemu abierto

Si PepoMote se cerró o se reinició mientras Cemu estaba abierto, Cemu deja de
preguntar por los mandos DSU y no vuelve a intentarlo: cierra Cemu y ábrelo
otra vez (con PepoMote ya en marcha). Lo mismo si ves que el juego no responde
al móvil aunque el PC diga «Modo Wii U».

## El móvil no enseña la pantalla del GamePad (doble pantalla)

El hueco central del GamePad dice el motivo: «Cemu no está abierto» o «Abre la
vista del GamePad en Cemu (Options → Separate GamePad view)». Minimizada no se
puede capturar: PepoMote la restaura al fondo (sin robar el foco) mientras el
móvil pida la pantalla. Que no la veas en el PC es lo normal: con un móvil en
modo Wii U la ventana se queda escondida detrás de la de Cemu (sin botón en la
barra de tareas), porque la pantalla es para el móvil.
PepoMote deja `open_pad = true` en el settings.xml de Cemu al configurarlo, así
que la ventana GamePad View se abre sola al arrancar Cemu; si la cerraste,
vuelve a abrirla desde ese menú. En Linux la captura es X11 (vale XWayland):
con Cemu nativo en Wayland, lánzalo con `GDK_BACKEND=x11`.

## El teclado en pantalla de Cemu no reacciona al táctil

Es cosa de Cemu: su teclado en pantalla (nombre del jugador, mensajes…) solo
acepta teclas del PC, no toques. Pulsa el botón **Teclado** del móvil (modo
Wii U), escribe y **Aceptar**. En Linux, el texto entra por el teclado virtual
de PepoMote: la ventana de Cemu tiene que tener el foco.

## El GamePad gira al revés (Wii U apaisado)

Android detecta solo hacia qué lado giraste el móvil. En un móvil Linux, el
ajuste **Giro** de la pantalla del GamePad (izquierda / derecha) tiene que
coincidir con el lado hacia el que queda el borde superior del móvil.

## Apuntado absoluto o relativo: cuál usar

Los dos mueven el cursor exactamente igual (1:1 con el giroscopio). La
diferencia es qué pasa con la posición:

- **Absoluto** (por defecto): el cursor está donde apunta el móvil, y vuelve al
  mismo sitio si vuelves a apuntar igual (la altura la fija la gravedad; el
  giro horizontal, el recentrado). Ni la aceleración del ratón de Windows ni
  nada del sistema tocan el recorrido. Al pasarte de un borde, el cursor
  responde al instante al volver (como un ratón) y el apuntado recupera su
  sitio con el propio movimiento.
- **Relativo**: como un ratón: solo cuentan los desplazamientos; el sistema le
  aplica su aceleración si la tienes activada, y no hay noción de «dónde
  apunta el móvil». Es el modo para juegos que capturan el ratón.

Con varios monitores, en absoluto, el cursor sale por los bordes que dan a
otro monitor, como haría el ratón.

## El cursor da un tirón al parar, o no va fluido

Desde 1.3.0 el cursor lo mueve el giroscopio, 1:1 con la mano, y el rotation
vector del móvil solo ancla la posición cuando el móvil está quieto, en
silencio (ninguna corrección mueve el cursor). El cursor solo se congela con
la mano quieta de verdad (más de 0,3 s por debajo de 0,25°/s): un movimiento
lento y suave nunca cae en la congelación (antes sí, y el cursor iba a
trompicones). Los bordes de la pantalla se
comportan como con un ratón: si te pasas del borde, al volver el cursor
responde al instante (no hay recorrido invisible que deshacer). Nada se
esconde ni se frena: si tras un flick brusco la mano rebota unos grados hacia
atrás (pasa sin que uno lo note), el cursor lo hace también, porque el cursor
es la mano. Si aun así notas algo raro,
grábalo y mándalo: arranca el receptor con la variable de entorno
`PEPOMOTE_RECORD=C:\ruta\gesto.bin`, repite el gesto, cierra el receptor y
pasa el archivo por `PepoMote --replay gesto.bin` (saca un CSV con lo que
llegó del móvil y lo que salió del motor, paquete a paquete).

En **modo relativo** (Ajustes → apuntado absoluto desactivado) Windows aplica
al cursor su aceleración de ratón: si quieres que el recorrido sea exacto,
desactiva «Mejorar la precisión del puntero» en la configuración del ratón
de Windows. El apuntado absoluto no pasa por esa aceleración.

## El clic del móvil no activa la ventana (Windows)

Un clic inyectado no siempre lleva la ventana a primer plano: Windows solo se
lo concede al proceso que «recibió la última entrada». Desde 1.3.0 el
receptor activa a mano la ventana sobre la que cae el clic (izquierdo o
derecho), como haría el ratón. Si una ventana no responde a nada, suele ser
que está elevada (administrador): Windows no deja inyectarle entrada desde un
programa normal.

## El cursor va a tirones

- HUD del receptor: si el RTT sube de ~15 ms, es la red — pásate a 5 GHz o al hotspot del móvil.
- Frecuencia real del sensor en el HUD: algunos móviles capan a 50-100 Hz; funciona igual, con algo menos de finura.

## Latencia jugando en Dolphin

PepoMote añade lo mismo que en modo puntero (~10-20 ms en LAN 5 GHz). Si notas
retardo, casi siempre viene de la cadena de vídeo, no del mando:

1. **La TV**: activa el "Modo juego" de la tele. Una TV en modo normal mete
   50-100 ms de procesado — es la causa nº 1.
2. **Dolphin**: Gráficos → Avanzado → activa "Present XFB Immediately"
   (Immediately Present XFB) y desactiva V-Sync. Pantalla completa.
3. **Wi-Fi**: HUD del receptor con RTT alto → 5 GHz o hotspot del móvil.
4. Comprueba que el HUD marca ~200-250 paquetes/s durante el juego.

## Linux móvil: "No encuentro giroscopio"

`ls /sys/bus/iio/devices/*/in_anglvel_x_raw` debe listar un archivo. Si no,
el kernel no expone el gyro (falta el driver o el móvil no lo tiene). Si la
cabecera del mando marca 50 Hz, instala la regla udev con
`packaging/linux-mobile/install.sh` para que la app pueda subir la frecuencia.
Más en [MOBILE-LINUX.md](MOBILE-LINUX.md).

## Android mata la conexión al apagar la pantalla

PepoMote usa un servicio en primer plano con wakelock; concédele la exención de optimización de batería cuando la pida. En OEMs agresivos (Xiaomi, Huawei…): dontkillmyapp.com/<tu-marca>.

## Android: la notificación de PepoMote

Con la app en pantalla no hay notificación: el enlace va en un servicio
normal. Al irte a otra app o apagar la pantalla, el servicio sube a primer
plano y aparece la notificación «PepoMote · Conectado a …» (con
«Desconectar»), que es lo que impide que Android duerma la conexión. Desde
Android 13 se puede deslizar (el enlace sigue); en Android 12 o anterior el
sistema no deja quitar la de un servicio en primer plano, y se va sola al
volver a la app. Cerrar la app (Atrás desde el inicio, o deslizarla de
recientes) desconecta y la quita.

## El modo cambia solo al abrir o cerrar Dolphin o Cemu

Desde 1.4 el receptor vigila los emuladores: abrir Dolphin pone el modo
Dolphin, abrir Cemu el modo Wii U, y cerrar el que manda vuelve al puntero (o
al otro emulador si sigue abierto). Los móviles cambian de pantalla solos y
ven un aviso. Lo que elijas a mano en el móvil se respeta hasta la siguiente
vez que abras o cierres un emulador. Si prefieres el modo fijo: Ajustes del
receptor → «Cambiar de modo al abrir o cerrar Dolphin o Cemu» desactivado.
`--diag` dice si está activo y qué emulador ve abierto.

## Modo precisión (el cursor va al 40 %)

Mantén la tira de la mirilla: borde izquierdo del mando vertical (espejo de
la de scroll) o la píldora «Precisión» del mando apaisado; en el móvil Linux,
la tira izquierda. Sigue activo mientras no levantes el dedo, aunque se
salga de la tira. Al soltar no hay salto: el apuntado queda corrido hasta
que recentres con la diana. En Dolphin y Wii U no hace nada.

## Cruceta y volumen en modo puntero

Cruceta ← y → = atrás y adelante en el navegador (en Windows también en el
Explorador); ↑ y ↓ siguen siendo las flechas del teclado. Mantener − / + o
🔉 / 🔊 sigue bajando o subiendo el volumen: el receptor repite la tecla cada
100 ms a partir de los 350 ms (el sistema no repite las teclas inyectadas).
En Dolphin y Wii U la cruceta va al emulador como siempre.

## Varios PCs

Conectar enseña «Tus PCs»: toca uno para conectar con él; mantén pulsado
para olvidarlo (en el móvil Linux, botón «Olvidar»). Si el PC actual no
responde y hay otro guardado, la app abre Conectar en vez de dar error. Un
token es un PC: si el PC cambia de IP o de nombre, el emparejamiento se
actualiza solo; una IP nueva de un PC nunca pisa la de otro con el mismo
nombre.

## Se ha caído la conexión y la app dice «Reconectando…»

Desde 1.4 la app vuelve sola: reintenta con espera creciente (1, 2, 4, 8,
15 s) durante dos minutos, buscando el PC por si cambió de IP, y al volver
repone el modo (Dolphin, Wii U) y el Mando de Wii. La pantalla del mando se
queda. Si a los dos minutos no ha vuelto, aviso y al inicio. Los rechazos del
PC (QR antiguo, ocupado, versión) no se reintentan.

## Tema oscuro e idioma

Android sigue al tema del sistema (y no vuelve al inicio si cambia en mitad
de una partida). El receptor lo sigue en Windows; en Linux, elígelo en
Ajustes («Tema»), porque el escritorio no lo comunica. El móvil Linux lo
elige en el inicio. Idioma: español o inglés según el sistema (español para
cualquier otro), y se cambia con ES/EN arriba a la derecha (receptor, Android
y móvil Linux). Los avisos que el PC manda a los móviles van en el idioma
del PC; el log y `--diag` siempre en español.

## macOS: «Inyección: ninguna» o el cursor no se mueve

Falta el permiso de **Accesibilidad** (así funciona cualquier app que mueva
el ratón en macOS). La ventana enseña una tarjeta con **Abrir Ajustes →
Accesibilidad**: marca PepoMote en la lista y vuelve; se activa solo, sin
reiniciar, y el pie pasa a «Inyección: CGEvent». Si quitas el permiso, la
tarjeta vuelve. Como cada versión va firmada con el mismo certificado, el
permiso se conserva al actualizar. Más en [MACOS.md](MACOS.md).

## macOS: no llega la doble pantalla de Cemu

La captura de la ventana GamePad View necesita **Grabación de pantalla**
(Ajustes del Sistema → Privacidad y seguridad) y macOS solo lo aplica al
reiniciar la app: la tarjeta de la ventana tiene **Abrir Ajustes** y
**Reiniciar PepoMote**. Además: Options → Separate GamePad view en Cemu, y la
ventana GamePad View **no minimizada** (tapada por otra sí se captura). En
macOS 15 o más, el sistema recuerda cada mes que PepoMote captura la ventana
de Cemu: **Permitir**.

## macOS: «Apple no ha podido verificar "PepoMote"» al abrirlo

PepoMote no está notarizado (cuenta de desarrollador de pago). Una vez por
versión descargada: en macOS 15/26, **Listo** → Ajustes del Sistema →
Privacidad y seguridad → abajo del todo **Abrir igualmente** → **Abrir**; en
macOS 13/14, Control-clic sobre la app → **Abrir**. Por terminal:
`xattr -d com.apple.quarantine /Applications/PepoMote.app`.

## macOS: el móvil no encuentra el Mac

Permiso de **Red local** para PepoMote (macOS 15 o más lo pregunta al
arrancar; si dijiste que no: Ajustes → Privacidad y seguridad → Red local),
misma Wi-Fi, y el firewall del Mac con «Permitir» para PepoMote. El
autodescubrimiento (mDNS) puede no funcionar en algún Mac: el **QR** funciona
igual. Informe: `/Applications/PepoMote.app/Contents/MacOS/PepoMote --diag`.

## Aviso de versión nueva

Desde 1.5.5 el receptor y las apps consultan una vez al día si hay una versión
publicada más reciente y, si la hay, enseñan una tarjeta con el enlace a la
release de GitHub (en el receptor, también en el menú de la bandeja). La
consulta es una sola petición a la página de la última release de GitHub
(`releases/latest`), sin cuerpo ni identificadores: GitHub solo ve una
petición más, como si abrieras la página. Se apaga en Ajustes («Avisar de
versiones nuevas»). Si no sale la tarjeta: hace menos de 24 h de la última
consulta, la ocultaste con «Ocultar» (una versión posterior sí se anuncia)
o no hay salida a Internet (se reintenta en una hora). En iPhone/iPad,
SideStore y AltStore avisan además por su cuenta con la fuente de PepoMote.
