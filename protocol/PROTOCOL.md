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
| `hello` | móvil→PC | `{"m":"hello","pv":1,"token":"...","code":"1234"?,"name":"<móvil>","model":"<modelo>","role":"wiimote"\|"nunchuk"?,"pad":"wiimote"\|"retropad"\|"nes"\|"gun"?,"nunchuk":"own"?,"screen_only":true?,"layout":"md"?}` | `ok` / `err` |
| `ok` | PC→móvil | `{"m":"ok","session_id":u32,"udp_port":26761,"token":"..."?,"mode":"pointer","slot":0,"role":"wiimote","player":1,"name":"<PC>","modes":["pointer","dolphin","cemu","switch","retroarch"],"pad":"gamepad"\|"pro"\|"wiimote"\|"nunchuk"\|"retropad"\|"nes"\|"gun","half":null,"side":null,"nunchuk":"own"\|"none","screen_only":true\|false,"tilt":true?}` | — |
| `err` | PC→móvil | `{"m":"err","code":"bad_token"\|"bad_code"\|"bad_version"\|"busy","msg":"..."}` | cerrar |
| `mode` | ambas | `{"m":"mode","mode":"pointer"\|"dolphin"\|"cemu"\|"switch"\|"retroarch","by":"pc"?}` | eco `mode` como confirmación; el PC lo difunde además a las otras sesiones cuando cambia. Desde 1.4 el PC puede cambiarlo por su cuenta (modo automático: se abrió Dolphin, Cemu, Eden o RetroArch, o se cerró con `return_to_pointer` activo): lo difunde a TODAS las sesiones con `"by":"pc"` seguido de un `notice`, y el móvil aplica el modo descartando sin aviso cualquier petición suya pendiente |
| `pad` | ambas | `{"m":"pad","pad":"wiimote"\|"gamepad"\|"pro"\|"retropad"\|"nes"\|"gun","layout":"md"?}` | eco con tipo efectivo y metadatos `half`, `side` y `player`; también se difunde cuando cambia el reparto. Tipos Wii U: `wiimote`, `gamepad`, `pro`; tipo Switch: `pro` (los nombres antiguos se migran a Pro); tipos RetroArch: `retropad`, `nes`, `gun` (otro nombre en ese modo se ignora y se devuelve el vigente). Un Nunchuk recibe `wiimote` si está en uso en Wii U y `nunchuk` si no. Desde 1.10 el móvil puede añadir `layout` (id de plantilla de consola de `protocol/retro-layouts.json`: `nes`, `gb`, `gba`, `snes`, `ms`, `md`, `md3`, `pce`, `arcade`, `neogeo`, `atari2600`, `n64`, `psx` o `retropad`), que el receptor solo usa para su ventana y devuelve en el eco como `layout` (la misma cadena, o `null` si falta o no es válida). |
| `hotkey` | móvil→PC | `{"m":"hotkey","name":"save_state","down":true}` tecla rápida de RetroArch por su nombre: `menu`, `save_state`, `load_state`, `slot_plus`, `slot_minus`, `fast_forward` (mantener), `fast_forward_toggle`, `rewind` (mantener), `slow_motion` (mantener), `pause`, `frame_advance`, `reset`, `screenshot`, `fullscreen`, `mute`, `volume_up`, `volume_down`, `close_content`, `quit`, `shader_next`, `shader_prev`, `shader_toggle`, `disk_eject`, `disk_next`, `disk_prev`, `osk`, `game_focus`, `grab_mouse`, `turbo_fire`. Las de mantener llevan `down` true al pulsar y false al soltar; las demás disparan con `down` true (ausente = true) | eco `{"m":"hotkey","name":…,"down":…,"ok":true\|false}` (`ok` false: nombre desconocido, fuera del modo RetroArch o rol Nunchuk). Solo desde 1.10; un receptor anterior lo ignora |
| `nunchuk` | ambas | `{"m":"nunchuk","own":true\|false}` (modo Dolphin: el mando lleva su propio Nunchuk en el mismo móvil) | eco `nunchuk` con el valor efectivo (`false` siempre para un móvil con rol `nunchuk`); si cambia, el PC reconfigura Dolphin (con Dolphin abierto queda pendiente hasta cerrarlo) |
| `screen_only` | ambas | `{"m":"screen_only","on":true\|false}` (modo Wii U: el móvil GamePad solo hace de pantalla táctil a pantalla completa; el mando real del usuario sigue siendo el Controller 1 de Cemu) | eco `screen_only` con el valor efectivo (`false` siempre para rol `nunchuk`); si cambia, el PC reconfigura Cemu (abierto: pendiente hasta cerrarlo) |
| `notice` | PC→móvil | `{"m":"notice","text":"..."}` aviso legible (p. ej. «Cemu está abierto: ciérralo…»), en el idioma del PC (desde 1.4: español o inglés) | — (se muestra unos segundos) |
| `game` | PC→móvil | `{"m":"game","console":"md"\|null,"system":"Mega Drive","core":"Genesis Plus GX","title":"Cave Story","path":"C:\\...\\cave_story.zip"}` el juego que RetroArch tiene cargado según su historial (`content_history.lpl`, que RetroArch escribe al cargar contenido) y la ficha de su núcleo (`info/*.info`): `console` es un id de `protocol/retro-layouts.json` (`nes`, `gb`, `gba`, `snes`, `ms`, `md`, `pce`, `arcade`, `neogeo`, `atari2600`, `n64`, `psx`) o `null` si no se conoce; `path` es la ruta local del contenido (clave para recordar la plantilla elegida por juego). Las cinco claves van siempre (vacías sin juego). Se envía justo después de `ok` en cualquier modo y se difunde a todas las sesiones cuando cambia; con RetroArch cerrado se conserva el último. El móvil enseña el mando de esa consola en modo RetroArch (o el RetroPad completo si `console` es `null` o no lo conoce). Desde 1.10; un móvil anterior lo ignora | — |
| `text` | móvil→PC | `{"m":"text","text":"..."}` texto para teclear en el PC (`\n` = Intro, `\u0008` = borrar). En modo `cemu` va al teclado en pantalla de Cemu, que solo atiende a teclas (Windows: mensajes de tecla a su ventana; Linux: teclado virtual, solo con Cemu abierto); en `switch` activa Eden antes de inyectar Unicode en Windows/macOS (en Linux Eden debe estar en primer plano); en `retroarch` igual con la ventana de RetroArch; en los demás modos, a la ventana con el foco. Desde 1.10.5: como mucho 1024 caracteres por mensaje y 16 mensajes en cola (lo que sobra se recorta o se rechaza, avisando), y lo que el sistema no pueda teclear vuelve como `notice` al móvil que lo escribió, en vez de perderse en silencio | — (un receptor anterior a 1.3 lo ignora) |
| `config` | ambas | `{"m":"config","sensor_hz":u16?,"sens_deg":f32?,...}` solo claves presentes | eco `config` |
| `ping` | ambas | `{"m":"ping","t":u64}` | `{"m":"pong","t":<mismo t>}` |
| `bye` | ambas | `{"m":"bye"}` | cerrar |

Latido: `ping` TCP cada 1 s si no hay tráfico. Sesión muerta a los 5 s sin nada (TCP ni UDP): el receptor libera el slot y descarta el `session_id`.

**Multijugador (desde pv=1, cambio aditivo):** hasta 4 sesiones simultáneas. El receptor asigna a cada `hello` el slot libre más bajo y lo devuelve en `ok.slot` (0 = Jugador 1). Con los 4 ocupados, `err busy`. El mensaje `mode` solo tiene efecto desde el slot 0 (a los demás se les responde con el modo vigente). En modo puntero solo inyecta el slot 0; en modo Dolphin cada sesión alimenta su slot DSU homónimo (0..3), cada uno con su MAC (`"PMP1"+0x00+slot`) y su pulso de recentrado propio.

**Wii U / Cemu (desde pv=1, cambio aditivo):** modo `cemu`. Como en `dolphin`, el receptor no inyecta nada en el SO y cada sesión alimenta su pad DSU, pero con el perfil Wii U (`protocol/DSU.md`) y configurando Cemu en vez de Dolphin. El Jugador 1 es el **GamePad** y los demás **Pro Controller**; cualquier móvil puede pedir ser **Mando Wii** (`pad`), y entonces su puntero IR (motor de puntero del receptor) y su Nunchuk llegan a Cemu. El mando 1 de Cemu es siempre un Wii U GamePad con dispositivo (Cemu lo necesita): si el Jugador 1 pide Mando Wii, el receptor deja ahí un GamePad manejado por el teclado del PC (o el mando real del usuario) y los móviles ocupan los mandos 2 en adelante, sin que cambie su número de jugador ni su `pad`; varios móviles pueden ser Mando Wii a la vez. `ok.modes` anuncia los modos del receptor: un móvil no ofrece Wii U si falta `cemu` (un receptor antiguo contesta `pointer` al pedir `cemu`, y el móvil se queda en el layout Wii). Mientras el receptor ha confirmado `cemu` y el móvil actúa como GamePad/Pro, el `INPUT` lleva el bloque de extensión (§4.1) con el stick derecho y la pantalla táctil; en Switch también se usa la extensión cuando ese modo está confirmado; en los demás casos mide 72 bytes. Los mensajes con `m` desconocido se ignoran.

**Switch / Eden (pv=1, cambio aditivo):** `ok.modes` anuncia `switch`. Un móvil solo ofrece los controles Switch y envía su extensión tras confirmar ese modo. El único tipo de mando es `pro`. Ejemplo de eco: `{"m":"pad","pad":"pro","half":null,"side":null,"player":1}`. Cada móvil con rol de mando es un jugador independiente, numerado por orden de slot; el rol Nunchuk no ocupa jugador en Switch. Solo el slot 0 cambia el modo general.

Los valores `joycons`, `joycon_side` y `joycon_r` se aceptan exclusivamente como migración de preferencias antiguas, tanto en `hello` como en `pad`: se normalizan a `pro` y el receptor devuelve `pro`. Nunca se emparejan móviles ni se configura un Joy-Con. `half` y `side` se mantienen a null en las respuestas para limpiar clientes de pruebas anteriores. Los tipos de otras consolas se rechazan devolviendo el tipo efectivo.

En Switch, bit 28 es Capturar; no hay micrófono, pantalla remota ni táctil. El receptor neutraliza el táctil aunque reciba datos antiguos. Los móviles limpian botones, sticks y gestos al cambiar de modo o diseño. La preferencia de mando Wii U se conserva al entrar y salir de Switch. Ver [DSU.md](DSU.md) para el perfil y [SETUP-SWITCH.md](../docs/SETUP-SWITCH.md) para Eden.

**RetroArch (desde 1.10, cambio aditivo):** `ok.modes` anuncia `retroarch`. En ese modo el receptor no usa el DSU ni inyecta botones en el SO: traduce cada `INPUT` al **mando en red** de RetroArch (UDP `127.0.0.1:55400 + slot`, un datagrama `remote_message` de 20 bytes por control, dosificado a uno por jugador y fotograma con el reloj de la propia interfaz de comandos de RetroArch; `desktop/src/retroarch/`) y las teclas rápidas al **puerto de comandos** (`127.0.0.1:55355`). Tres mandos por móvil (`pad`): `retropad` (el apaisado de dos sticks, mismo `INPUT` de 80 bytes que en Switch: A/B/X/Y → A/B/X/Y del RetroPad, L/R, ZL/ZR → L2/R2, clics de stick → L3/R3, − / + → Select/Start, cruceta y los dos sticks; Home = menú de RetroArch, bit 28 = avance rápido mientras se mantiene), `nes` (el Mando de Wii de lado, 72 bytes: 1 → B, 2 → A, A → X, B → Y, − / +, cruceta tal como se ve en pantalla —sin el giro de Dolphin—, Home = menú) y `gun` (el Mando de Wii apuntando: el puntero mueve el ratón del SO como en modo puntero, B = clic izquierdo (gatillo), A = clic derecho (recarga), y 1, 2, − / +, cruceta y Home como en `nes`; solo apunta el slot 0). Cada móvil con rol de mando es un jugador (slot + 1); el rol Nunchuk no tiene papel. Los bits 17, 18, 27, 29 (salvo en `gun`) y 30 se ignoran. El receptor escribe `retroarch.cfg` con RetroArch cerrado (`docs/SETUP-RETROARCH.md`). El receptor nunca pregunta el estado a RetroArch 1.22.2 ni anteriores (`GET_STATUS` los cierra); qué juego corre lo lee de su historial y lo anuncia con `game` (arriba), y el móvil elige con ello una plantilla de consola: todas las plantillas mandan el mismo `INPUT` de 80 bytes del `retropad`, y cada botón en pantalla emite el bit cuyo id del RetroPad espera el mapeo por defecto del núcleo (p. ej. Mega Drive A/B/C → bits Y/B/A); la tabla completa está en `protocol/retro-layouts.json`, generada desde `pmp::retro`. El **servidor Android** de esta integración también anuncia `retroarch`: mismo mando en red (`127.0.0.1:55400 + slot`) y mismos comandos (55355) contra el RetroArch del propio móvil (`android/app/src/main/java/dev/pepotech/pepomote/server/retro/`); `hello.pad` y `pad` solo aceptan `retropad` y `nes` (`gun` devuelve el mando vigente y un `notice`: no hay ratón); `game` sale del historial de la carpeta `RetroArch` elegida por SAF (sin ficha `.info`: consola por núcleo, extensión y `db_name`); y el modo es automático: cuando RetroArch empieza a responder, `mode retroarch` con `by:"pc"` a todas las sesiones, y cuando un emulador nuevo pide mandos por DSU con RetroArch mudo, vuelve al último modo con `by:"pc"`.

**Nunchuk (desde pv=1, cambio aditivo):** un segundo móvil en la otra mano. El `hello` lleva `"role":"nunchuk"` (ausente o `"wiimote"` = mando). Los Wiimotes ocupan slots desde el 0 hacia arriba y los Nunchuks desde el 3 hacia abajo; el Nunchuk i-ésimo (por slot descendente) va emparejado con el Wiimote i-ésimo (por slot ascendente) y el `ok` lo dice en `player` (1..4, el jugador al que pertenece; en un Wiimote, `player` = `slot`+1). Un Nunchuk nunca inyecta puntero ni cambia el modo; en modo Dolphin alimenta su slot DSU con el stick (bytes 6-7 del INPUT), C/Z y su acelerómetro, y el receptor configura el Wiimote emulado del jugador con `Extension = Nunchuk` leyendo de ese slot (`protocol/DSU.md`).

**Nunchuk en el mismo móvil (desde 1.5.5, cambio aditivo):** un mando (rol `wiimote`) puede llevar su propio Nunchuk: lo anuncia en el `hello` con `"nunchuk":"own"` o después con el mensaje `nunchuk`, y el `ok` lo confirma en `nunchuk` (`"own"`; `"none"` si no, y un receptor anterior no manda el campo: el móvil no debe enseñar el trazado con Nunchuk sin esa confirmación, porque el receptor antiguo interpreta C/Z como A/B). Con él, su `INPUT` lleva también el stick (bytes 6-7, `FLAG_STICK_VALID`) y los bits C/Z, todo en la misma trama de 72 bytes, y en modo Dolphin el receptor configura el Wiimote emulado con `Extension = Nunchuk` leyendo del **mismo** pad DSU (C→L1, Z→R1, así no chocan con A/B). Si además hay un Nunchuk de otro móvil emparejado a ese jugador, queda sin uso. En modo Wii U (Cemu) el Nunchuk propio no se aplica de momento.

**Solo pantalla (desde 1.6, cambio aditivo):** en modo Wii U un mando puede anunciar que solo hace de pantalla táctil del GamePad (`"screen_only":true` en el `hello`, o el mensaje `screen_only`): el usuario juega con un mando real conectado al PC y el móvil enseña la pantalla del GamePad a pantalla completa. El `ok` lo confirma en `screen_only` (`true`/`false`; un receptor anterior no manda el campo ni el eco: el móvil enseña la pantalla completa igual y avisa una vez de que el receptor es antiguo). Con él, el receptor no sustituye el perfil `controller0.xml` del usuario: lo **fusiona**, añadiendo el DSU del móvil como segundo mando del GamePad emulado sin movimiento ni botones (el táctil viene implícito con el DSU), con copia `.pepomote.bak` del perfil del usuario, que vuelve cuando el móvil se va o apaga la opción. Sin perfil del usuario, el receptor escribe el perfil completo de siempre. Si el Controller 1 del usuario no es un GamePad, el receptor avisa al móvil (`notice`) de que el táctil no se aplicará. Solo el jugador 1 GamePad puede ser «solo pantalla»; en un Pro o un Mando de Wii no aplica. El canal de pantalla (§4.4) no cambia.

## 4. Telemetría (UDP, binario)

### 4.1 `INPUT` móvil→receptor — 72 bytes (80 con el bloque extendido Wii U / Switch)

| off | tam | tipo | campo |
|---|---|---|---|
| 0 | 4 | u32 | magic `0x31504D50` (ASCII "PMP1") |
| 4 | 1 | u8 | tipo = `0x01` |
| 5 | 1 | u8 | flags: bit0 = quaternion válido (el móvil tiene GAME_ROTATION_VECTOR); bit1 = stick válido (Nunchuk o stick izquierdo del GamePad); bit2 = `FLAG_EXT`, el paquete mide 80 bytes y trae el bloque extendido; bit3 = `FLAG_TOUCH`, hay un dedo en la pantalla táctil del GamePad; bit4 = `FLAG_TILT`, apuntado por inclinación (el receptor saca el cursor del acelerómetro; solo con `ok.tilt`); resto reservado 0 |
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
| 72 | 1 | i8 | **bloque extendido (solo con bit2):** `stick_rx`, stick derecho, + = derecha |
| 73 | 1 | i8 | `stick_ry`, stick derecho, + = arriba |
| 74 | 2 | u16 | `touch_x`: fracción horizontal de la pantalla táctil del GamePad, 0 = izquierda … 65535 = derecha (válido con bit3) |
| 76 | 2 | u16 | `touch_y`: fracción vertical, 0 = arriba … 65535 = abajo |
| 78 | 2 | — | reservado 0 |

El bloque extendido solo se envía cuando el receptor ha confirmado `cemu` o `switch` (§3), para un tipo de mando que lo use. Un receptor anterior a estas capacidades nunca confirma esos modos y sigue recibiendo 72 bytes. En Switch, `FLAG_TOUCH` permanece a cero y los bytes 74–77 no representan ninguna pantalla.

Cadencia: la del sensor cuando está disponible, con tope de 250 Hz. La conexión transmite también sin giroscopio: un temporizador independiente mantiene los botones y sticks a aproximadamente 100 Hz. Al faltar o detenerse el sensor se envía gyro cero; la orientación es estable o neutra y el acelerómetro disponible se conserva. El temporizador termina al detener la sesión. Cada paquete lleva el estado completo de botones: la pérdida de un paquete nunca deja un botón atascado.

**Apuntado por inclinación (`FLAG_TILT`, bit4; cambio aditivo):** un móvil sin giroscopio real (solo acelerómetro, o un giroscopio «virtual» derivado de él que no ve el giro sobre la vertical) no puede dar el yaw que necesita el puntero: el cursor solo subiría y bajaría. Con bit4 el receptor ignora quat y gyro para el cursor y lo saca del acelerómetro (bytes 52-63): horizontal = roll (inclinar a los lados; borde derecho hacia abajo = derecha), vertical = pitch, los dos referidos a la gravedad. El quaternion, el gyro y bit0 se siguen enviando exactamente igual (los pads DSU no cambian). El móvil solo pone bit4 si el `ok` trajo `"tilt":true`: un receptor anterior no lo anuncia y el servidor Android descarta flags desconocidos. En Android lo decide el ajuste del móvil (giroscopio / acelerómetro) y, si no se ha elegido, la detección del giroscopio real; en Linux móvil, la ausencia de giroscopio.

### 4.2 Bitmask de botones

| bit | botón | modo puntero (interpretación del receptor) |
|---|---|---|
| 0 | A | clic izquierdo (mantener = arrastrar) |
| 1 | B (gatillo) | clic derecho |
| 2 | cruceta ↑ | flecha ↑ |
| 3 | cruceta ↓ | flecha ↓ |
| 4 | cruceta ← | atrás en el navegador (antes de 1.5.0: flecha ←) |
| 5 | cruceta → | adelante en el navegador (antes de 1.5.0: flecha →) |
| 6 | Plus | volumen + (mantener = repite: el receptor re-toca la tecla cada 100 ms a partir de los 350 ms) |
| 7 | Minus | volumen − (mantener = repite) |
| 8 | Home | (local en el móvil: abre su menú; se envía igualmente) |
| 9 | Uno | Enter |
| 10 | Dos | Esc |
| 11 | media vol+ | volumen + (mantener = repite) |
| 12 | media vol− | volumen − (mantener = repite) |
| 13 | media mute | mute |
| 14 | media play/pausa | play/pausa |
| 15 | media next | siguiente pista |
| 16 | media prev | pista anterior |
| 17 | C (Nunchuk) | — (solo Dolphin / Cemu) |
| 18 | Z (Nunchuk) | — (solo Dolphin / Cemu) |
| 19 | X (Wii U / Switch) | — (Cemu / Switch) |
| 20 | Y (Wii U / Switch) | — (Cemu / Switch) |
| 21 | L | — (Cemu / Switch) |
| 22 | R | — (Cemu / Switch) |
| 23 | ZL | — (Cemu / Switch) |
| 24 | ZR | — (Cemu / Switch) |
| 25 | click stick izquierdo | — (Cemu / Switch) |
| 26 | click stick derecho | — (Cemu / Switch) |
| 27 | soplar al micrófono del GamePad | — (solo Cemu) |
| 28 | TV↔GamePad en Cemu / Capturar en Switch | — (Cemu / Switch) |
| 29 | precisión (mantener) | el cursor se mueve al 40 % mientras se mantiene (desde 1.4; en Dolphin / Cemu se ignora) |
| 30 | acercar (mantener) | en Dolphin acerca el Mando de Wii emulado a la pantalla: separa los puntos IR en 4 niveles en rampa mientras se mantiene (juegos que piden acercar el mando; desde 1.8.5, Dolphin 2407+); en puntero / Cemu / Switch se ignora |

En `dolphin`, `cemu` y `switch` las entradas del mando van al servidor DSU, sin inyección de botones o movimiento en el SO; en `retroarch` van al mando en red de RetroArch (§3, «RetroArch»), y solo la pistola (`gun`) del slot 0 mueve el ratón; el mensaje `text` es un canal explícito de teclado aparte (mapeo en `protocol/DSU.md`).

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

`pv` va en `hello` y en el TXT de mDNS. Mismo `pv` = compatible. `pv` distinto → `err bad_version` con `msg` legible ("Actualiza PepoMote en el PC/móvil"). El paquete `INPUT` de 72 bytes no cambia dentro de pv=1; el bloque extendido Wii U / Switch es un añadido opcional que solo se emite hacia receptores que lo anuncian. Cambios de layout = pv=2.

## 6. Vectores dorados (`vectors/`)

Cada `.hex` es un paquete completo en hex ASCII (sin espacios) + un `.json` hermano con los valores decodificados esperados. Tests: Kotlin (`PmpCodecTest`) y Rust (`codec::tests`) parsean el mismo hex y comparan contra el json. Vectores mínimos: `input_neutral`, `input_buttons_all`, `input_motion`, `input_nunchuk` (flags 0x03, stick (100, −50), C+Z), `input_wiiu` (80 bytes, flags 0x0F, sticks (100, −50) y (−30, 120), táctil (0x8000, 0x4000), botones 0x1FF80001), `ping`, `pong`. Los valores esperados están en los propios tests (Rust y Kotlin), no hay `.json` hermanos.
