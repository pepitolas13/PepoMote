# PepoMote en iPhone y iPad

La app de iOS/iPadOS es el mismo emisor que la de Android: puntero, mando de
Wii para Dolphin, GamePad de Wii U para Cemu (con doble pantalla), Nunchuk,
varios PCs, reconexión automática, español/inglés y tema claro/oscuro.
Funciona en iOS/iPadOS **15 o superior** (desde iPhone 7 hasta los iPad con
iPadOS 26).

No está en la App Store: se instala con tu propio Apple ID (el gratuito vale).
Hay tres formas. **La que recomiendo personalmente es SideStore** (es la que
uso yo), aunque también se puede con AltStore y con Sideloadly:

- **SideStore** (mi recomendación): se renueva **en el propio iPhone/iPad**,
  sin ningún PC encendido. Solo necesita el PC el día que la instalas.
- **AltStore**: igual de fácil de usar, pero para renovar la firma necesita
  AltServer abierto en tu PC y en la misma Wi-Fi (sus autores trabajan en
  quitar ese requisito).
- **Sideloadly**: sin tienda; instalas el `PepoMote.ipa` a mano desde el PC
  y lo repites cada 7 días.

Con un Apple ID gratuito la firma dura 7 días (las tiendas la renuevan; con
Sideloadly la renuevas tú); con una cuenta de desarrollador de pago
(99 $/año) dura un año.

## La fuente de PepoMote (para SideStore y AltStore)

Es la misma URL en las dos tiendas: Sources (Fuentes) → **+** → pegar →
Browse → **PepoMote** → **Instalar**. Cuando salga una versión nueva te la
ofrece ahí mismo.

```
https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json
```

Enlaces de un toque (abren la tienda y añaden la fuente; funcionan desde una
web, un mensaje o un QR, no desde esta página de GitHub):

```
sidestore://source?url=https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json
altstore://source?url=https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json
```

## Antes de nada (una sola vez)

1. Instala el **receptor** en el PC como siempre (`PepoMote.exe` en Windows o
   el AppImage en Linux): en su ventana verás el QR.
2. En el iPhone/iPad, con iOS 16 o superior, activa el **modo Desarrollador**:
   Ajustes → Privacidad y seguridad → Modo Desarrollador → activar (pide
   reiniciar). Sin él, iOS no abre las apps instaladas fuera de la App Store.
   En iOS 15 no hace falta.
3. Móvil y PC en la **misma Wi-Fi**.

## Instalar con SideStore (mi recomendación: sin PC encendido)

SideStore es un derivado de AltStore que se refirma a sí mismo y a sus apps
desde el propio iPhone/iPad, con una VPN local (StosVPN) y tu Apple ID. El PC
hace falta una sola vez, para instalarlo y generar el archivo de
emparejamiento.

1. En el iPhone/iPad instala **StosVPN** desde la App Store (es de los
   autores de SideStore). Ábrela una vez y acepta añadir la configuración VPN.
2. En el PC baja `SideStore.ipa` de la última release de
   [github.com/SideStore/SideStore](https://github.com/SideStore/SideStore/releases).
3. Instala `SideStore.ipa` en el iPhone/iPad: con **Sideloadly**
   ([sideloadly.io](https://sideloadly.io); en Windows necesita iTunes de la
   web de Apple): cable, arrastra el IPA, tu Apple ID, **Start**. (También
   vale AltServer: mantén **Mayús** al pulsar su icono de la bandeja →
   «Sideload .ipa».)
4. En el iPhone/iPad: Ajustes → General → **VPN y gestión de dispositivos** →
   tu Apple ID → **Confiar**.
5. **Archivo de emparejamiento**: baja `jitterbugpair` para Windows
   ([github.com/osy/Jitterbug/releases](https://github.com/osy/Jitterbug/releases),
   `jitterbugpair-win64.zip`), descomprímelo y, con el dispositivo conectado
   por cable y **desbloqueado**, ejecuta `jitterbugpair.exe` en una terminal
   dentro de esa carpeta. Crea un archivo `….mobiledevicepairing`: renómbralo a
   `ALTPairingFile.mobiledevicepairing` y pásalo al iPhone/iPad (iCloud Drive,
   AirDrop o cable a la app Archivos).
6. Abre **SideStore**: elige el archivo de emparejamiento en Archivos cuando
   lo pida, inicia sesión con tu Apple ID (Settings → Sign in) y, si lo pide,
   enciende StosVPN.
7. Sources → **+** → pega la fuente de arriba → Browse → **PepoMote** →
   **Instalar**. Listo.

Renovación: SideStore refirma lo que esté a punto de caducar cuando lo abres
y, cuando iOS le deja, en segundo plano. Para que sea lo más automático
posible: Ajustes → General → Actualización en segundo plano activada (y para
SideStore), no cierres SideStore desde el multitarea, notificaciones de
SideStore activadas. Si el segundo plano falla, te avisa antes de los 7 días:
abres SideStore y renueva solo (My Apps → Refresh All). Si dejas caducar el
propio SideStore, hay que instalarlo otra vez desde el PC (pasos 3 y 4; el
archivo de emparejamiento se conserva).

Límite del Apple ID gratuito: 3 apps sideloaded a la vez (SideStore y
PepoMote son dos; StosVPN viene de la App Store y no cuenta).

## Instalar con AltStore (alternativa: AltServer en el PC)

1. En el PC (Windows): baja **AltServer** de [altstore.io](https://altstore.io)
   e instálalo. Necesita **iTunes** e **iCloud** instalados desde la web de
   Apple (no las versiones de la Microsoft Store): el propio instalador te lo
   dice si faltan.
2. Conecta el iPhone/iPad al PC por cable y, si lo pregunta, «Confía en este
   ordenador».
3. En la bandeja del sistema de Windows, icono de AltServer → **Install
   AltStore** → elige tu dispositivo → escribe tu **Apple ID y contraseña**
   (van directos a Apple; AltStore los usa solo para firmar).
4. En el iPhone/iPad: Ajustes → General → **VPN y gestión de dispositivos** →
   tu Apple ID → **Confiar**. Ya puedes abrir AltStore.
5. Sources → **+** → pega la fuente de arriba → Browse → **PepoMote** →
   **Instalar**.

Renovación: AltStore renueva la firma en segundo plano cuando el
iPhone/iPad está en la misma Wi-Fi que el PC con AltServer abierto. Si pasas
más de 7 días sin coincidir, la app deja de abrirse: abre AltStore → **My
Apps** → **Refresh All** (con AltServer a mano) y vuelve a funcionar. Si ya
tienes AltStore, puedes instalar SideStore desde él (My Apps → «+» →
`SideStore.ipa`) y luego borrar AltStore.

## Instalar con Sideloadly (sin tienda)

1. Baja **Sideloadly** de [sideloadly.io](https://sideloadly.io) e instálalo en
   el PC (Windows o Mac). En Windows también necesita iTunes de la web de
   Apple.
2. Baja `PepoMote.ipa` de la
   [última release](https://github.com/pepitolas13/PepoMote/releases/latest).
3. Conecta el iPhone/iPad por cable, abre Sideloadly, arrastra el IPA a la
   ventana, escribe tu Apple ID y pulsa **Start**; te pedirá la contraseña
   (y el código de dos factores si lo tienes).
4. En el iPhone/iPad: Ajustes → General → VPN y gestión de dispositivos → tu
   Apple ID → **Confiar**.
5. A los 7 días la app deja de abrirse: repite el paso 3 (mismo IPA o el
   nuevo). Sideloadly también puede renovarla sola por Wi-Fi si lo dejas
   abierto en el PC.

## Sin renovaciones: cuenta de desarrollador de pago

Con una cuenta de desarrollador de Apple (99 $/año) la firma dura un año:
instalas el IPA una vez (Sideloadly, AltStore o SideStore) y te olvidas.
Además permite repartir la app por TestFlight (enlace público, sin cable, 90
días por build). TrollStore (instalación permanente sin firmar) solo llega
hasta iOS 16.6.1 / 17.0: no sirve en dispositivos actuales.

## Primer arranque

- **Conectar** → **Escanear QR del PC** → apunta al QR de la ventana de
  PepoMote. iOS pedirá permiso de **cámara** y, al conectar, de **red local**:
  acepta los dos. Si dijiste que no, se cambian en Ajustes → PepoMote.
- Si el PC no aparece en «En tu red», no pasa nada: el QR funciona igual. El
  descubrimiento va por Bonjour (`_pepomote._tcp`), que el receptor anuncia.
- Los PCs quedan guardados en la pantalla Conectar; toca uno para conectar,
  mantén pulsado para olvidarlo.

## Diferencias con Android

- La app tiene que estar **en pantalla**: al irse a segundo plano iOS corta
  la conexión (y al volver se reconecta sola). La pantalla no se apaga
  mientras el mando está abierto.
- Los **botones físicos de volumen** no se pueden usar como A/B: iOS no deja
  a una app capturarlos.
- No hay accesos directos del icono ni tile de ajustes rápidos.
- El GamePad de Wii U se bloquea en apaisado mientras está abierto; el iPad
  usa toda la pantalla (la app no admite Split View).

## Si algo no va

- La fuente da «**decoding failed**» o «no es JSON válido»: la URL apunta a
  la última release y esa release tiene que traer `altstore.json` (desde
  1.5.0). Comprueba que la última release lo lleva entre sus archivos.
- «**No se puede verificar la app**» o no abre pasados unos días: la firma ha
  caducado. SideStore/AltStore → My Apps → Refresh All (o Sideloadly otra vez).
- «**Modo Desarrollador**» necesario: paso 2 de «Antes de nada».
- Conecta pero **el cursor no se mueve**: el receptor está en modo Dolphin o
  Wii U; toca el chip **Puntero**. Si el cursor va al revés en algún eje,
  abre un issue con el modelo del iPhone/iPad: la conversión de ejes de
  CoreMotion a los del protocolo es la misma que la de Android y se ajusta en
  un sitio (`Sensor/MotionEngine.swift`).
- «**El PC no responde**»: permiso de red local en Ajustes → PepoMote, misma
  Wi-Fi, y que el firewall del PC deje el puerto 26761 (el receptor lo abre
  solo en Windows).
- Precisión, scroll, mirilla, cruceta ←/→ (atrás/adelante en el navegador) y
  volumen mantenido: iguales que en Android; ver
  [TROUBLESHOOTING.md](TROUBLESHOOTING.md).

## Para compilarlo tú (opcional)

No hace falta Mac para instalarlo: el IPA lo compila la CI de GitHub
(`.github/workflows/ios.yml`, runner macOS) en cada cambio de `ios/` y en cada
release (junto con `altstore.json`). Si quieres compilarlo a mano en un Mac:

```
brew install xcodegen
cd ios && xcodegen generate && open PepoMote.xcodeproj
```

En Xcode, elige tu equipo de firma (Signing & Capabilities) y ejecuta en tu
dispositivo. Los tests (`Cmd+U`) cubren el codec contra los vectores dorados
del protocolo, el latch de botones, el stick, el teclado de Cemu, el remapeo
de ejes, la reconexión, el routing, los PCs guardados, el canal de pantalla
(con un receptor falso) y la paridad de textos español/inglés.
