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

Alternativa sin cámara (el emisor Linux móvil la usa siempre): el receptor muestra bajo el QR un código de 4 dígitos temporal (TTL 120 s, **un solo uso**; se regenera además tras 5 intentos fallidos); el móvil lo envía en el `hello` como `code` (sin `token`) sobre un receptor descubierto o tecleado; el receptor responde `ok` incluyendo `token` definitivo, que el móvil persiste. Código malo o caducado → `err bad_code`. El `ok` lleva también `name` (nombre del PC).

El token no es criptografía seria: identifica y evita conexiones accidentales en la LAN. Modelo de amenaza documentado en `docs/SECURITY.md`.

## 3. Canal de control (TCP, JSON por líneas)

Una línea UTF-8 = un mensaje JSON terminado en `\n`. El móvil conecta y envía `hello`; el receptor responde `ok` o `err` y la conexión queda abierta como canal de control y latido.

| Mensaje | Dirección | Campos | Respuesta |
|---|---|---|---|
| `hello` | móvil→PC | `{"m":"hello","pv":1,"token":"...","code":"1234"?,"name":"<móvil>","model":"<modelo>"}` | `ok` / `err` |
| `ok` | PC→móvil | `{"m":"ok","session_id":u32,"udp_port":26761,"token":"..."?,"mode":"pointer","slot":0,"name":"<PC>"}` | — |
| `err` | PC→móvil | `{"m":"err","code":"bad_token"\|"bad_code"\|"bad_version"\|"busy","msg":"..."}` | cerrar |
| `mode` | ambas | `{"m":"mode","mode":"pointer"\|"dolphin"}` | eco `mode` como confirmación |
| `config` | ambas | `{"m":"config","sensor_hz":u16?,"sens_deg":f32?,...}` solo claves presentes | eco `config` |
| `ping` | ambas | `{"m":"ping","t":u64}` | `{"m":"pong","t":<mismo t>}` |
| `bye` | ambas | `{"m":"bye"}` | cerrar |

Latido: `ping` TCP cada 1 s si no hay tráfico. Sesión muerta a los 5 s sin nada (TCP ni UDP): el receptor libera el slot y descarta el `session_id`.

**Multijugador (desde pv=1, cambio aditivo):** hasta 4 sesiones simultáneas. El receptor asigna a cada `hello` el slot libre más bajo y lo devuelve en `ok.slot` (0 = Jugador 1). Con los 4 ocupados, `err busy`. El mensaje `mode` solo tiene efecto desde el slot 0 (a los demás se les responde con el modo vigente). En modo puntero solo inyecta el slot 0; en modo Dolphin cada sesión alimenta su slot DSU homónimo (0..3), cada uno con su MAC (`"PMP1"+0x00+slot`) y su pulso de recentrado propio.

## 4. Telemetría (UDP, binario)

### 4.1 `INPUT` móvil→receptor — 72 bytes fijos

| off | tam | tipo | campo |
|---|---|---|---|
| 0 | 4 | u32 | magic `0x31504D50` (ASCII "PMP1") |
| 4 | 1 | u8 | tipo = `0x01` |
| 5 | 1 | u8 | flags: bit0 = quaternion válido (el móvil tiene GAME_ROTATION_VECTOR); resto reservado 0 |
| 6 | 2 | u16 | reservado (0) |
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

En modo `dolphin` el receptor NO inyecta nada en el SO: todo el estado va al servidor DSU (mapeo en `protocol/DSU.md`).

### 4.3 `PING`/`PONG` UDP (RTT del hot path, para los HUD)

Mismo socket UDP, **bidireccional**. Layout (20 bytes): bytes 0-7 como INPUT (magic, tipo, flags, reservado), bytes 8-11 `session_id`, bytes 12-19 u64 `t_envio_us` (reloj del emisor del PING). Tipos: `0x02` PING, `0x03` PONG.

Regla: quien recibe un PING responde un PONG con el mismo cuerpo (solo cambia el tipo). Quien recibe un PONG con su `session_id` calcula `RTT = ahora − t_envio_us` con su propio reloj. El móvil pinguea a 1 Hz (HUD del mando); el receptor pinguea a 2 Hz (HUD de la ventana).

## 5. Versionado

`pv` va en `hello` y en el TXT de mDNS. Mismo `pv` = compatible. `pv` distinto → `err bad_version` con `msg` legible ("Actualiza PepoMote en el PC/móvil"). El paquete `INPUT` no cambia dentro de pv=1; cambios de layout = pv=2.

## 6. Vectores dorados (`vectors/`)

Cada `.hex` es un paquete completo en hex ASCII (sin espacios) + un `.json` hermano con los valores decodificados esperados. Tests: Kotlin (`PmpCodecTest`) y Rust (`codec::tests`) parsean el mismo hex y comparan contra el json. Vectores mínimos: `input_neutral`, `input_buttons_all`, `input_motion`, `ping`, `pong`.
