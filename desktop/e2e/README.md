# e2e del receptor (Windows/Linux, en el propio equipo)

Pruebas de extremo a extremo contra un receptor REAL: TCP/UDP del protocolo
PMP, cliente DSU como el de Dolphin/Cemu, y los archivos de configuración que
escribe. Necesitan Python 3 (sin dependencias).

Arranca un receptor de pruebas AISLADO (no toca tu configuración, ni Dolphin,
ni Cemu, y convive con el receptor normal):

```
APPDATA=<dir>\appdata            (Cemu roaming aislado; crea <dir>\appdata\Cemu vacío)
PEPOMOTE_CONFIG_DIR=<dir>\config  (settings.json y token.txt propios)
PEPOMOTE_DOLPHIN_DIR=<dir>\dolphin
PEPOMOTE_PORT=26771 PEPOMOTE_DSU_PORT=26770
PEPOMOTE_PAIR_CODE=1234 PEPOMOTE_ASSUME_EMULATOR_CLOSED=1 PEPOMOTE_NO_TRAY=1
PepoMote.exe
```

y con las mismas `PEPOMOTE_PORT`, `PEPOMOTE_DSU_PORT` y `PEPOMOTE_DOLPHIN_DIR`
en el entorno:

- `python e2e_cemu.py <dir>\appdata` — modo Wii U: `ok.modes`/`ok.pad`, perfiles
  de Cemu (GamePad, Pro, Mando de Wii + Nunchuk, backup/restauración), PadData
  Wii U (botones, gatillos, sticks, táctil, Home→Touch, puntero IR), avisos
  `pad`/`notice`, difusión de `mode`, regresión de Dolphin.
- `python e2e_nunchuk.py` — Nunchuk en modo Dolphin: slots, emparejamiento,
  stick/C/Z en su pad DSU.
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

Ojo en Windows: un `python` instalado como paquete MSIX ve un
`%APPDATA%\Roaming` virtualizado; pasa rutas absolutas y el token por variable
de entorno en vez de leer `%APPDATA%` desde Python.
