# DSU (cemuhook) — notas verificadas y mapeo PepoMote

Servidor DSU del receptor: `127.0.0.1:26760` (UDP), hasta 4 mandos (slot = jugador − 1, MAC `PMP1`+0x00+slot). Verificado contra la spec comunitaria (v1993.github.io/cemuhook-protocol) y `DualShockUDPClient.cpp` del código de Dolphin.

## Estructura

- **Header 16 bytes LE**: magic `DSUS` (servidor→cliente) / `DSUC` (cliente→servidor) · versión u16 = **1001** · longitud del payload sin header u16 · CRC32 u32 del paquete entero con este campo a cero · id del emisor u32.
- Tras el header, tipo de mensaje u32: `0x100000` versión · `0x100001` PortInfo · `0x100002` PadData.
- **PortInfo** (respuesta): 11 bytes por mando — slot u8, estado u8 (2=conectado), modelo u8 (**2 = gyro completo**), tipo de conexión u8, MAC 6 bytes, batería u8 — más 1 byte cero final.
- **PadData** (respuesta, 100 bytes totales): header 16 + tipo 4 + los 11 bytes de PortInfo + connected u8(1) + nº de paquete u32 (contador propio del servidor) + bitmask de botones 2 B + botón PS/Home u8 + botón Touch u8 + sticks LX,LY,RX,RY (0-255, neutro 128, **Y invertida: 255 = arriba**) + 12 B analógicos de botones + 2 toques de 6 B + **timestamp de movimiento u64 en µs** + accel X,Y,Z f32 + gyro pitch,yaw,roll f32.

## Unidades en el cable

**Accel en g. Gyro en °/s.** Dolphin convierte internamente (÷ por la gravedad para g→m/s² y a rad/s). Enviar m/s² por el cable multiplica los movimientos por ~9,8 y rompe el juego.

Android entrega accel en m/s² y gyro en rad/s → el receptor convierte SOLO al construir PadData:

```
accel_dsu = accel_android / 9.80665
gyro_dsu  = gyro_android · 180 / π
```

## Timestamp

`motion_ts` = `t_sensor_us` del paquete PMP **tal cual** (reloj del sensor del móvil). Dolphin integra el gyro con este timestamp: el jitter de la red no ensucia la integración. No usar el reloj de llegada.

## Cadencia y clientes

- Emitir un PadData por cada INPUT recibido, tope 250 Hz.
- Dolphin re-envía sus peticiones (PortInfo/PadData) cada 1 s. Expirar el registro de un cliente a los 3 s sin re-petición.
- **La petición PadData lleva suscripción** (tras el tipo: `flags` u8, `pad_id` u8, `mac` [6]): `flags=0` = todos los pads; bit0 = solo `pad_id`; bit1 = solo la MAC (combinables). HAY QUE HONRARLA: Dolphin abre un socket UDP por mando y se registra con `flags=1, pad_id=índice`, y al recibir NO filtra por slot (se queda con el último PadData que entre por ese socket). Enviar todos los slots a todos los sockets hace que cada Wiimote se mueva con todos los móviles a la vez (bug real de v1.1.0 con dos jugadores).
- Responder peticiones de versión con 1001.

## Mapeo de ejes (móvil en mano como mando: pantalla arriba, borde superior apuntando a la TV)

Ejes Android: X = derecha del dispositivo, Y = hacia la TV (borde superior), Z = perpendicular a la pantalla, hacia arriba.

```
dsu_accel_x = -ax / 9.80665
dsu_accel_y = -az / 9.80665
dsu_accel_z = +ay / 9.80665
dsu_pitch   = +gx · 180/π
dsu_yaw     = -gz · 180/π
dsu_roll    = +gy · 180/π
```

Convención verificada contra `DualShockUDPClient.cpp` de Dolphin: `Accel Up =
-y_dsu`, `Accel Right = -x_dsu`, `Accel Forward = +z_dsu`, `Gyro Pitch Up =
+pitch`, `Yaw Right = +yaw`, `Roll Right = +roll`. El pitch del puntero IMU lo
ancla el ACELERÓMETRO (el recentrado de Dolphin solo resetea el yaw): un signo
mal en `dsu_accel_z` invierte el vertical aunque el gyro esté bien.

Los signos exactos se validan en el hito 3 con el protocolo de calibración; **cualquier corrección se hace únicamente en `desktop/src/dsu/mapping.rs`** (matriz de signos comentada por eje).

## Protocolo de calibración (h3, contra las barras vivas de Dolphin)

En Dolphin: Controllers → Alternate Input Sources ON (servidor 127.0.0.1:26760) → mapear un Wiimote emulado y abrir Motion Input. Las entradas `Accel Up/Down/Left/Right/Forward/Backward` y `Gyro Pitch/Yaw/Roll ±` muestran barras en vivo.

6 poses estáticas (accel, cada una debe encender SOLO su barra):
1. Plano sobre la mesa, pantalla arriba → Accel Up
2. Boca abajo → Accel Down
3. De canto sobre el borde izquierdo → Accel Left... (las 6 caras del dispositivo)

3 rotaciones puras (gyro, con el móvil apuntando a la TV):
1. Muñeca arriba/abajo → Gyro Pitch
2. Girar a izquierda/derecha (plano horizontal) → Gyro Yaw
3. Rotar sobre el eje de apuntado → Gyro Roll

Con las 9 comprobaciones en verde, el puntero IMU ("Point" con Total Yaw/Pitch + Recenter) y Wii Sports funcionan. Perfil listo en `assets/dolphin/PepoMote.ini`.

## Nunchuk (segundo móvil)

Un móvil con `role = nunchuk` (PROTOCOL.md §3) ocupa un slot DSU propio, asignado desde el 3 hacia abajo (los Wiimotes van del 0 hacia arriba); el Nunchuk i-ésimo pertenece al Wiimote i-ésimo. Su PadData lleva: `LX = 128 + stick_x`, `LY = 128 + stick_y` (255 = arriba, como exige Dolphin: `Left Y+`), C → Cross y Z → Circle (bytes analógicos, igual que A/B), y su accel/gyro con el mismo mapeo de ejes. En la configuración de Dolphin el Wiimote emulado del jugador lleva `Extension = Nunchuk` y las entradas del Nunchuk apuntan al pad del otro móvil con nombre completo:

```
Nunchuk/Buttons/C = `DSUClient/3/PepoMote:Cross`
Nunchuk/Buttons/Z = `DSUClient/3/PepoMote:Circle`
Nunchuk/Stick/Up = `DSUClient/3/PepoMote:Left Y+`   (Down = Left Y-, Left = Left X-, Right = Left X+)
Nunchuk/IMUAccelerometer/Up = `DSUClient/3/PepoMote:Accel Up`   (y los otros cinco ejes)
```

Con el acelerómetro IMU mapeado, Dolphin usa la aceleración real del móvil para el Nunchuk (agitar, inclinar: boxeo de Wii Sports) en vez de los gestos simulados.

## Modo Wii U (Cemu)

Cemu lee del PadData los bits de los bytes 36-37 (botón i = bit i del 36, 8+i = bit i del 37), el Touch (byte 39) como botón 16, los sticks (40-43), los gatillos analógicos l2/r2 (bytes 55/54) como ejes y el touchpad 1 (bytes 56-61: activo, id, x u16 0..1920, y u16 0..942) como posición; el PS (38) lo ignora. Con 17 botones digitales para 19 del GamePad, ZL/ZR van por los gatillos analógicos (Cemu convierte un eje pasado de la zona muerta en pulsación) y L2/R2 quedan para soplar y TV↔Pad. Mapeo de `desktop/src/dsu/mapping.rs` (`buttons_to_dsu_wiiu`) y los perfiles que escribe `desktop/src/cemu.rs`:

| Wii U | PadData | botón/eje en Cemu |
|---|---|---|
| A / B / X / Y | Cross / Circle / Square / Triangle (bits 37.6/5/7/4 + analógicos 48-51) | 14 / 13 / 15 / 12 |
| 1 / 2 (Mando Wii) | Square / Triangle | 15 / 12 |
| L / R | L1 / R1 (bits 37.2/3 + analógicos 53/52) | 10 / 11 |
| ZL / ZR | l2 / r2 analógicos (bytes 55 / 54) = 255, sin bit | ejes 42 / 43 (`X/Y-Trigger+`) |
| soplar / pantalla | bits L2 / R2 (37.0 / 37.1) | 8 / 9 |
| click stick izq / dcho | L3 / R3 (bits 36.1 / 36.2) | 1 / 2 |
| + / − | Options / Share (bits 36.3 / 36.0) | 3 / 0 |
| Home | Touch (byte 39) = 0xFF, y PS | 16 |
| cruceta | bits 36.4-7 + analógicos 44-47 | 4-7 |
| stick izquierdo / derecho | LX LY / RX RY = 128 + valor (255 = arriba) | ejes 38/39/44/45 y 40/41/46/47 |
| pantalla táctil del GamePad | touchpad 1 activo, x = `touch_x`·1920/65535, y = `touch_y`·942/65535 | `has_position` → toque en la pantalla del GamePad |
| puntero IR de un Mando Wii | touchpad 1 con la salida del motor de puntero del receptor (fuera de pantalla: inactivo) | `has_position` → IR del Wiimote emulado |

En este perfil el pulso de recentrado NO toca el byte Touch (es Home); el recentrado lo consume el motor de puntero del receptor cuando el móvil es Mando Wii. Perfil de Cemu por jugador en `controllerProfiles/controller{N}.xml` con `<api>DSUController</api>`, `<uuid>{pad}</uuid>`, `<ip>127.0.0.1</ip>`, `<port>26760</port>` y `<motion>true</motion>` (GamePad y Mando Wii); Mando Wii con `<device_type>5</device_type>` (MotionPlus) o `6` (MotionPlus + Nunchuk, segundo `<controller>` leyendo del pad del otro móvil).

## Recentrado

La diana del móvil incrementa `recenter_count` (PMP); el servidor DSU traduce cada flanco en un **pulso de 150 ms del botón Touch**, que el perfil mapea a `IMUPointer/Recenter`. Así el mismo gesto recentra en modo puntero y en Dolphin.

## Modo

El receptor solo alimenta el DSU en modo `dolphin` (perfil Wii) y en modo `cemu` (perfil Wii U); en modo puntero el pad se reporta desconectado al caducar el TTL de 1 s. Así un Dolphin o un Cemu abiertos no reciben movimiento mientras usas el cursor.
