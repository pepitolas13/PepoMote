# SETUP-DOLPHIN — jugar a la Wii con PepoMote

Requisitos: Dolphin 5.0+ reciente (2023 en adelante), PepoMote en el PC y el
móvil emparejado.

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
2. Dolphin → Controllers → **Wiimote 1 = Emulated Wii Remote** → Configure.
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

- **Dolphin no lista el servidor**: ¿modo Dolphin activo en el móvil? ¿"1
  cliente(s) DSU" en la ventana? Reinicia Dolphin tras añadir el servidor.
- **El puntero del menú deriva**: recentra (diana). Ajusta `Total Yaw/Pitch`
  en Motion Input a tu gusto (más grados = menos sensible).
- **Movimiento invertido en un juego**: comprueba la calibración del punto 4
  antes de tocar nada más.
