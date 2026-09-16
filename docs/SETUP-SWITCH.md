# Jugar a la Switch con PepoMote

El modo **Switch** convierte el móvil en un **Pro Controller** para
**Eden**, con botones, sticks y movimiento cuando el móvil tiene giroscopio. Usa el servidor DSU del receptor;
no necesita un driver de mando virtual. Ryujinx requiere otra integración y
no está incluido en este modo todavía.

## Primera partida

1. Abre el receptor de PepoMote en el PC y conecta el móvil al mismo PC.
2. **Cierra Eden**. En el móvil toca **Switch**: se abre directamente el Pro Controller.
3. Espera el aviso de que Switch está listo. Abre Eden y juega.

PepoMote escribe los mandos en `qt-config.ini`, con una copia de seguridad en
`qt-config.ini.pepomote.bak`. Si Eden está abierto, espera a que lo cierres:
Eden guarda su propia configuración al salir y no puede recibir estos cambios
en caliente. Cambia el número de mandos con Eden cerrado.

La configuración permanece entre sesiones. Después de configurarlo una vez,
puedes conectar el móvil antes o después de abrir Eden. Si cambias el reparto,
cierra y vuelve a abrir Eden.

## Pro Controller y multijugador

Cada móvil es un Pro Controller completo e independiente: dos sticks con sus
pulsaciones, cruceta, A/B/X/Y, L/R/ZL/ZR, −/+, Home y Capturar. Puedes conectar
hasta cuatro móviles, uno por jugador. Conecta los mandos con Eden cerrado
para que se configure el reparto antes de abrir el juego.

Las preferencias de Joy-Con de las primeras pruebas se convierten a Pro
Controller automáticamente. Ya no hay selector de tipos de mando en Switch.

**Capturar** y **Home** corresponden a los botones de Switch. El modo no usa
pantalla táctil, transmisión de vídeo ni micrófono de Wii U. El rol Nunchuk no
forma parte de los mandos de Switch.

## La cabecera del mando

En **Android** y en **iPhone/iPad**, arriba, en el centro, hay una **pastilla
con el modo** («Switch») y, pegado a ella, el botón **Teclado**, que se queda
siempre a la vista para tenerlo a un toque mientras juegas. Toca la pastilla y
baja una tarjeta con el nombre del PC, el jugador y el pad, los chips de modo
y **Salir**; se pliega sola a los 4 segundos, y mientras la estés usando no se
pliega. Al entrar sale desplegada, y también si se cae la conexión.

En **Linux móvil** la cabecera sigue como estaba: los chips **Salir**, el
giro, **Teclado** y el modo, arriba a la derecha y siempre a la vista.

## Teclado

Cuando el juego abra su teclado, toca **Teclado**: en Android y en iPhone/iPad
está al lado de la pastilla de la cabecera; en Linux móvil, entre los chips de
arriba a la derecha.
En Windows y macOS, PepoMote activa la ventana de Eden antes de enviar el texto.
En Linux, deja el diálogo de Eden en primer plano. Si Eden está cerrado, el
receptor no envía texto de Switch a otra aplicación.

## Recuperar tus mandos anteriores

En el receptor, **Restaurar mis mandos de Eden** devuelve únicamente las claves
de los jugadores configurados por PepoMote. Conserva las demás preferencias y
los cambios ajenos al mando, como sonido y gráficos. Los mandos que PepoMote
no haya sustituido no se restauran desde una copia antigua.

La restauración desactiva **Configurar Eden automáticamente** para que una
reconexión no vuelva a sustituir tus mandos. Puedes volver a activarlo en Ajustes,
o usar **Configurar Eden** manualmente. Si Eden está abierto, la restauración
queda pendiente hasta que se cierre, también si reinicias el receptor.

Si has añadido servidores DSU después de configurar PepoMote, se conserva la
lista actual para no renumerar sus mandos. Puede quedar una entrada local sin
mandos asociados; eso no impide usar los demás.

## Localización de Eden

| Instalación | Archivo |
|---|---|
| Windows normal | `%APPDATA%\eden\config\qt-config.ini` |
| Windows portable | `user\config\qt-config.ini` junto a `eden.exe` |
| Linux/macOS normal | `$XDG_CONFIG_HOME/eden/qt-config.ini` o `~/.config/eden/qt-config.ini` |
| Linux/macOS portable | `user/config/qt-config.ini` bajo el directorio desde el que se inició Eden |

En **Ajustes → Carpeta de Eden** puedes indicar la carpeta del ejecutable,
la carpeta que contiene `qt-config.ini` o el propio archivo. **Detectar** busca
la instalación y el receptor aprende su ubicación cuando la ve abierta.
En Linux se comprueba también el directorio de trabajo del proceso. Si una
instalación portable de macOS arranca desde otra carpeta, selecciona directamente
su carpeta de configuración.

PepoMote mantiene el orden de los servidores DSU ya configurados y usa el índice
de mando que corresponde a su servidor. Por eso el mando puede aparecer en Eden
como **UDP Controller 4** u otro número, en lugar de **UDP Controller 0**.
Si ya hay ocho servidores, PepoMote pide liberar uno antes de configurar nada.

## Comprobar esta compilación

Prueba primero Pro Controller en los ajustes de mandos de Eden: A/B/X/Y,
cruceta, ambos sticks y sus pulsaciones, L/R/ZL/ZR, −/+, Home y Capturar.
Comprueba que arriba es arriba y que el giroscopio acompaña al móvil.
Si tienes otro móvil, comprueba que cada uno controla un jugador independiente.

En un móvil **sin giroscopio**, prueba botones, pulsaciones de ambos sticks y
gatillos en Switch, Wii U y Dolphin. La conexión y los controles táctiles
funcionan sin el sensor; el movimiento por giroscopio necesita ese hardware.
Si hay acelerómetro se conserva su lectura disponible.

En **Ajustes del receptor → Volver al puntero al cerrar un emulador**, la casilla
está desactivada por defecto. Cerrar Dolphin, Cemu o Eden mantiene el modo actual.
Activarla recupera el retorno automático; si queda otro emulador abierto,
se utiliza su modo. El ajuste general de cambio automático debe estar activo.

Las pruebas automáticas comprueban el protocolo, las asignaciones, la escala del
giroscopio y la protección de los archivos. La sensación de movimiento, la
orientación física y la compatibilidad de cada juego requieren teléfonos reales.
