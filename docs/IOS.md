# PepoMote en iPhone y iPad

La app de iOS/iPadOS es el mismo emisor que la de Android: puntero, mando de
Wii para Dolphin, GamePad de Wii U para Cemu (con doble pantalla), Nunchuk,
varios PCs, reconexión automática, español/inglés y tema claro/oscuro.
Funciona en iOS/iPadOS **15 o superior** (desde iPhone 7 hasta los iPad con
iPadOS 26).

No está en la App Store: se instala el archivo `PepoMote.ipa` de las
[releases](https://github.com/pepitolas13/PepoMote/releases) con **AltStore**
(recomendado: se renueva solo) o con **Sideloadly**. En los dos casos el IPA se
firma con tu propio Apple ID; con un Apple ID gratuito la firma dura 7 días
(AltStore la renueva sola; con Sideloadly la renuevas tú con un clic).

## Antes de nada (una sola vez)

1. Instala el **receptor** en el PC como siempre (`PepoMote.exe` en Windows o
   el AppImage en Linux): en su ventana verás el QR.
2. En el iPhone/iPad, con iOS 16 o superior, activa el **modo Desarrollador**:
   Ajustes → Privacidad y seguridad → Modo Desarrollador → activar (pide
   reiniciar). Sin él, iOS no abre las apps instaladas fuera de la App Store.
   En iOS 15 no hace falta.
3. Móvil y PC en la **misma Wi-Fi**.

## Instalar con AltStore (recomendado)

AltStore es una tienda alternativa que firma e instala apps con tu Apple ID y
las **renueva sola** cada 7 días mientras tu PC (con AltServer) esté encendido
en la misma Wi-Fi.

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
5. En AltStore: pestaña **Sources** (Fuentes) → **+** → pega esta URL y acepta:

   ```
   https://github.com/pepitolas13/PepoMote/releases/latest/download/altstore.json
   ```

6. Pestaña **Browse** → **PepoMote** → **Instalar**. Listo. Cuando salga una
   versión nueva, AltStore te la ofrece ahí mismo.

Renovación: AltStore renueva la firma en segundo plano cuando el
iPhone/iPad está en la misma Wi-Fi que el PC con AltServer abierto. Si pasas
más de 7 días sin coincidir, la app deja de abrirse: abre AltStore → **My
Apps** → **Refresh All** (con AltServer a mano) y vuelve a funcionar.

Límites del Apple ID gratuito: 3 apps instaladas a la vez por AltStore y
renovación cada 7 días. Con una cuenta de desarrollador de pago la firma
dura un año.

## Instalar con Sideloadly (alternativa, sin AltServer)

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

- «**No se puede verificar la app**» o no abre pasados unos días: la firma ha
  caducado. AltStore → My Apps → Refresh All (o Sideloadly otra vez).
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
release. Si quieres compilarlo a mano en un Mac:

```
brew install xcodegen
cd ios && xcodegen generate && open PepoMote.xcodeproj
```

En Xcode, elige tu equipo de firma (Signing & Capabilities) y ejecuta en tu
dispositivo. Los tests (`Cmd+U`) cubren el codec contra los vectores dorados
del protocolo, el latch de botones, el stick, el teclado de Cemu, el remapeo
de ejes, la reconexión, el routing, los PCs guardados, el canal de pantalla
(con un receptor falso) y la paridad de textos español/inglés.
