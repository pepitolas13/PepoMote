# PepoMote en Linux móvil (Mobian, postmarketOS…)

El mismo emisor que la app Android, nativo para móviles con Linux de verdad:
PinePhone / PinePhone Pro, Librem 5, OnePlus 6/6T, Fairphone, SHIFT6mq,
Poco F1… cualquier móvil con **kernel mainline** (los sensores se leen por
IIO) y un escritorio Wayland (Phosh, Plasma Mobile, Sxmo, GNOME Mobile).

Se abre como una app más. Todo es táctil (multitouch real: puedes mantener B
y pulsar A). Sin cámara: el emparejamiento es con el **código de 4 dígitos**
que el receptor muestra bajo el QR.

## Instalar

Descarga de la release el paquete de tu distro y el `packaging/linux-mobile/`
del repo (o usa el `install.sh` que va dentro del tar.gz):

| Distro | Paquete |
|---|---|
| Mobian, Debian, Ubuntu, Fedora, Arch/Manjaro ARM… (glibc) | `PepoMote-Mobile-aarch64.AppImage` |
| postmarketOS, Alpine (musl) | `PepoMote-Mobile-aarch64-musl.tar.gz` |

```bash
bash install.sh PepoMote-Mobile-aarch64.AppImage        # o el .tar.gz
```

El script deja la app en `~/.local/opt/PepoMote-Mobile` (extrae el AppImage:
no hace falta FUSE), crea el lanzador con su icono y, si tienes `sudo`,
instala una regla udev opcional que permite subir la frecuencia del IMU
(sin ella funciona igual, a la frecuencia por defecto del driver, 50-100 Hz).

Desde el repo también vale `packaging/linux-mobile/install.sh <paquete>`.

## Emparejar (una sola vez)

1. Abre PepoMote en el PC (muestra el QR y, debajo, **"Sin cámara: código
   1234"**; el código cambia cada 2 minutos y es de un solo uso).
2. En el móvil: **Conectar** → aparece tu PC en la lista (búsqueda por
   broadcast en tu Wi-Fi) → tócalo → teclea el código. Si no aparece,
   **Escribir IP a mano** con la IP:puerto que hay bajo el QR.
3. Listo: el móvil guarda el emparejamiento (`~/.config/pepotech/PepoMote/pairing.json`)
   y a partir de ahí conecta de un toque. Si el PC cambia de IP, la app lo
   vuelve a encontrar por nombre sola.

## Usar

- **Conectar**: modo puntero (el cursor del PC va a donde apuntas). Diana =
  recentrar (mantener 150 ms). A = clic izquierdo, B = derecho, tira derecha =
  scroll, cruceta = flechas, 1/2 = Enter/Esc, ± = volumen, multimedia plegable.
- **Dolphin**: Wiimote virtual (el receptor alimenta el servidor DSU). Con
  varios móviles, cada uno entra como Jugador N; el modo lo manda el Jugador 1.
- **Mando**: solo botones, sin cambiar el modo.
- La cabecera enseña RTT y la frecuencia real del sensor.

## Sensores (IIO)

La app busca en `/sys/bus/iio/devices` un dispositivo con `in_anglvel_*_raw`
(giroscopio) y `in_accel_*_raw` (acelerómetro), aplica escala, offset y la
`mount_matrix` del dispositivo, y fusiona gyro+accel (Madgwick) para producir
la misma orientación que el GAME_ROTATION_VECTOR de Android. Comprobar que
hay gyro:

```bash
ls /sys/bus/iio/devices/*/in_anglvel_x_raw
```

Si no existe, el móvil no tiene gyro expuesto (o falta el driver): PepoMote
necesita giroscopio.

Frecuencia: la app pide la mayor disponible ≤ 250 Hz (`sampling_frequency`).
Escribir ahí requiere permiso: la regla udev de `install.sh` lo da; si no,
verás la frecuencia real en la cabecera del mando.

## Limitaciones conocidas

- **Ubuntu Touch no está soportado**: sus móviles (Halium) no exponen los
  sensores por IIO sino por el HAL de Android, y Lomiri solo instala apps Qt
  en paquetes click. Es otro proyecto.
- La pantalla se apaga con el tiempo de bloqueo del sistema (no hay
  "mantener encendida" desde la app): súbelo mientras juegas.
- Las teclas físicas de volumen las gestiona el escritorio, no la app.
- Sin sonidos ni vibración (por ahora).

## Compilar

```bash
cd mobile-linux && cargo build --release      # binario en target/release/PepoMote-Mobile
cargo test                                    # fusión, IIO (sysfs simulado), botones, códec
./target/release/PepoMote-Mobile --fake-sensors   # en un PC, sin IMU: sensores simulados
./target/release/PepoMote-Mobile --pair 192.168.1.10 1234   # emparejar sin UI
./target/release/PepoMote-Mobile --autoconnect dolphin      # directo al mando, ya conectado
```

Opciones: `--fake-sensors` (sin IMU), `--pair HOST[:PUERTO] CODIGO`,
`--autoconnect [pointer|dolphin]` (para un lanzador que abra el mando
conectado). En el receptor, `PEPOMOTE_PORT=26771` cambia el puerto PMP
(puerto ocupado o dos receptores en el mismo PC); el móvil lo teclea como
`IP:puerto`.

Empaquetado: `packaging/linux-mobile/appimage.sh aarch64` (glibc) y
`packaging/linux-mobile/tarball.sh aarch64-musl` (tras compilar en Alpine).
El CI (`mobile-linux.yml`) hace ambos en runners ARM nativos.
