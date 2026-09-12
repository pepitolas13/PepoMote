# PMP v1 — PepoMote Protocol

Versión de protocolo (`pv`): **1**. Todo little-endian. Transporte: UDP para telemetría (caliente), TCP para control (frío). Puerto por defecto del receptor: **26761** (TCP y UDP, mismo número).

Este documento es la fuente de verdad. Los codecs de Android (`PmpCodec.kt`) y del receptor (`net/codec.rs`) se testean contra los vectores dorados de `vectors/*.hex`; si este documento y el código divergen, manda este documento.

## 1. Descubrimiento

1. **mDNS**: el receptor anuncia `_pepomote._tcp.local.` con TXT `pv=1`, `name=<nombre del PC>`. El móvil resuelve con `NsdManager`.
2. **Fallback broadcast**: el móvil emite por UDP al puerto 26761 (broadcast limitado 255.255.255.255 y broadcast dirigido de la subred) el datagrama ASCII `PMPDISCOVER1`. El receptor responde al remitente con `PMPHERE1 <json>` donde json = `{"pv":1,"name":"...","tcp":26761}`.
3. **Manual**: el usuario teclea `IP:puerto`.

## 2. Emparejamiento

El receptor genera un token aleatorio de 128 bits (hex, 32 chars) y lo persiste. Lo muestra como QR:

```
pepomote://pair?v=1&host=<ip>&port=<tcp>&t=<token_hex32>&name=<url-encoded>
```

Alternativa sin cámara (el emisor Linux móvil la usa siempre): el receptor muestra bajo el QR un código de 4 dígitos temporal (TTL 5 min, **un solo uso**; se regenera además tras 5 intentos fallidos); el móvil lo envía en el `hello` como `code` (sin `token`) sobre un receptor descubierto o tecleado; el receptor responde `ok` incluyendo `token` definitivo, que el móvil persiste. Código malo o caducado → `err bad_code`. El `ok` lleva también `name` (nombre del PC). Si el `hello` lleva `"probe":true` es solo una sonda de emparejamiento: el receptor responde `ok` (con `token` y `name`) y cierra, sin asignar slot ni avisar de conexión; el móvil conecta después de verdad con el token.

El token no es criptografía seria: identifica y evita conexiones accidentales en la LAN. Modelo de amenaza documentado en `docs/SECURITY.md`.

## 3. Canal de control (TCP, JSON por líneas)

Una línea UTF-8 = un mensaje JSON terminado en `\n`. El móvil conecta y envía `hello`; el receptor responde `ok` o `err` y la conexión queda abierta como canal de control y latido.

| Mensaje | Dirección | Campos | Respuesta |
|---|---|---|---|
| `hello` | móvil→PC | `{"m":"hello","pv":1,"token":"...","code":"1234"?,"name":"<móvil>","model":"<modelo>","role":"wiimote"\|"nunchuk"?,"pad":"wiimote"?}` | `ok` / `err` |
| `ok` | PC→móvil | `{"m":"ok","session_id":u32,"udp_port":26761,"token":"..."?,"mode":"pointer","slot":0,"role":"wiimote","player":1,"name":"<PC>","modes":["pointer","dolphin","cemu"],"pad":"gamepad"\|"pro"\|"wiimote"\|"nunchuk"}` | — |
| `err` | PC→móvil | `{"m":"err","code":"bad_token"\|"bad_code"\|"bad_version"\|"busy","msg":"..."}` | cerrar |
| `mode` | ambas | `{"m":"mode","mode":"pointer"\|"dolphin"\|"cemu","by":"pc"?}` | eco `mode` como confirmación; el PC lo difunde además a las otras sesiones cuando cambia. Desde 1.4 el PC puede cambiarlo por su cuenta (modo automático: se abrió o cerró Dolphin o Cemu): lo difunde a TODAS las sesiones con `"by":"pc"` seguido de un `notice`, y el móvil aplica el modo descartando sin aviso cualquier petición suya pendiente |
| `pad` | ambas | `{"m":"pad","pad":"wiimote"\|"gamepad"}` (modo Wii U: ser Mando de Wii o GamePad/Pro) | eco `pad` con el tipo efectivo (`gamepad`, `pro`, `wiimote`); el PC lo envía además sin que se lo pidan a cualquier móvil cuyo tipo cambie (J2 pasa a `gamepad` si J1 se va; a un Nunchuk le dice `wiimote` cuando su jugador es Mando de Wii y está en uso, `nunchuk` si no) |
| `notice` | PC→móvil | `{"m":"notice","text":"..."}` aviso legible (p. ej. «Cemu está abierto: ciérralo…») | — (se muestra unos segundos) |
| `text` | móvil→PC | `{"m":"text","text":"..."}` texto para teclear en el PC (`\n` = Intro, `\u0008` = borrar). En modo `cemu` va al teclado en pantalla de Cemu, que solo atiende a teclas (Windows: mensajes de tecla a su ventana; Linux: teclado virtual, solo con Cemu abierto); en los demás modos, a la ventana con el foco | — (un receptor anterior a 1.3 lo ignora) |
| `config` | ambas | `{"m":"config","sensor_hz":u16?,"sens_deg":f32?,...}` solo claves presentes | eco `config` |
| `ping` | ambas | `{"m":"ping","t":u64}` | `{"m":"pong","t":<mismo t>}` |
| `bye` | ambas | `{"m":"bye"}` | cerrar |

Latido: `ping` TCP cada 1 s si no hay tráfico. Sesión muerta a los 5 s sin nada (TCP ni UDP): el receptor libera el slot y descarta el `session_id`.

**Multijugador (desde pv=1, cambio aditivo):** hasta 4 sesiones simultáneas. El receptor asigna a cada `hello` el slot libre más bajo y lo devuelve en `ok.slot` (0 = Jugador 1). Con los 4 ocupados, `err busy`. El mensaje `mode` solo tiene efecto desde el slot 0 (a los demás se les responde con el modo vigente). En modo puntero solo inyecta el slot 0; en modo Dolphin cada sesión alimenta su slot DSU homónimo (0..3), cada uno con su MAC (`"PMP1"+0x00+slot`) y su pulso de recentrado propio.

**Wii U / Cemu (desde pv=1, cambio aditivo):** modo `cemu`. Como en `dolphin`, el receptor no inyecta nada en el SO y cada sesión alimenta su pad DSU, pero con el perfil Wii U (`protocol/DSU.md`) y configurando Cemu en vez de Dolphin. El Jugador 1 es el **GamePad** y los demás **Pro Controller**; cualquier móvil puede pedir ser **Mando Wii** (`pad`), y entonces su puntero IR (motor de puntero del receptor) y su Nunchuk llegan a Cemu. `ok.modes` anuncia los modos del receptor: un móvil no ofrece Wii U si falta `cemu` (un receptor antiguo contesta `pointer` al pedir `cemu`, y el móvil se queda en el layout Wii). Mientras el receptor ha confirmado `cemu` y el móvil actúa como GamePad/Pro, el `INPUT` lleva el bloque de extensión (§4.1) con el stick derecho y la pantalla táctil; en cualquier otro caso mide 72 bytes. Los mensajes con `m` desconocido se ignoran.

**Nunchuk (desde pv=1, cambio aditivo):** un segundo móvil en la otra mano. El `hello` lleva `"role":"nunchuk"` (ausente o `"wiimote"` = mando). Los Wiimotes ocupan slots desde el 0 hacia arriba y los Nunchuks desde el 3 hacia abajo; el Nunchuk i-ésimo (por slot descendente) va emparejado con el Wiimote i-ésimo (por slot ascendente) y el `ok` lo dice en `player` (1..4, el jugador al que pertenece; en un Wiimote, `player` = `slot`+1). Un Nunchuk nunca inyecta puntero ni cambia el modo; en modo Dolphin alimenta su slot DSU con el stick (bytes 6-7 del INPUT), C/Z y su acelerómetro, y el receptor configura el Wiimote emulado del jugador con `Extension = Nunchuk` leyendo de ese slot (`protocol/DSU.md`).

## 4. Telemetría (UDP, binario)

### 4.1 `INPUT` móvil→receptor — 72 bytes (80 con el bloque Wii U)

| off | tam | tipo | campo |
|---|---|---|---|
| 0 | 4 | u32 | magic `0x31504D50` (ASCII "PMP1") |
| 4 | 1 | u8 | tipo = `0x01` |
| 5 | 1 | u8 | flags: bit0 = quaternion válido (el móvil tiene GAME_ROTATION_VECTOR); bit1 = stick válido (Nunchuk o stick izquierdo del GamePad); bit2 = `FLAG_EXT`, el paquete mide 80 bytes y trae el bloque Wii U; bit3 = `FLAG_TOUCH`, hay un dedo en la pantalla táctil del GamePad; resto reservado 0 |
| 6 | 1 | i8 | `stick_x` (Nunchuk o stick izquierdo): −127..127, + = derecha (0 en un Wiimote) |
| 7 | 1 | i8 | `stick_y` (Nunchuk o stick izquierdo): −127..127, + = arriba (0 en un Wiimote) |
| 8 | 4 | u32 | `session_id` (del `ok`) |
| 12 | 4 | u32 | `seq` monótono con wrap; el receptor descarta paquetes con `seq` ≤ último visto (ventana de wrap 2³¹) |
| 16 | 8 | u64 | `t_sensor_us`: `SensorEvent.timestamp` de la muestra de gyro, en µs (ns/1000) |
| 24 | 16 | f32×4 | quaternion `w,x,y,z` (`GAME_ROTATION_VECTOR`, normalizado, marco del dispositivo) |
| 40 | 12 | f32×3 | gyro `x,y,z` rad/s (ejes Android del dispositivo, calibrado) |
| 52 | 12 | f32×3 | accel `x,y,z` m/s² con gravedad incluida (ejes Android) |
| 64 | 4 | u32 | botones (bitmask, tabla 4.2) |
| 68 | 1 | u8 | `recenter_count`: incrementa con cada pulsación de recentrado; el receptor actúa al detectar el cambio |
| 69 | 1 | u8 | batería 0-100 |
| 70 | 2 | i16 | `touch_scroll_dy`: píxeles acumulados de la tira de scroll desde el paquete anterior (+ = dedo hacia arriba = scroll up) |
| 72 | 1 | i8 | **bloque Wii U (solo con bit2):** `stick_rx`, stick derecho, + = derecha |
| 73 | 1 | i8 | `stick_ry`, stick derecho, + = arriba |
| 74 | 2 | u16 | `touch_x`: fracción horizontal de la pantalla táctil del GamePad, 0 = izquierda … 65535 = derecha (válido con bit3) |
| 76 | 2 | u16 | `touch_y`: fracción vertical, 0 = arriba … 65535 = abajo |
| 78 | 2 | — | reservado 0 |

El bloque Wii U solo se envía cuando el receptor ha confirmado el modo `cemu` (§3): un receptor antiguo descarta cualquier `INPUT` que no mida 72 bytes, y nunca confirma `cemu`, así que nunca lo recibe.

Cadencia: la del sensor (típico 100-200 Hz), tope 250 Hz, mínimo keepalive 1 Hz aunque no cambie nada. Cada paquete lleva el estado completo de botones: la pérdida de un paquete nunca deja un botón atascado.

### 4.2 Bitmask de botones

| bit | botón | modo puntero (interpretación del receptor) |
|---|---|---|
| 0 | A | clic izquierdo (mantener = arrastrar) |
| 1 | B (gatillo) | clic derecho |
| 2 | cruceta ↑ | flecha ↑ |
| 3 | cruceta ↓ | flecha ↓ |
| 4 | cruceta ← | flecha ← |
| 5 | cruceta → | flecha → |
| 6 | Plus | volumen + |
| 7 | Minus | volumen − |
| 8 | Home | (local en el móvil: abre su menú; se envía igualmente) |
| 9 | Uno | Enter |
| 10 | Dos | Esc |
| 11 | media vol+ | volumen + |
| 12 | media vol− | volumen − |
| 13 | media mute | mute |
| 14 | media play/pausa | play/pausa |
| 15 | media next | siguiente pista |
| 16 | media prev | pista anterior |
| 17 | C (Nunchuk) | — (solo Dolphin / Cemu) |
| 18 | Z (Nunchuk) | — (solo Dolphin / Cemu) |
| 19 | X (Wii U) | — (solo Cemu) |
| 20 | Y (Wii U) | — (solo Cemu) |
| 21 | L | — (solo Cemu) |
| 22 | R | — (solo Cemu) |
| 23 | ZL | — (solo Cemu) |
| 24 | ZR | — (solo Cemu) |
| 25 | click stick izquierdo | — (solo Cemu) |
| 26 | click stick derecho | — (solo Cemu) |
| 27 | soplar al micrófono del GamePad | — (solo Cemu) |
| 28 | pantalla TV↔GamePad (función de Cemu) | — (solo Cemu) |
| 29 | precisión (mantener) | el cursor se mueve al 40 % mientras se mantiene (desde 1.4; en Dolphin / Cemu se ignora) |

En modo `dolphin` y en modo `cemu` el receptor NO inyecta nada en el SO: todo el estado va al servidor DSU (mapeo en `protocol/DSU.md`).

### 4.3 `PING`/`PONG` UDP (RTT del hot path, para los HUD)

Mismo socket UDP, **bidireccional**. Layout (20 bytes): bytes 0-7 como INPUT (magic, tipo, flags, reservado), bytes 8-11 `session_id`, bytes 12-19 u64 `t_envio_us` (reloj del emisor del PING). Tipos: `0x02` PING, `0x03` PONG.

Regla: quien recibe un PING responde un PONG con el mismo cuerpo (solo cambia el tipo). Quien recibe un PONG con su `session_id` calcula `RTT = ahora − t_envio_us` con su propio reloj. El móvil pinguea a 1 Hz (HUD del mando); el receptor pinguea a 2 Hz (HUD de la ventana).

### 4.4 Canal de pantalla (doble pantalla del GamePad, desde 1.3, aditivo)

El móvil que hace de GamePad recibe la pantalla del GamePad de Cemu (la ventana «GamePad View», que el receptor captura y comprime en JPEG) por una conexión TCP APARTE al mismo puerto PMP:

1. Primera línea del móvil: `{"m":"screen","session_id":u32,"w":854,"h":480,"q":70}` (`w`/`h` = tamaño máximo, se conserva la relación de aspecto; `q` = calidad JPEG, opcional).
2. Respuesta: `{"m":"screen","ok":true}` o `{"m":"err","code":"bad_session","msg":"…"}` (sesión desconocida o que no es un mando). Un receptor sin canal de pantalla no contesta: el móvil lo trata como «sin pantalla» a los 3 s.
3. Después, solo tramas binarias del receptor al móvil: `"PMPS"` (4 bytes) · `tipo` u8 · `longitud` u32 LE · carga. Tipo 1 = imagen JPEG; tipo 2 = estado en texto UTF-8 («Cemu no está abierto», «Abre la vista del GamePad en Cemu…»); tipo 3 = keepalive sin carga (cada 2 s si no hay nada que mandar).
4. Control de flujo: tras pintar cada imagen el móvil envía un byte `0x01`; el receptor no manda la siguiente hasta recibirlo (una imagen en vuelo: el ritmo se adapta al Wi-Fi y al móvil, la latencia no se acumula) y siempre manda la captura más reciente. Sin confirmación en 10 s el receptor cierra; sin nada en 6 s el móvil reconecta.

El receptor captura solo mientras hay algún móvil suscrito, a 30 fps como máximo y descartando fotogramas idénticos; la captura es de la ventana (Windows: Windows.Graphics.Capture, recortada al área cliente, con `PrintWindow` de respaldo; Linux: X11/XWayland con Composite — con Cemu nativo en Wayland no hay ventana X que capturar). Una captura uniforme (toda blanca o negra) aislada entre dos buenas se descarta: es un fallo de captura, no una imagen.

## 5. Versionado

`pv` va en `hello` y en el TXT de mDNS. Mismo `pv` = compatible. `pv` distinto → `err bad_version` con `msg` legible ("Actualiza PepoMote en el PC/móvil"). El paquete `INPUT` de 72 bytes no cambia dentro de pv=1; el bloque Wii U es un añadido opcional que solo se emite hacia receptores que lo anuncian. Cambios de layout = pv=2.

## 6. Vectores dorados (`vectors/`)

Cada `.hex` es un paquete completo en hex ASCII (sin espacios) + un `.json` hermano con los valores decodificados esperados. Tests: Kotlin (`PmpCodecTest`) y Rust (`codec::tests`) parsean el mismo hex y comparan contra el json. Vectores mínimos: `input_neutral`, `input_buttons_all`, `input_motion`, `input_nunchuk` (flags 0x03, stick (100, −50), C+Z), `input_wiiu` (80 bytes, flags 0x0F, sticks (100, −50) y (−30, 120), táctil (0x8000, 0x4000), botones 0x1FF80001), `ping`, `pong`. Los valores esperados están en los propios tests (Rust y Kotlin), no hay `.json` hermanos.
