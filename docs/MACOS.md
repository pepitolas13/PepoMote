# PepoMote en macOS (beta)

El receptor para Mac es el mismo que el de Windows y Linux: el móvil (Android,
iPhone/iPad o Linux) mueve el cursor del Mac, hace de mando de Wii en Dolphin
y de GamePad de Wii U en Cemu (con la doble pantalla). Funciona en **Mac con
chip Apple (M1 o posterior) y macOS 13 o superior**; no hay versión para Mac
con Intel.

**Beta**: está compilado y probado por la CI en un Mac, pero no lo he podido
probar en uno de verdad. Si algo no va como aquí se cuenta, abre un issue
con el modelo, la versión de macOS y el informe de `--diag` (abajo).

## Instalar

1. Descarga `PepoMote-macOS.dmg` de la
   [última release](https://github.com/pepitolas13/PepoMote/releases/latest).
2. Ábrelo y arrastra **PepoMote** a **Aplicaciones**. Expulsa el disco.
3. Primera apertura. PepoMote no está notarizado por Apple (eso cuesta una
   cuenta de desarrollador de pago), así que macOS lo frena **una vez por
   versión descargada**:
   - **macOS 15 y 26**: al abrirlo sale «Apple no ha podido verificar que
     "PepoMote" no contiene software malicioso». Pulsa **Listo** (no lo
     tires a la papelera), abre **Ajustes del Sistema → Privacidad y
     seguridad**, baja hasta el final y pulsa **Abrir igualmente**, y de nuevo
     **Abrir** (te pedirá la contraseña o Touch ID).
   - **macOS 13 y 14**: haz **Control-clic** sobre PepoMote en Aplicaciones →
     **Abrir** → **Abrir**.
   - Alternativa por terminal (equivale a lo anterior):
     `xattr -d com.apple.quarantine /Applications/PepoMote.app`.
   Para comprobar la descarga: `shasum -a 256 ~/Downloads/PepoMote-macOS.dmg`
   y compáralo con `SHA256SUMS.txt` de la release.
4. Ya se abre como cualquier app. Verás la ventana con el QR y un icono en la
   barra de menús (arriba a la derecha).

## Permisos (la primera vez)

macOS pregunta por cada cosa que PepoMote necesita; la propia ventana te va
diciendo qué falta y tiene un botón que abre el panel de Ajustes exacto.

- **Red local** (macOS 15 o más): al arrancar, «PepoMote quiere buscar y
  conectarse a dispositivos de tu red local». **Permitir**: es como el móvil
  encuentra al Mac. Si dijiste que no: Ajustes → Privacidad y seguridad → Red
  local → PepoMote.
- **Accesibilidad**: hace falta para mover el cursor y pulsar teclas (así
  funciona cualquier app que controle el ratón en macOS). Al primer intento
  sale el diálogo del sistema; si no, la tarjeta de la ventana tiene **Abrir
  Ajustes → Accesibilidad**: marca PepoMote en la lista y vuelve. Se activa
  solo, sin reiniciar: el pie de la ventana pasa a «Inyección: CGEvent».
  Si quitas el permiso, la tarjeta vuelve a salir.
- **Grabación de pantalla**: solo para la **doble pantalla de Cemu** (la
  ventana GamePad View llega al móvil). La tarjeta sale al entrar en modo Wii
  U: **Abrir Ajustes → Grabación de pantalla**, marca PepoMote y pulsa
  **Reiniciar PepoMote** (macOS solo aplica este permiso al reiniciar la
  app). En macOS 15 o más, el sistema puede recordarte una vez al mes que
  PepoMote captura la ventana de Cemu: es normal, **Permitir**.
- **Firewall** (si lo tienes activado): «¿Quieres que PepoMote acepte
  conexiones entrantes?» → **Permitir**.

Gracias a que cada versión va firmada con el mismo certificado, los permisos
se conservan al actualizar: no hay que volver a darlos.

## Cómo va en el Mac

- El **icono de la barra de menús** dice el estado (punto verde con móviles
  conectados) y tiene **Mostrar** y **Salir**; cuando hay una versión nueva,
  también **Nueva versión…** (abre la release en el navegador).
- El **botón rojo** de la ventana la minimiza (sigue en el Dock; clic en el
  Dock o «Mostrar» la devuelve). Para cerrar del todo: ⌘Q o **Salir** en la
  barra de menús.
- **Arrancar con el sistema** (Ajustes de la ventana): crea un LaunchAgent en
  `~/Library/LaunchAgents/dev.pepotech.pepomote.plist` que abre PepoMote
  oculto (solo el icono de la barra de menús) al iniciar sesión. Aparece en
  Ajustes del Sistema → General → Ítems de inicio. Desmarcarlo lo quita.
- **Varios monitores**: el cursor puede cubrirlos todos o solo uno (Ajustes →
  «Apuntado absoluto en»).
- **Cruceta ← / →** en modo puntero = atrás / adelante (⌘[ y ⌘] en Safari,
  Chrome, Firefox y el Finder). Volumen y multimedia, como en Windows.

## Dolphin y Cemu en el Mac

- Igual que en Windows: PepoMote es un servidor DSU en `127.0.0.1:26760` y
  escribe la configuración de mandos de **Dolphin** en
  `~/Library/Application Support/Dolphin/Config` y los perfiles de **Cemu** en
  `~/Library/Application Support/Cemu`. El modo automático detecta Dolphin y
  Cemu abiertos (`/Applications/Dolphin.app`, `Cemu.app`).
- Doble pantalla: en Cemu, Options → **Separate GamePad view**. La ventana
  GamePad View se queda visible en el Mac (aquí no se esconde detrás de la
  principal como en Windows): puedes taparla con otra ventana, pero no
  minimizarla (minimizada no se captura).
- El teclado en pantalla de Cemu: el texto que escribes en el móvil va a la
  app que tenga el foco, así que Cemu tiene que estar delante.

## Si algo no va

- «**Inyección: ninguna**» en el pie de la ventana → falta Accesibilidad (ver
  arriba).
- El móvil no encuentra el Mac → Red local permitida y misma Wi-Fi; el QR
  funciona aunque falle el autodescubrimiento.
- Informe para un issue: `/Applications/PepoMote.app/Contents/MacOS/PepoMote --diag`
  (dice versión de macOS, permisos, inyector, puertos y las últimas líneas del
  log). El log vive en
  `~/Library/Application Support/dev.pepotech.PepoMote/receptor.log`.
- El scroll va al revés: macOS invierte la rueda con «Desplazamiento
  natural» (Ajustes → Ratón/Trackpad). Si te pasa con la tira de scroll del
  móvil, dímelo en un issue: es un cambio de signo.

## Para compilarlo tú (opcional)

```
cd desktop && cargo build --release
bash packaging/macos/bundle.sh      # .app firmado ad hoc + DMG en dist/
```

La CI (`.github/workflows/desktop.yml`, runner `macos-15`) hace lo mismo con
el certificado del repo y deja el DMG como artefacto; en cada release lo
publica como `PepoMote-macOS.dmg`.
