# PepoMote en Linux móvil (Mobian, postmarketOS…)

El mismo emisor que la app Android, nativo para móviles con Linux de verdad:
PinePhone / PinePhone Pro, Librem 5, OnePlus 6/6T, Fairphone, SHIFT6mq,
Poco F1… cualquier móvil con **kernel mainline** (los sensores se leen por
IIO) y un escritorio Wayland (Phosh, Plasma Mobile, Sxmo, GNOME Mobile).

Se abre como una app más. Todo es táctil (multitouch real: puedes mantener B
y pulsar A). Sin cámara: el emparejamiento es con el **código de 4 dígitos**
que el receptor muestra bajo el QR.

## Instalar

**Mobian (o cualquier Debian/Ubuntu en el móvil): sin terminal.** Abre en el
navegador del móvil la [última release](https://github.com/pepitolas13/PepoMote/releases/latest),
descarga `pepomote-mobile_<versión>_arm64.deb`, tócalo en las descargas y
pulsa **Instalar** (lo abre "Software"). Aparece "PepoMote" en el lanzador,
con la regla del sensor ya instalada. Si tu Mobian no abre .deb al tocarlos:
`sudo apt install ~/Downloads/pepomote-mobile_*_arm64.deb`.

**Cualquier otra distro**, un solo comando en el terminal (detecta glibc o musl
y descarga el paquete que toca de la última release):

```sh
wget -qO- https://raw.githubusercontent.com/pepitolas13/PepoMote/main/packaging/linux-mobile/install.sh | sh
```

(Con `curl`: `curl -fsSL <esa URL> | sh`.) Si prefieres descargar el paquete a
mano: `sh install.sh PepoMote-Mobile-aarch64.AppImage` (Mobian, Debian,
Fedora, Arch… — glibc) o `sh install.sh PepoMote-Mobile-aarch64-musl.tar.gz`
(postmarketOS / Alpine — musl).

El script deja la app en `~/.local/opt/PepoMote-Mobile` (extrae el AppImage:
no hace falta FUSE), crea el lanzador con su icono y, si hay `sudo` o `doas`,
instala una regla udev opcional que permite subir la frecuencia del IMU
(sin ella funciona igual, a la frecuencia por defecto del driver, 50-100 Hz).
Desde el repo también vale `packaging/linux-mobile/install.sh <paquete>`.

## Emparejar (una sola vez)

1. Abre PepoMote en el PC (muestra el QR y, debajo, **"Sin cámara: código
   1234"**; el código vale 5 minutos y es de un solo uso).
2. En el móvil: **Conectar** → aparece tu PC en la lista (búsqueda por
   broadcast en tu Wi-Fi) → tócalo → teclea el código. Si no aparece,
   **Escribir IP a mano** con la IP:puerto que hay bajo el QR.
3. Listo: el móvil guarda el emparejamiento (`~/.config/pepomote/pairing.json`)
   y a partir de ahí conecta de un toque. Si el PC cambia de IP, la app lo
   vuelve a encontrar por nombre sola.

## Usar

- **Conectar**: modo puntero (el cursor del PC va a donde apuntas). Diana =
  recentrar (mantener 150 ms). A = clic izquierdo, B = derecho, tira derecha =
  scroll, tira izquierda (mirilla) = precisión, cruceta ↑/↓ = flechas y ←/→ =
  atrás/adelante del navegador, 1/2 = Enter/Esc, ± = volumen (mantener
  repite), multimedia plegable.
- **Dolphin**: Wiimote virtual (el receptor alimenta el servidor DSU). Con
  varios móviles, cada uno entra como Jugador N; el modo lo manda el Jugador 1.
- **Mando**: solo botones, sin cambiar el modo.
- **Nunchuk**: el móvil como Nunchuk del Jugador 1 (o del siguiente mando sin
  Nunchuk): stick, C, Z y su acelerómetro van al Nunchuk emulado en Dolphin.
- **Wii U**: el móvil apaisado como Wii U GamePad para Cemu (dos sticks,
  A/B/X/Y, L/R/ZL/ZR, giroscopio, pantalla táctil). Si el escritorio del móvil
  no gira la pantalla (Phosh sin rotación automática), la app pinta el mando
  girado para que lo sostengas apaisado; el ajuste **Giro** (izquierda /
  derecha) dice hacia dónde queda el borde superior del móvil y orienta los
  sensores. Chip **Mando Wii** para ser un mando de Wii dentro de Cemu. La
  zona central enseña la pantalla del GamePad que manda Cemu, y el botón
  **Teclado** escribe en el teclado en pantalla de Cemu (que no acepta
  toques). Ver `docs/SETUP-CEMU.md`.
- La cabecera enseña RTT y la frecuencia real del sensor.

## Sensores

Dos caminos, en este orden:

1. **IIO** (PinePhone/Pro, Librem 5 y todo móvil cuyo kernel exponga el IMU).
2. **Qualcomm SSC/SLPI** (Snapdragon 845 y posteriores: OnePlus 6/6T, SHIFT6mq,
   Poco F1…): en esos móviles el IMU cuelga del DSP de sensores y Linux no ve
   nada en IIO. La app habla con el DSP directamente por QRTR/QMI (el mismo
   protocolo que usa libssc para la rotación de pantalla), pide gyro y accel a
   ~200 Hz y usa los timestamps del propio DSP. Requiere el SLPI arrancado
   (firmware del dispositivo + `hexagonrpcd`, lo normal en postmarketOS).

`PepoMote-Mobile --sensors` enseña qué ve la app por los dos caminos y, si
alguno funciona, lee 1,5 s de muestras: frecuencia real y vector de gravedad
(con el móvil plano boca arriba debe salir `z ≈ +9.8`).

### Calibrar los ejes (puntero al revés o a tirones)

Cada camino entrega los ejes con su propia convención y a veces uno viene
invertido: el puntero sube cuando bajas el móvil, o va a tirones porque el
giroscopio y el acelerómetro se contradicen y la fusión los pelea. En
**Inicio → Calibrar sensores** hay seis pasos guiados (tres posturas quietas y
tres gestos de dos segundos); la app deduce el signo de cada eje, lo guarda en
`~/.config/pepomote/axes.json` y lo aplica en todas las conexiones
siguientes (Inicio lo muestra como `ejes accel +-+ gyro +++`). `--sensors`
también lo enseña. Para volver al estado original: el botón «Borrar
calibración» dentro de la pantalla.

### Deriva y suavidad

Ni IIO ni el SSC entregan el giroscopio calibrado (Android lo hace en su HAL),
así que un gyro en reposo marca unas décimas de grado por segundo y el puntero
se iría solo hacia un lado. La app estima ese bias sola: cuando el móvil está
quieto medio segundo (gyro y acelerómetro sin varianza), la media del gyro es
el bias, se adopta y luego se refina despacio. Basta con dejar el móvil quieto
un segundo al empezar; si deriva, déjalo sobre la mesa un momento.

Algunas fuentes entregan las muestras a ráfagas (el DSP de Qualcomm agrupa
varias): la fusión no lo nota porque usa los timestamps del sensor, pero el
puntero absoluto saltaría a cada ráfaga. La salida al receptor va a ritmo fijo
(250 Hz) e interpolada con el reloj de pared, con un retardo igual a la peor
ráfaga reciente (0 si la entrega es regular, máximo 80 ms). `--sensors`
enseña cómo llega la entrega (hueco máximo y porcentaje en ráfaga).

### Qualcomm SSC/SLPI (postmarketOS)

En postmarketOS los paquetes de dispositivo SDM845 (OnePlus 6/6T, SHIFT6mq…)
ya traen todo: `firmware-*-sensors` (registro de sensores copiado de Android
en `/usr/share/qcom/<soc>/<Vendor>/<device>/sensors/`) y el servicio
`hexagonrpcd-sdsp`, que sirve esos ficheros al DSP por FastRPC. Con eso el
SLPI arranca y anuncia el servicio QMI `400` (SSC) en el bus QRTR. Si
`--sensors` dice que el bus no lo anuncia:

```bash
cat /sys/class/remoteproc/remoteproc*/name /sys/class/remoteproc/remoteproc*/state
sudo rc-service hexagonrpcd-sdsp status
sudo rc-service hexagonrpcd-sdsp restart
```

`slpi` debe estar en `running` y `hexagonrpcd` corriendo; tras reiniciarlo,
el SSC tarda unos segundos en estar listo. El acceso al bus QRTR no requiere
root.

### IIO

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
- Mientras hay conexión la app mantiene la pantalla encendida y evita la
  suspensión en Phosh / GNOME Mobile (`gnome-session-inhibit`). En otros
  escritorios solo evita la suspensión (`systemd-inhibit` o `elogind-inhibit`)
  y la pantalla se apaga con el tiempo de bloqueo del sistema: súbelo mientras
  juegas. `mobile.log` dice cuál de los tres ha usado.
- Las teclas físicas de volumen las gestiona el escritorio, no la app.
- Sin sonidos ni vibración (por ahora).

## Si algo falla

Todo lo de la app vive en `~/.config/pepomote/`:

- `mobile.log`: lo que ve la app (pantalla y escala, sensores encontrados,
  bias del gyro, ráfagas, calibración, inhibidor de pantalla). Es lo primero
  que mirar.
- `launch.log`: solo si instalaste con `install.sh`; errores de arranque y el
  reintento con render por software.
- `pairing.json` y `axes.json`: emparejamiento y calibración. Borrarlos =
  empezar de cero.
- `PepoMote-Mobile --sensors` en un terminal: inventario de sensores por los
  dos caminos y 1,5 s de muestras reales.

## Compilar

```bash
cd mobile-linux && cargo build --release      # binario en target/release/PepoMote-Mobile
cargo test                                    # fusión, IIO (sysfs simulado), botones, códec
./target/release/PepoMote-Mobile --fake-sensors   # en un PC, sin IMU: sensores simulados
./target/release/PepoMote-Mobile --pair 192.168.1.10 1234   # emparejar sin UI
./target/release/PepoMote-Mobile --autoconnect dolphin      # directo al mando, ya conectado
./target/release/PepoMote-Mobile --autoconnect cemu         # directo al GamePad de Wii U
```

Opciones: `--fake-sensors` (sin IMU), `--pair HOST[:PUERTO] CODIGO`,
`--autoconnect [pointer|dolphin|cemu|nunchuk]` (para un lanzador que abra el mando
conectado). En el receptor, `PEPOMOTE_PORT=26771` cambia el puerto PMP
(puerto ocupado o dos receptores en el mismo PC); el móvil lo teclea como
`IP:puerto`.

Empaquetado: `packaging/linux-mobile/appimage.sh aarch64` (glibc) y
`packaging/linux-mobile/tarball.sh aarch64-musl` (tras compilar en Alpine).
El CI (`mobile-linux.yml`) hace ambos en runners ARM nativos.
