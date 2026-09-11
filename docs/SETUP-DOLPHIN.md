# SETUP-DOLPHIN — jugar a la Wii con PepoMote

## Multijugador (v1.1): se configura solo

Conecta el segundo móvil escaneando el MISMO QR: entra como Jugador 2. Con
Dolphin CERRADO y el modo Dolphin activo, PepoMote deja Dolphin listo por ti:
selecciona **Emular el adaptador Bluetooth de la Wii** (con "Acceder
directamente a un adaptador de Bluetooth" el juego se cierra al arrancar),
escribe un mando emulado por móvil conectado (Wiimote N = móvil N por orden
de conexión) con el mapeo PepoMote, deja los demás en «Ninguno» (un mando de
más aparece en pantalla en los juegos y molesta) y registra el servidor DSU. Deja copia `.pepomote.bak` de cada archivo que
toca. Abre Dolphin y a jugar: cada móvil es su propio Wiimote. Si Dolphin
estaba abierto, PepoMote lo deja pendiente y lo escribe solo en cuanto lo
cierras (Dolphin pisa su configuración al salir): ciérralo, ábrelo y listo.
El automatismo se puede apagar en Ajustes. Los pasos manuales de abajo solo
hacen falta si prefieres mapear a mano.

Dónde escribe: en la carpeta de usuario que usa tu Dolphin, con la misma
lógica que Dolphin (portable.txt junto al exe → registro → `Documentos\Dolphin
Emulator` si existe → `AppData\Roaming\Dolphin Emulator`; en Linux
`~/.config/dolphin-emu` o el Flatpak). La ventana del PC lo dice («carpeta:
portable…», «AppData…»). Un Dolphin portable (RetroBat, LaunchBox, una
carpeta suelta) se reconoce al verlo abierto; si no, pon la carpeta del
`Dolphin.exe` en Ajustes → Carpeta de Dolphin → Detectar.

Requisitos: Dolphin 5.0+ reciente (2023 en adelante), PepoMote en el PC y el
móvil emparejado.

## Nunchuk: dos móviles, uno en cada mano

Abre PepoMote en un segundo móvil y toca **Nunchuk**: entra emparejado con el
mando del Jugador 1 (la ventana del PC lo lista como «J1 · Nunchuk»). En el
móvil tienes el stick, C y Z; su acelerómetro también llega a Dolphin (agitar
e inclinar el Nunchuk: boxeo de Wii Sports). El receptor configura solo el
Wiimote del jugador con `Extension = Nunchuk` leyendo del pad de ese segundo
móvil, con Dolphin cerrado como siempre. Con más jugadores, el segundo
Nunchuk que entre es el del Jugador 2, y así. El móvil Nunchuk no mueve el
cursor del PC ni cambia el modo: eso lo decide el mando.

## 1. Activar el modo Dolphin

En el móvil, dentro del mando: chip **Dolphin** (o la tarjeta Dolphin del menú
principal). La ventana del PC pasa a "Modo Dolphin" y deja de mover el cursor:
todo el movimiento va ahora al servidor DSU en `127.0.0.1:26760`.

## 2. Conectar Dolphin al servidor (una sola vez)

1. Dolphin → **Opciones → Configuración del mando** (Controllers).
2. Abajo: **Alternate Input Sources** → marcar **Enable**.
3. Add server: descripción **PepoMote** (importante: este nombre exacto, el
   perfil lo referencia), dirección `127.0.0.1`, puerto `26760`. Aceptar.
4. En la ventana de PepoMote del PC verás "Dolphin: 1 cliente(s) DSU" —
   confirmación de que Dolphin está escuchando.

## 3. Perfil del Wiimote (una sola vez)

1. Copia `assets/dolphin/PepoMote.ini` a la carpeta de perfiles de Dolphin:
   - Windows: `Documentos\Dolphin Emulator\Config\Profiles\Wiimote\`
   - Linux: `~/.config/dolphin-emu/Profiles/Wiimote/` (o el equivalente flatpak)
2. Dolphin → Controllers → en «Mandos de Wii» marca **Emular el adaptador
   Bluetooth de la Wii** y pon **Wiimote 1 = Emulated Wii Remote** → Configure.
3. Arriba a la derecha, en **Profile**: elige `PepoMote` → **Load**.
4. Comprueba en vivo: pestaña **Motion Input** — al mover el móvil, las barras
   de `Accel` y `Gyro` deben moverse; plano sobre la mesa, `Accel Up` marcada.

Si prefieres mapear a mano (o el perfil no carga): en Configure, Device =
`DSUClient/0/PepoMote`, y asigna A=Cross, B=Circle, 1=Square, 2=Triangle,
−=Share, +=Options, Home=PS, cruceta=`Pad N/S/W/E` (así llama Dolphin a la
cruceta del DSU), y en Motion Input los seis `Accel *` y los seis `Gyro *` a
sus homónimos. `IMUPointer/Recenter` = `Touch Button` (la diana del móvil
manda un pulso de Touch al recentrar).

## 4. Calibración de ejes (verificación de h3)

Con el diálogo de mapeo abierto (barras en vivo):

**6 poses estáticas** — cada una debe encender SOLO su barra de accel:
1. Plano sobre la mesa, pantalla arriba → `Accel Up`
2. Boca abajo → `Accel Down`
3. De canto, borde izquierdo abajo → `Accel Left`
4. De canto, borde derecho abajo → `Accel Right`
5. Vertical, borde superior arriba (pantalla hacia ti) → `Accel Forward`
6. Vertical, borde superior abajo → `Accel Backward`

**3 rotaciones puras** — móvil apuntando a la TV:
1. Muñeca arriba/abajo → `Gyro Pitch Up/Down`
2. Girar a izquierda/derecha (plano horizontal) → `Gyro Yaw Left/Right`
3. Rotar sobre el eje de apuntado → `Gyro Roll Left/Right`

Si alguna barra sale invertida o cruzada, se corrige en UN único archivo:
`desktop/src/dsu/mapping.rs` (matriz de signos comentada por eje).

## 5. Jugar

Wii Sports: en el menú, apunta con el móvil (puntero por IMU), diana para
recentrar. Bolos: mantén B, balancea y suelta. Boxeo: puños con el móvil.

## Problemas típicos

- **Dolphin enseña el mando desconectado (y el PC dice «1 cliente(s) DSU»)**:
  el DSU llega pero el Wiimote emulado no está activo en ESE Dolphin. Por
  orden: (1) el móvil tiene que estar en modo Dolphin; (2) cierra Dolphin y
  vuelve a abrirlo (si estaba abierto al conectar, la configuración queda
  pendiente y se escribe al cerrarse); (3) tiene que ser un juego de Wii, no
  de GameCube; (4) mira qué carpeta dice haber configurado la ventana del PC
  y compárala con Dolphin → Archivo → Abrir carpeta de usuario: si no es la
  misma (Dolphin portable), Ajustes → Carpeta de Dolphin → Detectar, o
  escribe la carpeta del Dolphin.exe a mano; (5) en Dolphin → Mandos, «Wii
  Remote 1» debe estar en «Emulated Wii Remote» y el adaptador Bluetooth en
  «Emular».
- **Dolphin no lista el servidor**: ¿modo Dolphin activo en el móvil? ¿"1
  cliente(s) DSU" en la ventana? Reinicia Dolphin tras añadir el servidor.
- **El puntero del menú deriva**: recentra (diana). Ajusta `Total Yaw/Pitch`
  en Motion Input a tu gusto (más grados = menos sensible).
- **Movimiento invertido en un juego**: comprueba la calibración del punto 4
  antes de tocar nada más.
