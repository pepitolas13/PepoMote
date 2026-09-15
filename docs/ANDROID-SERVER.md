# Jugar con dos móviles Android

Instala **PepoMote 1.8 en los dos móviles**. Uno ejecuta Dolphin o Eden y recibe los controles; el otro hace de mando. Es la misma APK y puedes elegir su función desde el inicio.

PepoMote requiere Android 8 o posterior. El móvil donde se ejecutan los juegos también debe cumplir los requisitos de Dolphin o Eden; la compatibilidad y el rendimiento del emulador dependen de ese dispositivo.

## Primera conexión

1. Instala y abre una vez Dolphin oficial o Eden oficial en el móvil donde vas a jugar. Termina su bienvenida y cierra cualquier partida abierta.
2. Conecta los móviles a la misma Wi-Fi. También puedes conectar el mando al punto de acceso del móvil servidor.
3. En el móvil de los juegos, abre PepoMote y toca **Servidor**.
4. En el móvil mando, toca **Conectar → Escanear QR**. Si prefieres escribir el código, toca **Conectar con código**, elige el otro Android y escribe los **4 dígitos** que aparecen en su pantalla Servidor.

La dirección local se elige automáticamente. El mando reconoce que está conectado a un Android y ofrece **Dolphin y Eden**. La conexión queda guardada para la próxima vez. Las conexiones que ya tengas con un PC siguen disponibles.

**Desde Android 1.8.1**, el botón **Conectar con código** sirve también para un PC: elígelo y escribe los cuatro dígitos que muestra PepoMote junto a su QR. Puedes tocar directamente cualquier dispositivo cercano para introducir el código. La app también acepta los códigos de seis dígitos de los servidores Android 1.8.0.

## Qué es el enlace y dónde copiarlo

Es una alternativa al QR: contiene los datos que necesita el mando para conectarse al servidor. No es una página web ni un enlace para descargar PepoMote.

En el Android que ejecuta los juegos, abre **PepoMote → Servidor → Copiar enlace**. Envíalo al móvil mando y, en este, abre **Conectar → Introducir enlace**, pégalo y toca **Conectar**. El enlace se copia desde la pantalla Servidor a partir de Android 1.8.1; en 1.8.0 se comparte desde **¿Estáis en redes distintas? → Usar conexión VPN → Compartir enlace del mando**.

En la misma Wi-Fi, el QR y el código bastan. Para conectar desde otra red, prepara la VPN y selecciona **Usar conexión VPN** antes de copiar o compartir el enlace, como se explica más abajo. Copiar un enlace local no lo convierte en una conexión por Internet.

## Configurar los controles

Conecta los mandos antes de configurar. En el panel del servidor, elige el emulador y toca **Configurar Dolphin** o **Configurar Eden**.

1. Toca **Ir al permiso**.
2. En la pantalla de Android, toca **Usar esta carpeta** y después **Permitir**.
3. Espera a que PepoMote asigne los controles y compruebe la configuración. Guarda una copia de los ajustes anteriores.
4. Toca **Ir a la ficha de Dolphin** —o Eden—, pulsa **Forzar detención** y confirma.
5. Vuelve a PepoMote y toca **Ya lo he cerrado: jugar**.

PepoMote abre el acceso del emulador directamente en Android 10 y posteriores. Si Android 8/9 o el selector de tu fabricante muestra otra ubicación, abre su menú lateral y toca **Dolphin** o **Eden**. No tienes que buscar archivos ni entrar en Android/data.

El permiso se recuerda. En las siguientes sesiones basta con conectar el mando y tocar **Jugar**. Si añades jugadores o cambias el Nunchuk, aparece **Actualizar mandos**, que aprovecha el permiso guardado. Si Eden aún no ha creado sus ajustes, la app te lleva a terminar su bienvenida y permite continuar después.

Android exige que confirmes el permiso y la detención del emulador. Esta última es necesaria cuando cambian los controles porque los emuladores conservan sus ajustes en memoria; quitarlos de recientes puede no bastar. PepoMote recuerda el paso pendiente aunque salgas de la pantalla.

## Durante el juego

El servidor sigue activo mientras el emulador está delante. La notificación permite volver a PepoMote o detenerlo. Puedes conectar hasta **cuatro mandos principales**, cada uno con su jugador. El jugador 1 también puede cambiar de consola desde su mando.

En Dolphin puedes activar el **Nunchuk en el mismo móvil**. En Eden se utiliza el **Pro Controller**. Este servidor Android ofrece esos dos emuladores; las funciones de puntero y Wii U siguen disponibles al conectar con el receptor de un PC.

Los móviles sin giroscopio mantienen la conexión, los botones y los sticks. El movimiento que requiere giroscopio necesita ese sensor. En Android se conservan las pulsaciones y liberaciones aunque lleguen juntas, también después de un periodo sin pulsar. Algunos teléfonos retienen las teclas de volumen para detectar combinaciones antes de enviarlas a la app; esa parte depende de Android.

**Movimiento en Eden:** las versiones oficiales contrastadas mezclan el movimiento recibido con los sensores del móvil servidor para el jugador 1. No ofrecen un ajuste independiente que quite esa interferencia. Los botones y sticks no dependen de esos sensores. PepoMote lo avisa en su panel y no cambia los sensores globales del teléfono.

## Jugar desde redes distintas

Necesitas una VPN que permita a los dos móviles comunicarse entre sí. Tener una segunda IP o activar una VPN comercial para navegar no basta. PepoMote detecta la dirección de la VPN; no crea ni administra esa red.

### WireGuard — recomendado

Usa esta opción si tienes una red WireGuard preparada, por ejemplo en un router compatible o un servidor accesible desde Internet.

1. Instala [WireGuard para Android](https://www.wireguard.com/install/) en ambos móviles.
2. Importa en cada móvil **su propia configuración** de esa red, por QR o archivo, y activa su túnel. La red debe permitir que ambos móviles se comuniquen por sus direcciones privadas IPv4.
3. Si todavía no tienes esa red, consulta la [configuración oficial de WireGuard](https://www.wireguard.com/quickstart/) o las instrucciones de tu router. Instalar las dos apps por sí solo no conecta los móviles.

### Tailscale — alternativa

1. Sigue la [instalación oficial de Tailscale en Android](https://tailscale.com/docs/install/android) en los dos móviles.
2. Añádelos a la misma red privada y activa la conexión en ambos. Las reglas de esa red deben permitir la comunicación entre ellos.

### Con la VPN activa

1. En PepoMote Servidor, toca **¿Estáis en redes distintas? → Usar conexión VPN**.
2. Toca **Copiar enlace** o **Compartir enlace del mando** y envíalo al móvil mando.
3. En el móvil mando, abre ese enlace o entra en **Conectar → Introducir enlace** y pégalo.

Si se pierde la VPN, PepoMote deja de mostrar su QR y te pide activarla. No cambia ese enlace por uno de la red local. El descubrimiento automático de dispositivos sigue limitado a la LAN.

La latencia depende de la distancia y de ambas conexiones. En Tailscale también influye si la conexión es directa o pasa por una retransmisión; su [documentación explica la diferencia](https://tailscale.com/docs/reference/connection-types). **Solo se envían los controles: la imagen del juego se ve en el móvil servidor.**

## Recuperar tus mandos anteriores

Abre **Opciones de controles → Restaurar controles anteriores**. PepoMote recupera los ajustes que gestionó y conserva las modificaciones posteriores que pueda identificar. Después te guía para detener el emulador y volver a abrirlo.

La configuración actúa sobre controles y perfiles, sin importar un paquete completo de datos ni modificar juegos, partidas, claves o gráficos. Si una configuración se interrumpe, la app conserva su copia y ofrece recuperación antes de abrir el emulador.

## Qué se ha comprobado

La integración se ha contrastado con Dolphin oficial 2606a y Eden oficial 0.2.1 y sus fuentes disponibles durante el desarrollo. Las pruebas automáticas cubren botones, sticks, movimiento, código y QR, tráfico entre cliente y servidor, cuatro jugadores, pérdida de señal, reconexión, configuración, restauración y la interfaz. El ciclo de vida del servidor se comprueba en Android API 26 y 35.

Dolphin se ha probado además con la versión de desarrollo en móviles reales. Las pruebas de software no garantizan el comportamiento de todos los fabricantes; no se ha medido una partida entre dos redes físicas distintas. Si encuentras un problema, indica los modelos, las versiones de Android y del emulador y si utilizabas Wi-Fi local o VPN.
