# TROUBLESHOOTING

Se completa con cada hito. Esqueleto:

## El móvil no encuentra el PC

1. Móvil y PC en la misma red (el Wi-Fi de invitados NO vale: aísla clientes).
2. Firewall de Windows: permite PepoMote en redes privadas (pregunta en el primer arranque).
3. Firewall de Linux: muchas distros (CachyOS, Fedora, openSUSE…) traen ufw o
   firewalld activados y descartan TODO lo entrante — el receptor arranca
   perfecto pero ningún paquete del móvil llega. El receptor lo detecta: la
   tarjeta de arriba de la ventana explica lo que va a pedir y, a los tres
   segundos, el diálogo del sistema pide tu contraseña y abre el puerto (y
   da acceso a uinput, todo de una vez); al acabar dice «Listo: …». Si
   cancelaste, queda el botón «Reparar ahora». Ojo: con ufw el receptor no
   puede leer las reglas sin root, así que dice «no puedo leer sus reglas»
   en vez de «bloqueado» hasta que la reparación abre el puerto o entra un
   móvil (desde 1.6; antes decía «bloqueado» para siempre, aunque la
   reparación hubiera funcionado). `packaging/linux/install.sh` hace lo
   mismo por script. A mano:
   - ufw: `sudo ufw allow 26761/tcp && sudo ufw allow 26761/udp && sudo ufw allow 5353/udp`
   - firewalld: `sudo firewall-cmd --permanent --add-port=26761/tcp --add-port=26761/udp --add-service=mdns && sudo firewall-cmd --reload`
   Si el diálogo de contraseña no aparece (sesión sin agente de polkit), la
   ventana enseña estos comandos con un botón para copiarlos. Más en
   [docs/LINUX.md](LINUX.md).
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

## Linux: la ventana no aparece, o se cierra nada más abrirla

Desde 1.6 el receptor no puede «cerrarse sin decir nada»: todo fallo al
abrir la ventana queda en `~/.config/pepomote/receptor.log` (líneas
«Ventana: …»), y antes de rendirse se relanza solo con render por software
(`LIBGL_ALWAYS_SOFTWARE=1`) y, en Wayland con XWayland, por X11; si nada
funciona, avisa con una notificación de escritorio. Los motivos de siempre:

- **Sin OpenGL usable** (driver roto o a medias tras una actualización,
  máquina virtual sin 3D): el log dice «failed to find a matching
  configuration» o «NoGlutinConfigs». El relanzamiento con software lo
  cubre si Mesa está instalado (`libgl1-mesa-dri` / `mesa-dri-drivers`).
- **Falta una biblioteca**: `PepoMote --diag` → sección «Bibliotecas» dice
  cuál (`libxkbcommon.so.0`, `libEGL.so.1`, `libasound.so.2`…) y qué paquete
  instalar según la distro está en [docs/LINUX.md](LINUX.md).
- **glibc antigua** (Ubuntu 20.04, Debian 11, RHEL 8/9): «GLIBC_2.34 not
  found» al lanzarlo desde un terminal; el binario necesita glibc 2.35 o más
  nueva, y no hay log porque el proceso ni arranca.
- **Otra copia ya abierta**: la nueva le pide a la primera que se muestre y
  se retira (el log dice «Ya hay un PepoMote escuchando»); si la primera no
  contesta, la nueva arranca igual.
- **Cerrar la ventana = salir**: en Linux no hay bandeja; minimízala para
  dejarlo corriendo.

Si sigue sin abrir: `PepoMote --diag` y pega el informe en un issue, junto con
`receptor.log`. `PEPOMOTE_NO_UI_FALLBACK=1` desde un terminal enseña el error
tal cual, sin relanzamientos.

## Linux: el diálogo «Se requiere autenticación para ejecutar /bin/sh como superusuario»

Es PepoMote (pkexec) pidiendo permiso para dos cosas, una sola vez y cada una
por su cuenta: la regla udev y el módulo uinput para mover el cursor, y el
puerto 26761 (y mDNS) en el firewall. La tarjeta de arriba de la ventana lo
explica antes de que salte, cuenta atrás incluida; «Ahora no» lo deja para el
botón «Reparar ahora». Al terminar sale «Listo: cursor y firewall
configurados» (o «Aplicado en parte: …» con lo que falló y por qué). Qué
escribe exactamente, y los comandos manuales para hacerlo sin diálogo, en
[docs/LINUX.md](LINUX.md).

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

Para traer el cursor (o el puntero IR en Dolphin y como Mando de Wii en Cemu)
al centro, mantén la **diana** entre − y +: está en el mando vertical, en el
mando apaisado (desde 1.8.5) y en el mando + Nunchuk.

## Escribo en el PC con el botón «Teclado» y falta algún carácter (Linux)

En Windows y macOS entra cualquier carácter. En Linux depende del escritorio:

- **Sway, Hyprland, river, niri y demás compositores wlroots**: entra todo,
  tildes, ñ y emoji incluidos.
- **X11** (Mint, XFCE, Cinnamon, MATE, i3…): entra todo también.
- **GNOME o KDE en Wayland**: el sistema solo deja escribir lo que esté en tu
  disposición de teclado. PepoMote escribe lo que puede, cambia lo que tiene
  equivalente (`ñ` → `n`) y **te avisa en el móvil** de lo que no pudo
  escribir. No hay forma de arreglarlo sin pedirte permisos de escritorio
  remoto en cada sesión.

Si el aviso dice que el sistema rechazó la escritura, la ventana de delante es
de un programa con permisos de administrador: el receptor no puede escribir
ahí salvo que se ejecute también como administrador.

## Al abrir el teclado el cursor se queda quieto

Es a propósito. Mientras escribes, el móvil se mueve en la mano: si el cursor
siguiera al móvil, se saldría del campo donde acabas de hacer clic (y en los
escritorios donde el foco sigue al ratón, el texto acabaría en otra ventana).
Al cerrar el teclado el puntero sigue justo donde estaba.

## Mi móvil no tiene giroscopio: el cursor solo sube y baja (o no se mueve)

El puntero saca la altura de la gravedad y el giro horizontal del giroscopio.
Girar el móvil sobre la vertical no cambia la gravedad, así que un móvil sin
giroscopio no puede ver ese giro: con solo acelerómetro el cursor sube y baja
pero no va a los lados. Muchos móviles baratos (Moto G04 / G04s y otros con
Unisoc) no llevan giroscopio de verdad y exponen uno «virtual» sacado del
acelerómetro: da un rotation vector y algo de giro vertical, pero nada de
horizontal, y además puede tener deriva, retardo o inconsistencias.

Qué hace PepoMote (con la app y el receptor actualizados):

- Detecta si el giroscopio es real. Si no lo es, la primera vez que entras en
  modo puntero te lo dice y pasa a **apuntar por inclinación**: inclinas el
  móvil a los lados (como un volante: borde derecho hacia abajo = derecha) y
  arriba o abajo. Sale solo de la gravedad, así que no deriva; a cambio lleva
  el ruido de la mano, y el cursor se congela con el móvil quieto para que el
  ratón real siga mandando.
- En **Ajustes → Sensor del puntero** eliges tú: **Giroscopio (recomendado)**
  o **Acelerómetro y derivados**, y ves el nombre del sensor que tiene el
  móvil. Con giroscopio real se usa el giroscopio y no cambia nada.
- La diana recentra, la precisión (tira de la mirilla) y el modo relativo
  funcionan igual; la sensibilidad es la misma de siempre y la ventana del PC
  marca «inclinación» junto al jugador.
- Hace falta el receptor actualizado: uno anterior no entiende la
  inclinación y el cursor sigue solo en vertical. El servidor Android
  (Dolphin/Eden) no tiene puntero: ahí no cambia nada, y el movimiento de
  los mandos emulados sigue necesitando un giroscopio físico.

Si el móvil tiene giroscopio de verdad pero la app dice que no (o al revés),
elige el sensor a mano en Ajustes y cuéntalo en un issue con el nombre del
sensor que enseña.

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

## El mando + Nunchuk (o el GamePad) sale del revés, o se da la vuelta solo

El mando + Nunchuk de Dolphin va fijo en apaisado y, hasta ahora, el sensor de
giro elegía hacia qué lado: en algunos móviles le daba la vuelta (180°) con
muy poco movimiento, o sin motivo. Desde 1.6, la primera vez que sale el mando
la app pregunta «¿El mando está bien así?»: «Darle la vuelta» lo gira 180° y
«Así lo quiero» guarda ese lado para siempre. Para cambiarlo después: Ajustes
→ «Lado del mando + Nunchuk» (Izquierda = el borde de la cámara a la
izquierda; Derecha; o Según el sensor, como antes). El GamePad de Wii U tiene
su propio ajuste, «Lado del GamePad de Wii U», con la misma pregunta la
primera vez: los dos lados se guardan por separado.

## A pantalla completa el mando de verdad no responde, o el táctil no llega

«Pantalla del GamePad a pantalla completa» (desde 1.6) está pensada para
un mando de verdad conectado al PC: el receptor añade el DSU del móvil como
segundo mando dentro de tu `controllerProfiles/controller0.xml` sin pisar tu
mando, y el táctil llega por ese DSU. Si algo no va:

- **Cemu estaba abierto** al configurar: el cambio queda pendiente hasta
  cerrarlo (el móvil lo dice). Cierra Cemu y vuelve a abrirlo.
- **Controller 1 en Cemu no es un GamePad** (aviso «Cemu: el mando 1 no es un
  GamePad»): el táctil de la Wii U solo existe en el GamePad emulado. En Cemu
  → Configuración de mandos → Controller 1 → emulated controller **Wii U
  GamePad**, con tu mando dentro; PepoMote añadirá el DSU del móvil al
  reconfigurar (con Cemu cerrado).
- **Tu mando ha desaparecido de Cemu**: mira `controller0.xml.pepomote.bak`
  en `controllerProfiles`: es tu perfil tal cual, que vuelve solo al irse el
  móvil o al apagar la opción. Si no vuelve, cópialo tú encima.
- **«El receptor no conoce la pantalla completa»**: el PC lleva un PepoMote
  anterior a 1.6, que escribe el perfil de siempre con el móvil como mando;
  actualiza el receptor.
- El móvil sigue en modo Wii U como GamePad: con «Mando de Wii» o como
  Jugador 2 (Pro) la pantalla completa no se aplica (no tienen pantalla).

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

Justo después de una actualización en caliente (o de cerrar PepoMote y volver
a abrirlo enseguida) el puerto del móvil puede seguir un instante en manos del
receptor que acaba de cerrarse. Hasta la 1.13.0 el receptor nuevo se rendía a
la primera y se quedaba sin puntero (el pie de la ventana decía «Inyección:
ninguna») hasta reabrirlo. Ahora espera a que se suelte y, si aun así sigue
ocupado, lo reintenta cada dos segundos con el aviso a la vista; el log
(`receptor.log`) deja dicho a quién esperó y cuánto.

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

Con **varios móviles como Mando de Wii** (Mario Party 10) y ninguno como
GamePad, PepoMote deja igualmente un Wii U GamePad en el Controller 1,
manejado por el teclado del PC (o tu mando real si lo tenías ahí), y pone los
Mandos de Wii en los Controller 2, 3…: Cemu necesita ese GamePad, y con un
dispositivo detrás (uno sin dispositivo cuenta como desconectado), para
arrancar el juego y leer los demás mandos. Las teclas están en
`docs/SETUP-CEMU.md`. Con un receptor anterior a 1.8.5, el Controller 1 se
quedaba como Wiimote y no funcionaba nada.

Si con los mandos ya en su sitio el juego se queda en «añade los mandos como
se indica en rojo» (Mario Party 10), pulsa **Home** en cada móvil: Cemu no da
por emparejado un Wiimote emulado hasta que recibe Home (fallo conocido de
Cemu: [bug 353](https://bugs.cemu.info/issues/353)). Home está entre 1 y 2 en
el mando vertical y junto a A en el
apaisado (desde 1.8.5; en modo puntero no aparece porque el PC no le da uso).

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

**«Cemu está abierto como administrador…» (Windows):** Cemu se abrió como
administrador (algunas guías, incluida la wiki de Cemu, lo aconsejan) y
PepoMote no, y Windows no deja a un programa normal tocar las ventanas de uno
elevado: ni esconder la GamePad View ni escribirle, y en Windows 10 y en
Windows 11 anteriores a 24H2 tampoco capturarla (en 24H2 la imagen llega
igual). Hasta la 1.13.2 el móvil decía entonces «Abre la vista del
GamePad…» aunque estuviera abierta. Cierra Cemu y ábrelo normal. Si se abre
siempre así: Propiedades de `Cemu.exe`, pestaña Compatibilidad, desmarca
«Ejecutar este programa como administrador» (y en su acceso directo,
Propiedades → Opciones avanzadas). Si lo tenías así porque Cemu está en
Archivos de programa, muévelo a otra carpeta: la wiki de Cemu tampoco lo
aconseja ahí.

## El teclado en pantalla de Cemu no reacciona al táctil

Es cosa de Cemu: su teclado en pantalla (nombre del jugador, mensajes…) solo
acepta teclas del PC, no toques. Pulsa el botón **Teclado** del móvil (modo
Wii U), escribe y **Aceptar**. En Linux, el texto entra por el teclado virtual
de PepoMote: la ventana de Cemu tiene que tener el foco. Si el móvil avisa de
que «Cemu está abierto como administrador», Windows no deja que las teclas le
lleguen: ábrelo normal (sección anterior).

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

## Mover el móvil arriba y abajo mueve el cursor en diagonal

Los ejes del puntero están girados. Es distinto de «el cursor deriva»: el
cursor no se escapa a ningún sitio y vuelve siempre donde apuntas, pero
«arriba» ha dejado de ser arriba. Suele aparecer **tras un buen rato jugando
sin parar quieto** (juegos de pistola, sobre todo).

Por qué pasa: el motor proyecta el giro al mundo con un marco propio, y el
roll de ese marco (el giro sobre el eje por el que apuntas) es el único grado
de libertad que no mueve el cursor ni un píxel mientras se tuerce — pero
reparte un cabeceo entre horizontal y vertical, que es justo la diagonal.
Hasta 1.12 ese roll solo se corregía con el móvil quieto, así que barriendo
sin parar crecía sin tope.

**Si te pasa:**

1. Pulsa **recentrar**: desde esta versión endereza también los ejes, no solo
   lleva el cursor al centro.
2. O deja el móvil quieto unos segundos.
3. Si vuelve a pasar, grábalo y mándalo: arranca el receptor con
   `PEPOMOTE_RECORD=C:\ruta\sesion.bin`, juega hasta que se tuerza, cierra el
   receptor y pasa el archivo por `PepoMote --replay sesion.bin`. Al terminar
   imprime una línea con el **roll máximo del marco propio** y cuánto tiempo
   estuvo congelado; con eso se ve al instante si es esto.

## El clic del móvil no activa la ventana (Windows)

Un clic inyectado no siempre lleva la ventana a primer plano: Windows solo se
lo concede al proceso que proveyó la última entrada. Desde 1.3.0 el receptor
activa a mano la ventana sobre la que cae el clic (izquierdo o derecho), como
haría el ratón; desde 1.6 lo hace inyectando antes un movimiento nulo (lo
que da el derecho), sin enganchar la cola de entrada del hilo en primer plano
(podía colgar la telemetría) ni pulsar ALT (activaba barras de menú y dejaba
ALT «pegado» en juegos). `PEPOMOTE_WIN_ACTIVATE=legacy` recupera la vía
antigua si con la nueva algo no se activa. Si una ventana no responde a nada,
suele ser que está elevada (administrador): Windows no deja inyectarle
entrada desde un programa normal.

## Windows: deja de funcionar cuando la ventana del emulador no está seleccionada

Casi siempre es **Dolphin**: ignora el mando (también el DSU del móvil) en
cuanto su ventana pierde el foco, salvo que tenga marcado «Background Input»
en la configuración de mandos. Desde 1.6 PepoMote lo deja activado al
configurar Dolphin (`[Input] BackgroundInput = True` en `Dolphin.ini`): con
Dolphin cerrado, entra en modo Dolphin y vuelve a abrirlo. Si es el propio
receptor el que se para al pasar a segundo plano: desde 1.6 se excluye del
ahorro de energía de Windows 11 (EcoQoS), que frenaba los procesos sin foco;
el log dice «Ahorro de energía de Windows (EcoQoS): desactivado». Y si lo que
no responde es una ventana elevada (administrador), ver el punto anterior.

## El puntero se va solo después de vibrar (o el mando en un juego con MotionPlus)

Síntoma: el juego hace vibrar el móvil y, al parar la vibración, el cursor (o
el Mando de Wii emulado en Wii Sports Resort, Skyward Sword y otros juegos
con MotionPlus) se va solo mientras mueves la mano, hasta que dejas el móvil
quieto un momento. Apareció con la vibración de la 1.12.

Por qué: el motor de vibración sacude el mismo giroscopio que apunta. Un
giroscopio MEMS sacudido lee una oscilación a la frecuencia del motor (150 a
250 Hz) que, muestreada a 200 o 400 Hz, se pliega a cualquier frecuencia
(también a continua) y además rectifica un pequeño sesgo. El receptor aprende
el sesgo del giroscopio con el móvil quieto, y la 1.12 lo aprendía también
con el móvil quieto VIBRANDO: lo que aprendía era el motor. Al parar la
vibración ese sesgo falso se quedaba puesto y el cursor se iba mientras la
mano se movía, hasta el siguiente reposo (ahí se corrige solo en uno o dos
segundos). Dolphin hacía lo mismo por su cuenta con el MotionPlus: su
calibración es la media de 3 s de muestras «estables» (dentro de la zona
muerta de 3°/s), y para ella la vibración es estable.

Desde 1.13: el receptor sabe cuándo tiene encendido el motor de cada móvil
(es él quien manda la vibración) y mientras vibra, y 0,6 s después, no
aprende el sesgo ni deja que un giroscopio sacudido mueva el estado interno
con el móvil quieto. En Dolphin, además, el giroscopio va sin el sesgo que ha
aprendido el receptor y el perfil apaga la calibración de Dolphin
(`IMUGyroscope/Calibration Period = 0`; la zona muerta sigue igual). Lo que
no cambia: mientras el móvil vibra y a la vez lo mueves, el sesgo del motor
se suma al movimiento (ningún receptor puede separarlos), así que si el motor
mete 0,5°/s el cursor se corre unos 30 px por cada segundo de vibración a
1080p; al parar, ya no sigue.

Lo que se midió, por simulación: grabaciones reales de un móvil sin
vibración, con la vibración inyectada en el giroscopio y pasadas por el motor
del puntero de la 1.12 y por el de la 1.13 (Full HD, sensibilidad 35, 55 px
por grado; 5,7 s quieto vibrando y luego 5 s moviéndose):

| Sesgo que mete el motor | 1.12: sesgo falso aprendido | 1.12: cursor de más en los 5 s tras parar | 1.13: sesgo aprendido | 1.13: cursor de más | Dolphin con su calibración: giro de más en 5 s |
|---|---|---|---|---|---|
| 0,5°/s | 0,5°/s | 31 px | ninguno | 6 px | 2° |
| 1°/s | 1°/s | 79 px | ninguno | 2 px | 5° |
| 3°/s | 2,9°/s | 314 px | ninguno | 0 | 14° |

Con la calibración de Dolphin apagada y el sesgo del receptor, que no aprende
vibrando, no queda nada que aprender: el giro de más tras parar es cero por
construcción. Los 6 px de la 1.13 son el sesgo real del sensor de esa
grabación (0,1°/s), que durante la vibración tampoco se aprende y se aprende
en el siguiente reposo sin vibrar.

Cuánto mete el motor de TU móvil no se sabe desde aquí: depende del motor,
de la carcasa y de la mano. Para medirlo: arranca el receptor con
`PEPOMOTE_RECORD=C:\ruta\vibra.bin`, juega a algo que vibre y deja el móvil
quieto en la mano mientras vibra; `PepoMote --replay vibra.bin` saca un CSV
con la columna `vibrando` y los tres ejes del giroscopio: lo que lea el
giroscopio quieto mientras `vibrando` sea 1 es el sesgo del motor. Si el
cursor se sigue yendo con la 1.13, manda esa grabación en una incidencia.

## Windows: la ventana no aparece (solo se ve en el Administrador de tareas o en la bandeja)

Síntoma (hasta la 1.13.2): al abrir PepoMote «se abre un momento y se
cierra»; en el Administrador de tareas sale PepoMote y en la bandeja (junto
al reloj) está su icono, pero la ventana no aparece ni pulsándolo.

Había dos causas, arregladas después de la 1.13.2:

1. **La ventana cerrada con la X no volvía.** La X no cierra PepoMote: lo
   esconde en la bandeja para que el móvil siga conectado. Para enseñarla
   otra vez, PepoMote buscaba su ventana por el título, «PepoMote», y cogía la
   primera de cualquier programa. En Windows 10 el Explorador abierto en una
   carpeta llamada «PepoMote» (donde mucha gente guarda el exe) se titula
   igual, así que el aviso iba a esa ventana (y hasta la desmaximizaba), y ni
   el icono de la bandeja ni volver a abrir el exe la sacaban: la segunda
   copia le pasa el aviso a la primera y se cierra al momento.
   Ahora PepoMote enseña siempre su propia ventana, la trae delante y no toca
   las de otros programas. La primera vez que pulsas la X avisa de que sigue
   en la bandeja (con «Salir del todo» si lo que querías era cerrarlo), y si
   no hay icono en la bandeja la X minimiza.
2. **PCs sin OpenGL 2.0.** PepoMote pintaba solo con OpenGL. Sin el driver de
   la tarjeta gráfica (el «Adaptador de pantalla básico de Microsoft»:
   Windows recién instalado o recortado sin Windows Update, gráficas muy
   antiguas, máquinas virtuales, escritorio remoto) Windows solo trae un
   OpenGL 1.1 que no sirve, y el receptor se cerraba sin decir nada, dejando
   un icono en la bandeja que no abría nada.
   Ahora, si OpenGL no sirve, la ventana se pinta con Direct3D 12 y, sin
   GPU, con WARP (el Direct3D por CPU que trae Windows 10 y 11). OpenGL se
   prueba antes en un proceso aparte que no se ve (la sonda): si falla o no
   contesta en 3 s (un driver que se cuelga), la ventana abre con Direct3D, y
   queda recordado para esa gráfica, así que las siguientes veces no hay
   espera. Si aun así una copia se queda colgada antes de pintar, al volver a
   abrir el exe le deja el sitio a la nueva, que abre con Direct3D. Si nada
   pinta, sale un mensaje con lo que pasa y dónde está el log. Y salir ya no
   deja iconos fantasma en la bandeja.

`receptor.log` (`%APPDATA%\pepotech\PepoMote\config`) dice qué gráfica tiene
el PC («Gráfica: …», con «OpenGL del fabricante: no» si falta el driver), qué
contestó la sonda y con qué pintó («Ventana: primer fotograma pintado
(intento 1 · Direct3D 12 · Microsoft Basic Render Driver (por CPU, WARP))»).
`PepoMote.exe --diag` lo resume en «Gráfica:» y «Ventana en esta gráfica:».
Para forzar uno (soporte): la variable `PEPOMOTE_UI_RENDERER=direct3d` u
`opengl`.

## Windows: la ventana de PepoMote sale negra

Síntoma (1.12.0): la ventana se abre pero está entera en negro, sin ni
siquiera el fondo gris o blanco, y el móvil se queda en «Conectando…». En
`receptor.log` (`%APPDATA%\pepotech\PepoMote\config`) está la línea
«PepoMote 1.12.0 arranca…» pero no «Ventana: primer fotograma pintado».

Por qué: la 1.12 le preguntaba al driver del mando virtual (ViGEmBus) antes
de pintar el primer fotograma, y el móvil, al conectar, esperaba a esa misma
pregunta. Un ViGEmBus a medio instalar, con una petición atascada dentro o
pisado por otro programa de mandos no contesta nunca, y con él se quedaban la
ventana y el `ok` del móvil.

Arreglado en 1.13: al driver se le pregunta en segundo plano y sin esperar.
Si no contesta a los 3 s, la ventana lo dice («El driver del mando virtual
no contesta…», con el botón «Instalar el mando virtual», que
repara la instalación) y todo lo demás funciona —puntero, Dolphin, Cemu,
RetroArch—, sin vibración ni mando universal hasta que el driver responda.
El log lo apunta («Mando virtual: el sondeo del driver no contesta a los
3 s») y, si la ventana tardara en pintar por lo que sea, «Ventana: sin primer
fotograma a los 15 s» con lo que la está frenando.

Si te pasa con la 1.12: Configuración → Aplicaciones → «Nefarius Virtual
Gamepad Emulation Bus» → Modificar (reparar) o Desinstalar, reinicia si lo
pide y vuelve a abrir PepoMote; o instala ViGEmBus 1.22.0 desde su página.
Si sigue negra, manda `receptor.log` en un issue (en 1.12, `PepoMote.exe
--diag` puede colgarse por lo mismo: eso también lo confirma).

### Con la 1.13 o posterior

Un negro puro (ni el fondo gris ni el blanco del tema) es una ventana en la
que no ha llegado a presentarse ningún fotograma, o que se quedó sin
repintar después del primero: Windows enseña la ventana justo antes de
presentar ese primer fotograma, y si el hilo que la pinta se para ahí, o
poco después, queda así. Hasta la 1.13.1 ese hilo todavía esperaba a
`reg.exe` (leer el autoarranque, justo después de pintar el QR), a la
consulta de la IP local y al candado del estado sin tope; y un cuelgue
posterior al primer fotograma no dejaba nada en el log.

Desde la 1.13.2: `reg.exe` y la IP local van en sus hilos, el candado del
estado tiene tope (si otro hilo lo retiene, la ventana repinta con la foto
anterior y lo apunta), y un vigilante deja en `receptor.log` «Ventana: sin
repintar desde hace N s · último paso: …» (a los 10 s, a los 60 s y luego
cada 10 min) y «Ventana: vuelve a repintar» si se recupera. Al abrir el
exe otra vez con la ventana negra, la copia abierta apunta cómo está
(«Otra copia de PepoMote pide mostrar la ventana · último fotograma hace
N s · último paso: …»), y `PepoMote.exe --diag` se lo pregunta por el
cerrojo de instancia única y lo imprime en «Receptor abierto:», aunque su
ventana esté colgada.

Si te pasa: con la ventana negra a la vista, `PepoMote.exe --diag > diag.txt`
desde una consola en la carpeta del exe, y manda `diag.txt` y
`receptor.log` (`%APPDATA%\pepotech\PepoMote\config`) en un issue, con
la versión de Windows y la tarjeta gráfica, y si el móvil conecta y mueve
el puntero aunque la ventana esté negra (entonces es solo la ventana; el
receptor sigue vivo). Para cerrar PepoMote no hace falta el Administrador
de tareas: cerrar la ventana la esconde en la bandeja; «Salir» está en el
menú del icono.

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

## Se me suelta el botón al mover el dedo (o quiero pulsar deslizando)

En Ajustes, en Android, iPhone/iPad y Linux móvil, hay dos opciones para
esto:

- **Mantener al salir del botón**: un botón pulsado sigue pulsado mientras
  no levantes el dedo, aunque el pulgar se salga de él. Viene **encendido**
  de serie en las tres apps. En Android antes se soltaba al salirse (era lo
  que hacía que el 2 se fuera solo en Mario Kart en cuanto el pulgar se
  corría); en iPhone y en el móvil Linux ya funcionaba así.
- **Pulsar deslizando**: el botón por el que pasa el dedo se pulsa y el
  anterior se suelta, y un dedo que nace en el hueco entre dos botones pulsa
  el primero al que llega. **Apagado** de serie.

Las dos no se combinan: con «Pulsar deslizando» encendido, «Mantener al
salir del botón» queda atenuado y manda el deslizar. Al apagarlo vuelve a
valer lo que tuvieras en el otro.

La cruceta, los sticks, la tira de scroll, la pantalla táctil del GamePad,
los chips de la cabecera, el teclado y la diana de recentrado no entran en
esto aposta: la cruceta ya desliza de una dirección a otra por su cuenta, y
un stick o un táctil que se engancharan al pasar el dedo por encima darían
más problemas que ventajas. Las tiras de **Precisión** y **Acercar** se
mantienen siempre aunque el dedo se salga: son «mantener», no botones.

Las tarjetas que se ponen encima del mando (la cabecera desplegada, la
pregunta del lado la primera vez y los avisos) se quedan el toque: tocar su
fondo no pulsa el botón que tapan.

Un botón que ya está apretando un dedo no se lo lleva otro: el segundo que
pase por encima no lo vuelve a pulsar y, al levantarlo, el botón sigue
pulsado mientras el primero no lo suelte.

En **iPhone y iPad**, «Pulsar deslizando» solo lleva **un dedo** desde el
hueco vacío: si nace un segundo dedo fuera de todos los botones, ese no
pulsa nada (los dedos que nacen sobre un botón sí van cada uno por su lado,
que es el caso normal). En Android y en el móvil Linux van todos.

En un barrido muy rápido puedes ver dos botones pulsados a la vez durante
unas decenas de milisegundos: la app retiene cada pulsación 70 ms para que
no se pierda si la Wi-Fi se traga un paquete, así que el que sueltas y el
que pulsas se solapan ese rato. Es a propósito: sin ese margen, un toque
muy rápido puede no llegar al emulador.

De paso, en **Ajustes → Sensor del puntero** la elección entre Giroscopio y
Acelerómetro se aplica al momento; antes había que salir y volver a entrar
en Ajustes para ver el cambio.

**¿Y «Salir»?** En **Android** y en **iPhone/iPad**, en los mandos apaisados
(el de lado, el mando + Nunchuk y el mando de Wii U / Switch) la cabecera es
una pastilla con el modo, arriba en el centro. Tócala y baja la tarjeta, que
se pliega sola a los 4 segundos sin tocarla: en los mandos de Wii lleva el
nombre del PC, los chips de modo, el chip **Nunchuk**, **Teclado** y
**Salir**; en el mando de Wii U / Switch lleva el nombre del PC, el jugador y
el pad (con los ms y, si la tienes, los fps de la pantalla), los chips de modo
y **Salir**, y el botón **Teclado** se queda justo al lado de la pastilla,
siempre a la vista para tenerlo a un toque mientras juegas. En los mandos
verticales los botones siguen a la vista como siempre, y en **Linux móvil** la
cabecera sigue como estaba, con sus chips arriba a la derecha.

## Cruceta y volumen en modo puntero

Cruceta ← y → = atrás y adelante en el navegador (en Windows también en el
Explorador); ↑ y ↓ siguen siendo las flechas del teclado. Mantener − / + o
🔉 / 🔊 sigue bajando o subiendo el volumen: el receptor repite la tecla cada
100 ms a partir de los 350 ms (el sistema no repite las teclas inyectadas).
En Dolphin y Wii U la cruceta va al emulador como siempre. La fila
**Multimedia** del mando vertical solo sale en modo puntero, porque en los
demás modos el PC no atiende esas teclas (los paquetes van al emulador); si
la quieres siempre, Ajustes → «Multimedia en todos los modos» (Android, iOS
y móvil Linux; solo cambia lo que se ve, no lo que el PC hace).

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

El nuevo actualizador consulta las versiones publicadas, muestra las novedades
y ofrece instalar desde la app. Desde Ajustes puedes comprobar otra vez o
recuperar un aviso pospuesto. Si no hay conexión o el paquete está incompleto,
puedes reintentar; no se sustituye la app con una descarga sin verificar.

Android requiere confirmar en el instalador del sistema. iPhone/iPad continúa
en SideStore o AltStore; Sideloadly requiere el ordenador. Consulta
[Actualizaciones y recuperación](UPDATES.md) para los pasos por plataforma.
Las versiones antiguas solo muestran un enlace a GitHub y requieren una primera
instalación de la versión que incorpora el actualizador.

## Switch / Eden

- **No aparece Switch o se avisa de receptor antiguo:** usa el APK y receptor de la misma compilación; cierra el receptor anterior antes de abrir el nuevo.
- **El mando no funciona tras cambiar el número de jugadores:** cierra Eden, espera a que PepoMote confirme la configuración y vuelve a abrirlo. Revisa Ajustes → Carpeta de Eden si tienes varias instalaciones.
- **Otro servidor DSU ya estaba configurado:** su orden se conserva; PepoMote puede usar UDP Controller 4, 8 u otro número. No cambies manualmente ese número al slot físico del móvil.
- **Tenía un Joy-Con elegido:** esta versión usa solo Pro Controller. Las preferencias anteriores se convierten automáticamente; conecta todos los móviles con Eden cerrado para actualizar sus mandos.
- **Mi móvil no tiene giroscopio:** los botones y sticks funcionan igualmente. Actualiza el APK y el receptor de esta prueba; el movimiento de giro requiere un giroscopio físico (el puntero del PC sí puede ir por inclinación: ver «Mi móvil no tiene giroscopio» más arriba).
- **Al cerrar el emulador cambia de modo:** desmarca Ajustes del receptor → Volver al puntero al cerrar un emulador. Desactivado conserva el modo, también en Dolphin y Wii U.
- **Quiero recuperar mis mandos:** Restaurar mis mandos de Eden. La restauración apaga la autoconfiguración de Switch; vuelve a activarla si después quieres usar PepoMote automáticamente.
- **No entra el texto:** abre el teclado del juego dentro de Eden. En Linux deja Eden en primer plano; Windows/macOS necesitan poder activar su ventana.
- **Hay un perfil específico de juego:** la integración escribe el perfil global. Si ese juego fuerza otro perfil, selecciona la configuración global de mandos en Eden.

Guía completa: [SETUP-SWITCH.md](SETUP-SWITCH.md).

## RetroArch

- **No aparece RetroArch en el móvil:** tanto el mando como el receptor (PC o servidor Android) deben incluir esta integración. El receptor lo anuncia como `retroarch` en sus modos; los servidores Android anteriores no lo ofrecen.
- **Servidor Android: «Esperando a que RetroArch responda»:** RetroArch solo atiende la red mientras está delante. Activa Comandos de red y RetroPad de red en Ajustes → Red, con «Mostrar ajustes avanzados» activado, y guarda en Menú principal → Archivo de configuración → Guardar configuración actual. Sal de RetroArch y vuelve a abrirlo. Android no deja que PepoMote toque `Android/data`. Ver [ANDROID-SERVER.md](ANDROID-SERVER.md#retroarch).
- **Servidor Android: conexión comprobada pero los botones no funcionan:** la respuesta comprueba los comandos de red; activa también RetroPad de red y RetroPad de red del usuario 1 (y los demás jugadores) en RetroArch. Puedes abrir «Ver la guía otra vez» desde el panel.
- **Servidor Android: el móvil no cambia al mando de la consola:** enlaza la carpeta RetroArch (Servidor → RetroArch → Elegir la carpeta RetroArch). Si RetroArch no tiene acceso a todos los archivos, guarda su historial en `Android/data`, que no se puede leer: elige la consola a mano en el móvil.
- **Servidor Android: no hay Pistola:** el servidor Android no tiene ratón que mover; el selector del móvil no la ofrece y el servidor, si se la piden, mantiene el mando anterior.
- **«Esperando a RetroArch» en el receptor:** RetroArch no contesta a los comandos de red (UDP 55355). Ciérralo, pulsa **Configurar RetroArch** y ábrelo; en RetroArch, Ajustes → Red → Comandos de red debe estar activado, y Ajustes → Entrada → Mando en red también (PepoMote pone ambos en `retroarch.cfg`, pero solo con RetroArch cerrado, porque lo reescribe al salir).
- **«Abierto pero no responde a los comandos de red»:** el mando funciona en modo degradado (sin sincronizar con los fotogramas). Activa los comandos de red y reinicia RetroArch.
- **RetroArch se cierra solo al poco de enlazar:** en la 1.22.2 y anteriores el comando de red `GET_STATUS` cierra RetroArch si el núcleo cargado no está en su lista de información (fallo de RetroArch corregido en enero de 2026). PepoMote solo pregunta el estado a versiones posteriores; si otro programa usa la interfaz de comandos, es él.
- **«Responde» pero el juego no se mueve:** RetroArch pausa el contenido cuando su ventana no está en primer plano (Ajustes → Interfaz → «Pausar el contenido cuando no está activo», `pause_nonactive`); haz clic en su ventana o desactívalo. Menú, capturas y estados funcionan igual con la ventana detrás.
- **El móvil no cambia al mando de la consola del juego:** el receptor lee el historial de RetroArch (`content_history.lpl`, activado de serie) y la ficha `.info` del núcleo; sin historial, sin ficha y con un nombre de núcleo raro, o con una extensión ambigua en un núcleo multisistema, se queda en el RetroPad completo: elige la consola a mano en el selector del móvil (se recuerda para ese juego). Con RetroArch en el menú sigue el último juego cargado.
- **Las etiquetas del mando no cuadran con el juego:** son los mapeos por defecto de cada núcleo; con controles remapeados en RetroArch o el dispositivo «Modern» de FBNeo, elige **RetroPad completo**.
- **El mando no aparece en el juego:** los mandos en red no salen en la lista de dispositivos; se suman al puerto del jugador (Jugador 1 = usuario 1, puerto 55400). Ajustes → Entrada → Usuarios máximos debe ser ≥ el número de móviles.
- **Un botón se suelta solo o dura un fotograma:** RetroArch en Windows vacía el mando en red en los fotogramas sin datagrama; PepoMote lo esquiva refrescando cada fotograma. Si pasa, la ventana del receptor debe decir «responde»; si dice degradado, ver arriba.
- **La pistola no apunta:** en el núcleo, Menú rápido → Controles → Puerto 1 → dispositivo de pistola (Zapper, Super Scope, GunCon…); PepoMote mueve el cursor del PC y B/A son los botones 1 y 2 del ratón. En Linux usa el driver de entrada `x11` o `sdl2` si con `udev` no sigue al cursor. Solo apunta el Jugador 1.
- **No entra el texto:** pon la ventana de RetroArch en primer plano (Windows/macOS la activan solos) y, si el juego usa teclado, el foco de juego (Bloq Despl).
- **Quiero mi retroarch.cfg de antes:** Restaurar mi retroarch.cfg en el receptor (solo las claves de PepoMote; apaga la autoconfiguración de RetroArch).

Guía completa: [SETUP-RETROARCH.md](SETUP-RETROARCH.md).

## Vibración en los juegos

- **El móvil no vibra con el juego:** mira la tarjeta «Vibración en los juegos» de Ajustes: ahí abajo dice lo que puede hacer el PC conectado. En Windows hace falta el mando virtual, que PepoMote instala él mismo la primera vez que abres su ventana (pide permiso de administrador una vez): si lo cancelaste, en la ventana del receptor está el botón «Instalar el mando virtual». En Linux es el permiso de `/dev/uinput`.
- **Windows: nunca vi el permiso de administrador:** hasta la 1.13.2 el receptor lo pedía a los pocos milisegundos de arrancar, sin su ventana, y Windows lo dejaba minimizado, parpadeando en la barra de tareas (y al arrancar con Windows, ni eso). Ahora lo pide con la ventana del receptor delante, y arriba, bajo el título, sale «Instalando el mando virtual…» mientras tanto. Si no llegaste a aceptarlo, PepoMote no vuelve a preguntar solo: pulsa «Instalar el mando virtual» en la ventana del receptor (en las tarjetas de Dolphin, Cemu o mando universal). `receptor.log` dice cuándo se pidió y, si no se pide, por qué.
- **Vibra el móvil de otro jugador:** actualiza el receptor. Hasta la 1.12 el PC escribía el mismo número de mando en los perfiles de todos los jugadores, así que la vibración de todos acababa en el Jugador 1.
- **Vibra demasiado o demasiado poco:** Ajustes → Vibración en los juegos → Alta / Normal / Baja. El botón «Probar» te deja oírlo sin abrir un juego.
- **No vibra nada y el móvil sí tiene motor:** algunos móviles apagan la vibración con el modo de ahorro de energía o con «No molestar». Compruébalo en los ajustes del sistema.
- **Un iPad no vibra:** no tiene motor. Ajustes lo dice.
- **El móvil se queda vibrando cuando el juego ya ha parado:** actualiza el receptor del PC (y la app del móvil). El móvil vibra exactamente mientras el receptor le dice que el motor sigue encendido, y hasta la 1.13.1 el receptor de Windows podía perderse el último «apaga»: Dolphin lo escribe en dos llamadas seguidas (motor L y luego R) y el receptor dejaba una sola petición de aviso esperando en el driver ViGEmBus. Con un ViGEmBus anterior a 1.17.333 (el que traen BetterJoy o x360ce) el segundo aviso se perdía siempre y el móvil no paraba desde la primera vibración; con los nuevos, de tarde en tarde. Desde la 1.13.2 el receptor deja varias peticiones esperando y no pierde ninguno, y `receptor.log` dice qué ViGEmBus tienes (línea «Mando virtual: ViGEmBus …»). En el móvil, desde la 1.13 ninguna orden al motor es infinita, y Apagada en Ajustes, irse a otra app o desconectar paran en el acto.
- **El puntero (o el mando emulado) se va solo al parar de vibrar:** ver «El puntero se va solo después de vibrar», más arriba: desde 1.13 el receptor no aprende el sesgo del giroscopio mientras el motor está encendido, ni Dolphin calibra por su cuenta.
- **Sigue vibrando con el juego en pausa o cerrado:** si pausas Dolphin, cierras el juego o el emulador se cuelga mientras pide vibración, el mando virtual del PC se queda con la última orden (igual que un mando de Xbox de verdad tras un cierre brusco; Dolphin no apaga el motor del Mando de Wii al pausar). Desde la 1.13.2 el receptor de Windows da por acabado un motor que el emulador no toca en 10 s, la misma cota que Dolphin pone a sus mandos SDL y evdev, y lo apunta en `receptor.log`. En Linux no hace falta: allí los efectos de vibración traen su duración y se borran al cerrarse el emulador.

## Mando universal

- **No aparece «Mando universal» en el móvil:** el PC tiene que anunciarlo (`gamepad` entre sus modos). Actualiza PepoMote en el PC. No sale con servidor Android ni en macOS: allí no se puede crear un mando virtual.
- **Windows: «Sin el mando virtual no hay mando»:** el driver del mando virtual (ViGEmBus) va dentro de PepoMote y se instala solo la primera vez que se abre la ventana del receptor, con un permiso de administrador de Windows. Si la cancelaste, pulsa «Instalar el mando virtual» en la ventana del receptor; PepoMote lo detecta al momento, sin reiniciar. Es el mismo driver que usa la vibración de los juegos. Para quitarlo: Configuración → Aplicaciones → «Nefarius Virtual Gamepad Emulation Bus».
- **Linux: no hay mando:** es el permiso de `/dev/uinput`, el mismo que la vibración; la reparación está en la propia ventana del receptor.
- **Windows Server: el mando virtual «no contesta» o PepoMote no termina de cerrarse:** ViGEmBus crea un mando de Xbox 360 y necesita el driver de clase de Windows (`xusb22`), que las ediciones Server no traen. Sin él el mando nunca está listo, la ventana lo dice y el puntero y los emuladores siguen funcionando, pero al cerrar el receptor puede quedarse un proceso «terminando» hasta reiniciar. Instala el driver de mando de Xbox 360 de Microsoft o usa PepoMote sin mando universal ni vibración.
- **Los botones salen cambiados en Steam:** Steam tiene su propia capa de mando (Steam Input) y puede remapear cualquier mando a su manera, no solo este. En Steam → Ajustes → Mando, desactiva la compatibilidad para ese juego, o configúralo allí como harías con un mando de Xbox de verdad.
- **Escribir o buscar dentro del juego:** toca **Teclado** en la cabecera del mando. «Enviar» pega el texto en la ventana que tenga el foco en el PC y «**Enviar + ⏎**» lo manda y pulsa Intro, que es lo que abre la búsqueda. Mientras el teclado está abierto el giro se para, para que la cámara no se vaya sola al mover el móvil escribiendo.
- **El juego hace cosas raras al mover el móvil:** es el apuntado por giro, que mueve el stick derecho (en The Binding of Isaac, por ejemplo, disparas hacia donde muevas el mando). El móvil lo pregunta la primera vez que conectas en mando universal; para cambiarlo, Ajustes → «Mover el móvil apunta».
- **Mover el móvil ya no apunta:** ese ajuste está apagado. En Ajustes → «Mover el móvil apunta» se vuelve a encender; vale al momento, sin reconectar. Solo afecta al mando universal: el puntero, Cemu, Dolphin, Switch y RetroArch no lo miran.
- **La cámara gira sola:** deja el móvil quieto un momento. Si sigue, tu giroscopio deriva más de lo normal: usa el stick derecho de la pantalla, que manda sobre el giro mientras lo tocas, o apaga «Mover el móvil apunta» en Ajustes.
- **Se ha quedado un botón pulsado:** no debería: el receptor suelta el mando si el móvil deja de mandar durante un segundo. Si pasa, mira `receptor.log` y ábrelo como incidencia.
- **El juego no ve el mando:** ábrelo después de conectar el móvil. Muchos juegos solo miran los mandos al arrancar.
