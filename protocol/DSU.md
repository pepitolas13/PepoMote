# DSU (cemuhook) — notas verificadas y mapeo PepoMote

Servidor DSU del receptor: `127.0.0.1:26760` (UDP), slot 0, un mando. Verificado contra la spec comunitaria (v1993.github.io/cemuhook-protocol) y `DualShockUDPClient.cpp` del código de Dolphin.

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

## Recentrado

La diana del móvil incrementa `recenter_count` (PMP); el servidor DSU traduce cada flanco en un **pulso de 150 ms del botón Touch**, que el perfil mapea a `IMUPointer/Recenter`. Así el mismo gesto recentra en modo puntero y en Dolphin.

## Modo

El receptor solo alimenta el DSU en modo `dolphin` (en modo puntero el pad se reporta desconectado al caducar el TTL de 1 s). Así un Dolphin abierto no recibe movimiento mientras usas el cursor.
