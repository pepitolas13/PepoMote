# SECURITY — modelo de amenaza (v1)

## Qué protege el token

El token de emparejamiento (128 bits aleatorios, en el QR) evita que un dispositivo cualquiera de la LAN se conecte por accidente o por gamberrismo casual. Viaja en claro por TCP/UDP dentro de tu red local.

## Qué NO es

- No hay cifrado del canal: alguien con acceso a tu LAN y un sniffer puede leer la telemetría (movimientos y botones) y, si captura el token, inyectar entrada.
- No hay autenticación criptográfica de dispositivos.

## Supuestos

- Red doméstica de confianza (tu Wi-Fi con WPA2/WPA3). En redes abiertas o compartidas (universidad, oficina, hotel): usa el hotspot del móvil, que crea una red privada directa entre móvil y PC.
- El receptor escucha en todas las interfaces; el firewall de Windows preguntará en el primer arranque — concede solo en "redes privadas".

## Futuro (fuera de v1)

Cifrado del canal (Noise/TLS con pinning del token) si el proyecto crece. PRs bienvenidos.
