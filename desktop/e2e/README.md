# e2e del receptor (Windows/Linux, en el propio equipo)

Pruebas de extremo a extremo contra un receptor REAL: TCP/UDP del protocolo
PMP, cliente DSU como el de Dolphin/Cemu/Eden, y los archivos de configuración que
escribe. Necesitan Python 3 (sin dependencias).

En Windows, `python desktop/e2e/run_switch_preview.py desktop/target/release/PepoMote.exe`
ejecuta las cinco suites aisladas: RetroArch, Eden, Cemu, Nunchuk y puntero IR
de Wii. Es la misma selección que usa la CI de Windows y la preparación de
la release. Son comprobaciones con clientes/emuladores simulados por sockets;
no sustituyen probar los juegos en cada dispositivo.

Arranca un receptor de pruebas AISLADO (no toca tu configuración, ni Dolphin,
ni Cemu ni Eden, y convive con el receptor normal):

```
APPDATA=<dir>\appdata            (Cemu roaming aislado; crea <dir>\appdata\Cemu vacío)
PEPOMOTE_CONFIG_DIR=<dir>\config  (settings.json y token.txt propios)
PEPOMOTE_DOLPHIN_DIR=<dir>\dolphin
PEPOMOTE_CEMU_DIR=<dir>\appdata\Cemu
PEPOMOTE_EDEN_DIR=<dir>\eden
PEPOMOTE_RETROARCH_DIR=<dir>\retroarch
PEPOMOTE_PORT=26771 PEPOMOTE_DSU_PORT=26770
PEPOMOTE_RETROARCH_PORT=26773 PEPOMOTE_RETROARCH_CMD_PORT=26778
PEPOMOTE_PAIR_CODE=1234 PEPOMOTE_ASSUME_EMULATOR_CLOSED=1 PEPOMOTE_NO_TRAY=1
PepoMote.exe
```

y con las mismas `PEPOMOTE_PORT`, `PEPOMOTE_DSU_PORT` y `PEPOMOTE_DOLPHIN_DIR`
en el entorno:

- `python e2e_cemu.py <dir>\appdata` — modo Wii U: `ok.modes`/`ok.pad`, perfiles
  de Cemu (GamePad, Pro, Mando de Wii + Nunchuk, backup/restauración), PadData
  Wii U (botones, gatillos, sticks, táctil, Home→Touch, puntero IR), avisos
  `pad`/`notice`, difusión de `mode`, «solo pantalla» (fusión del DSU del
  móvil con un mando ajeno en `controller0.xml`, copia y restauración, eco de
  `screen_only`, táctil, aviso si el mando 1 no es un GamePad), regresión de
  Dolphin.
- `python e2e_nunchuk.py` — Nunchuk en modo Dolphin: slots, emparejamiento,
  stick/C/Z en su pad DSU.
- `python e2e_retroarch.py` — modo RetroArch contra un RetroArch FALSO (el
  propio script escucha en el puerto base del mando en red y en el de
  comandos, consume un datagrama por jugador y fotograma a 60 fps como
  `input_driver.c` y contesta `VERSION` y `GET_STATUS`): `retroarch.cfg`
  escrito con copia byte a byte e idempotente, vocabulario `pad`
  (retropad/nes/gun), sondas y `SHOW_MSG`, botón al fotograma siguiente y
  latch, sin cola (nunca dos datagramas esperando), sticks (escala y signo),
  los 16 botones del RetroPad, Home → `MENU_TOGGLE`, Capturar →
  `FAST_FORWARD_HOLD` cada fotograma, `hotkey` (un toque y mantener),
  refresco que sobrevive al vaciado por fotograma de Windows, segundo
  jugador en su puerto con mando de NES, soltar al irse el móvil o al cambiar
  de modo, resincronización tras un RetroArch mudo, `GET_STATUS` nunca a
  una 1.22.2 (la cierra si el núcleo no está en su lista de información) y
  sí a una 1.23.0, solo después de su `VERSION`; y la detección del juego
  cargado por `content_history.lpl` + `info/*.info` (`game` justo tras `ok`
  y al cambiar; Mega Drive, NES, extensión sobre núcleo multisistema,
  miembro `#` de un zip, núcleo sin ficha por nombre, desconocido → `null`,
  JSON a medias, misma entrada sin re-anuncio, eco de `pad.layout`).
- `python real_retroarch.py <PepoMote.exe> <carpeta RetroArch> --wipe --core <núcleo 2048> [--game <rom> --game-core <núcleo> --console <id>]`
  — a mano, fuera de la CI: lo mismo contra un RetroArch DE VERDAD (una copia
  del zip oficial, nunca tu instalación: borra su `retroarch.cfg`, saves,
  states y capturas) con el núcleo 2048 fuera de su carpeta: cfg y copia,
  sondas, vivo 5 s sin `GET_STATUS`, Start y cruceta llegan al núcleo
  (capturas del tablero), Home abre el menú, `SAVE_STATE`, avance rápido y
  modo automático al reabrirlo; con `--game`, además, carga un juego de
  verdad y comprueba que el receptor lo anuncia desde el historial con su
  consola (Cave Story en Genesis Plus GX → `md`). Necesita Pillow para
  `--core`. Comprobado con la 1.22.2.
- `python e2e_dolphin_ir.py` — puntero IR del perfil Wii (Dolphin ≥ 2407,
  `protocol/DSU.md`): el sentido de los ejes cruzado con el Mando Wii de
  Cemu (mismo motor), yaw/pitch/roll con giroscopio y quaternion coherentes,
  «acercar» (bit 30) en rampa, fuera de cámara (centinela), Nunchuk propio
  sin roll y el `WiimoteNew.ini` escrito (IRPassthrough + IMUIR).
- `python e2e_gamepad.py <APPDATA aislado>` — mando universal: el móvil
  simulado contra el **mando de Xbox 360 virtual leído desde fuera** con la
  API XInput de Windows, que es lo que ve un juego de verdad. Comprueba que
  el modo crea un mando nuevo, cada botón en su bit (Home → Guide por el
  ordinal 100, que es el único que lo trae), los gatillos analógicos, el
  signo y el recorrido de los dos sticks, el giro en el stick derecho y el
  dedo ganándole, que un móvil sin giroscopio no apunta, que callarse suelta
  el botón, que no se configura ningún emulador y que al irse el móvil el
  mando se desenchufa. **Necesita el driver del mando virtual (ViGEmBus)**,
  que va dentro del exe: la CI lo instala antes con
  `PepoMote.exe --install-driver` lo instala en un PC; en la CI NO se instala:
  el Windows Server de GitHub no trae el driver de clase del mando de Xbox 360
  (`xusb22`), un mando de ViGEm nunca está listo allí y el receptor se queda
  «terminando» (medido: el paso del e2e colgado media hora), así que ahí se
  salta sola y sale con 0. Con `PEPOMOTE_E2E_REQUIRE_GAMEPAD=1` saltarse
  cuenta como fallo (para un PC con el driver, donde se ejecuta de verdad). Los
  receptores aislados arrancan con `PEPOMOTE_NO_DRIVER_SETUP=1` para que
  ninguna prueba abra la ventana de permiso de Windows.
  **Tres trampas de XInput aprendidas aquí**, por si alguien las vuelve a
  pisar (ninguna es culpa del receptor: su log sale impecable en los tres
  casos):
  1. No refleja el último `update` al instante — hay que leer con una pausa
     entre el envío y la lectura, no pegado al `sendto`.
  2. No ve las *retiradas* de mandos sin un bucle de mensajes de Windows;
     las altas sí. Para comprobar que un mando se fue hay que preguntarlo
     desde un proceso nuevo (`connected_fresh`).
  3. **Pulsar Guide deja de ver el mando en ese proceso.** Es el botón Xbox
     y Windows lo captura para la barra de juego; a partir de ahí todas las
     lecturas salen a cero mientras el receptor sigue escribiendo tan
     ricamente. Por eso esa comprobación va la última de las que leen el
     mando. En medio de la tanda envenenaba todo lo que viniera detrás, y
     de forma intermitente, que es lo peor de todo.
- `python e2e_port_linger.py <PepoMote.exe> [dir]` — arranca él solo un
  receptor aislado con el puerto UDP del móvil todavía en manos de un proceso
  que acaba de morir (un padre lo abre, un hijo lo hereda y el padre muere):
  lo que pasa al actualizar en caliente o al cerrar y abrir deprisa. El
  receptor debe esperar a que se suelte, cogerlo y llegar al inyector; hasta
  la 1.13.0 se rendía a la primera y se quedaba con «Inyección: ninguna».
  Se cierra solo; en Linux sin ventana añade `PEPOMOTE_NO_UI=1`.
- `python e2e_ui_hang.py <PepoMote.exe> [dir]` — la ventana se cuelga
  después de pintar (`PEPOMOTE_FAKE_UI_HANG=1`: el hilo de la ventana se
  duerme en el fotograma 3, con el QR a la vista; `=40`, en el 40) y tiene
  que notarse:
  `--diag` imprime «Receptor abierto: … último paso: gancho de prueba»
  preguntándoselo al receptor por su cerrojo, `receptor.log` apunta
  «Ventana: sin repintar desde hace 10 s · último paso: …» y el modo humo
  sale con 4 en vez de quedarse colgado. Sin el gancho, el humo sale con 0
  y el log no habla de cuelgues. Abre ventanas de verdad (dos receptores
  aislados, uno detrás de otro); hasta la 1.13.1 el humo daba por buena
  una ventana que ya no repintaba.
- `python e2e_screen.py <segundos> <salida.jpg>` — canal de pantalla (doble
  pantalla del GamePad): sesión mala rechazada, apertura, tramas. Con Cemu
  abierto y su ventana GamePad View a la vista (por ejemplo
  `Cemu.exe -g <juego.wux>`) recibe imágenes reales y guarda la última;
  sin Cemu comprueba que llegan estado y keepalive. Al final manda `text`
  (teclado del móvil para el teclado en pantalla de Cemu) y comprueba que la
  sesión sigue viva.
- `python cemu_live.py [idle|buttons|sticks|touch|all]` — móvil simulado contra
  el receptor NORMAL (token real por `PEPOMOTE_TOKEN`), para ver en Cemu el
  mando moviéndose. Un archivo `cemu_live.cmd` junto al script cambia el patrón.

Linux, compositor wlroots de verdad (lo que corre la CI en `desktop.yml`):
`bash desktop/e2e/e2e_wayland.sh` arranca un `sway` sin cabeza
(`WLR_BACKENDS=headless`, sin GPU) con `wev` a pantalla completa, un
receptor aislado sin ventana (`PEPOMOTE_NO_UI=1`) que debe elegir el backend
Wayland (línea `Inyección: Wayland` en `receptor.log`), y
`e2e_wayland.py`: hello por código, barrido de yaw, clic, cruceta ← → (atrás y
adelante del navegador), rueda y `text`, comprobando en la salida de `wev` los
`motion` (centro y recorrido), el `button 272`, las teclas `XF86Back` y
`XF86Forward`, el `axis` con el signo de Wayland y las teclas `a`, `b`, Intro.
Después escribe `ñ`, un emoji y `€`, que no tienen tecla propia y salen por
teclas de repuesto con su keysym Unicode (el receptor vuelve a subir el
keymap), y otra vez `ab` para comprobar que el ASCII sigue bien tras dos
cambios de keymap y que el inyector no se ha muerto por el camino.
Necesita `sway`, `wev` y `python3`; deja `wev.log`, `sway.log`,
`receptor.out` y `config/receptor.log` en `/tmp/pepomote-e2e`.
`python3 e2e_wayland.py --parse-only fixtures/wev_ok.log` prueba solo el
parser (vale en Windows), y
`python3 e2e_wayland.py --parse-only fixtures/wev_unicode.log --expect-text "abñ😀€ab"`
prueba el del texto Unicode.

Ventana (Linux): `bash smoke_gui.sh wayland|x11 [fallback]` abre la ventana de
verdad bajo un sway sin cabeza o un Xvfb, espera el primer fotograma y sale
sola (`PEPOMOTE_SMOKE=2500`); comprueba el código de salida, la línea «Ventana:
primer fotograma pintado» del log y que la ventana esté en el árbol de
ventanas. Con `fallback`, el primer intento falla a propósito
(`PEPOMOTE_SMOKE_FAIL_FIRST=1`) y se exige el relanzamiento con render por
software. Con `hang`, el sondeo del driver del mando virtual se cuelga a
propósito (`PEPOMOTE_FAKE_DRIVER_HANG=1`; vale también en Windows a mano, con
`PEPOMOTE_SMOKE`) y se exige que la ventana pinte igual y que el log diga
«Mando virtual: el sondeo del driver no contesta»: es la regresión de la
ventana negra de la 1.12, cuando ese sondeo iba en el hilo de la ventana.
`PEPOMOTE_BIN=dist/PepoMote-x86_64.AppImage APPIMAGE_EXTRACT_AND_RUN=1`
lo prueba con el AppImage. Necesita `sway` o `Xvfb` + `x11-utils` y las
bibliotecas de Mesa; deja todo en `/tmp/pepomote-smoke/<modo>`.

Puntero: `PEPOMOTE_RECORD=<archivo>` en el receptor graba cada INPUT del
Jugador 1 (llegada + paquete crudo) y `PepoMote --replay <archivo> [sens_deg]`
lo pasa por el motor del puntero y saca un CSV (sensor, llegada, gyro, quat,
salida) para analizar un gesto real fuera de línea.

Ojo en Windows: un `python` instalado como paquete MSIX ve un
`%APPDATA%\Roaming` virtualizado; pasa rutas absolutas y el token por variable
de entorno en vez de leer `%APPDATA%` desde Python.

## Validación local de Switch y regresiones

`python desktop/e2e/run_switch_preview.py desktop/target/release/PepoMote.exe dist/switch-preview/validation/e2e` ejecuta secuencialmente Switch, Wii U y Nunchuk contra un binario ya compilado. Crea directorios y puertos propios, desactiva actualizaciones y bandeja, y termina únicamente sus procesos de prueba. Los overrides `PEPOMOTE_CEMU_DIR` y `PEPOMOTE_EDEN_DIR` impiden buscar instalaciones personales o portátiles. Deja `checks.log` y el log del receptor por suite.

`e2e_eden.py` comprueba negociación, Pro Controller por jugador, migración de preferencias antiguas, cambios de modo, botones/sticks/gyro DSU y pulsación/liberación sin sensores en las tres consolas, ausencia de táctil y escritura de `qt-config.ini` con respaldo y secciones ajenas conservadas. La fixture incluye otro servidor DSU para comprobar los índices globales de Eden. Estas pruebas simulan móviles y el cliente DSU; no sustituyen una prueba con Eden, juegos y sensores físicos.


## Puntero con RetroArch real (Windows)

`real_retroarch_pointer.py` usa `fixtures/pointer_core.rs`, un núcleo de
prueba sin ROM que registra las llamadas reales Mouse, Lightgun y RetroPad.
Compílalo con `rustc --edition 2021 --crate-type cdylib
 desktop/e2e/fixtures/pointer_core.rs -o pointer_libretro.dll` y ejecuta
`py -3 desktop/e2e/real_retroarch_pointer.py <PepoMote.exe>
 <retroarch.exe> <pointer_libretro.dll>`.

La prueba crea ajustes y registros temporales, abre su propia ventana de
RetroArch y la activa para que DirectInput reciba el ratón. Comprueba las
orientaciones 0/90/270/0, movimiento absoluto y relativo, cuatro botones sin
clics duplicados, gatillo de J1 con J2 activo, recarga y liberación al cambiar
de mando. No modifica la configuración del RetroArch proporcionado.
