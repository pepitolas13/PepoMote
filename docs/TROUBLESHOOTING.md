# TROUBLESHOOTING

Se completa con cada hito. Esqueleto:

## El móvil no encuentra el PC

1. Móvil y PC en la misma red (el Wi-Fi de invitados NO vale: aísla clientes).
2. Firewall de Windows: permite PepoMote en redes privadas (pregunta en el primer arranque).
3. Si el mDNS está roto en tu router, PepoMote prueba solo el broadcast; si tampoco, teclea la IP:puerto que muestra el receptor bajo el QR.
4. Último recurso: hotspot del móvil + PC conectado a él. Funciona siempre.

## Linux: "sin permiso para /dev/uinput"

Ejecuta `packaging/linux/install.sh` (instala la regla udev) y cierra sesión y vuelve a entrar. La regla `uaccess` da acceso al usuario de la sesión activa, sin grupos ni root.

## El cursor no va donde apunto / se mueve "acumulando"

Mira en Ajustes de la ventana del PC si "Apuntado absoluto" está desactivado:
en modo relativo el cursor se desplaza con el giro en vez de ir a donde
apuntas (pensado para juegos). Actívalo para el uso normal.

## El cursor va a tirones

- HUD del receptor: si el RTT sube de ~15 ms, es la red — pásate a 5 GHz o al hotspot del móvil.
- Frecuencia real del sensor en el HUD: algunos móviles capan a 50-100 Hz; funciona igual, con algo menos de finura.

## Latencia jugando en Dolphin

PepoMote añade lo mismo que en modo puntero (~10-20 ms en LAN 5 GHz). Si notas
retardo, casi siempre viene de la cadena de vídeo, no del mando:

1. **La TV**: activa el "Modo juego" de la tele. Una TV en modo normal mete
   50-100 ms de procesado — es la causa nº 1.
2. **Dolphin**: Gráficos → Avanzado → activa "Present XFB Immediately"
   (Immediately Present XFB) y desactiva V-Sync. Pantalla completa.
3. **Wi-Fi**: HUD del receptor con RTT alto → 5 GHz o hotspot del móvil.
4. Comprueba que el HUD marca ~200-250 paquetes/s durante el juego.

## Linux móvil: "No encuentro giroscopio"

`ls /sys/bus/iio/devices/*/in_anglvel_x_raw` debe listar un archivo. Si no,
el kernel no expone el gyro (falta el driver o el móvil no lo tiene). Si la
cabecera del mando marca 50 Hz, instala la regla udev con
`packaging/linux-mobile/install.sh` para que la app pueda subir la frecuencia.
Más en [MOBILE-LINUX.md](MOBILE-LINUX.md).

## Android mata la conexión al apagar la pantalla

PepoMote usa un servicio en primer plano con wakelock; concédele la exención de optimización de batería cuando la pida. En OEMs agresivos (Xiaomi, Huawei…): dontkillmyapp.com/<tu-marca>.
