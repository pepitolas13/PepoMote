# TROUBLESHOOTING

Se completa con cada hito. Esqueleto:

## El móvil no encuentra el PC

1. Móvil y PC en la misma red (el Wi-Fi de invitados NO vale: aísla clientes).
2. Firewall de Windows: permite PepoMote en redes privadas (pregunta en el primer arranque).
3. Firewall de Linux: muchas distros (CachyOS, Fedora, openSUSE…) traen ufw o
   firewalld activados y descartan TODO lo entrante — el receptor arranca
   perfecto pero ningún paquete del móvil llega. El receptor lo detecta solo:
   en el primer arranque pide tu contraseña en el diálogo del sistema y abre
   el puerto él mismo; si cancelaste, queda el botón «Reparar ahora» en la
   ventana. `packaging/linux/install.sh` hace lo mismo por script. A mano:
   - ufw: `sudo ufw allow 26761/tcp && sudo ufw allow 26761/udp`
   - firewalld: `sudo firewall-cmd --permanent --add-port=26761/tcp --add-port=26761/udp && sudo firewall-cmd --reload`
4. Si el mDNS está roto en tu router, PepoMote prueba solo el broadcast; si tampoco, teclea la IP:puerto que muestra el receptor bajo el QR.
5. Último recurso: hotspot del móvil + PC conectado a él. Funciona siempre.

## Linux: "sin permiso para /dev/uinput"

Pulsa **«Reparar ahora»** en la ventana del receptor (o deja que el diálogo
del primer arranque haga lo suyo): instala la regla udev y da acceso al
instante, sin cerrar sesión. Manualmente: `packaging/linux/install.sh` (regla
`uaccess`: acceso para el usuario de la sesión activa, sin grupos ni root).

## El cursor no va donde apunto / se mueve "acumulando"

Mira en Ajustes de la ventana del PC si "Apuntado absoluto" está desactivado:
en modo relativo el cursor se desplaza con el giro en vez de ir a donde
apuntas (pensado para juegos). Actívalo para el uso normal.

## Varios monitores: a qué pantallas apunta

Por defecto ("Todas las pantallas" en Ajustes), el apuntado absoluto cubre
TODO el escritorio: el cursor llega a cualquiera de tus monitores según hacia
dónde apuntes. Al recentrar, el cursor vuelve al centro del conjunto.

Si juegas en un monitor y quieres apuntado más preciso, en Ajustes →
"Apuntado absoluto en" elige *Solo <ese monitor>*: el rango de giro se dedica
entero a esa pantalla (y el cursor se queda en ella). La app detecta los
monitores sola (se conecten o desconecten en caliente) y usa la disposición
real del escritorio (Wayland xdg-output o X11 xrandr).

Nota: con los monitores en "L" o "T" quedan zonas sin pantalla dentro del
rectángulo que las envuelve; si apuntas a una de esas zonas, el cursor cae en
el borde de la pantalla más próxima (lo coloca el escritorio, no PepoMote).

## En Dolphin el móvil entra como "Jugador 2" y no puedo activar Dolphin

Pasaba al reconectar (Wi-Fi que cae, app al segundo plano) con la sesión
anterior aún "viva" en el PC: el móvil recuperaba plaza como Jugador 2 y el
modo Dolphin solo lo activa el Jugador 1. Ya se arregla solo: al reconectar,
el receptor desaloja la sesión fantasma del mismo móvil y le devuelve su
plaza. Si lo ves en una versión vieja, reinicia el receptor del PC.

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
