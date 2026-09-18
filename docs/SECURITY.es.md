# Política de seguridad

[English](SECURITY.md) · **Español**

PepoMote puede enviar entradas a otro dispositivo y, en algunos modos, recibir contenido de su pantalla. Trata los datos de emparejamiento como secretos y utiliza la aplicación en redes de confianza. Aquí explicamos cómo comunicar una vulnerabilidad y cuáles son los límites del modelo de seguridad actual.

## Comunicar una vulnerabilidad en privado

Utiliza **[Report a vulnerability en GitHub](https://github.com/pepitolas13/PepoMote/security/advisories/new)**. Se abrirá un aviso privado dirigido al mantenedor del repositorio.

También puedes contactar con **Daniel (PepoTech)** en **[pepo@pepotech.es](mailto:pepo@pepotech.es)**, preferiblemente con el asunto `PepoMote security`, o por **mensaje privado a PepoTech en [Discord](https://discord.gg/Vx3MPuMPxb)**. Si el formulario de GitHub no está disponible, utiliza el correo.

Evita publicar vulnerabilidades sin corregir en incidencias, pull requests o canales públicos de Discord. Los cierres inesperados de la aplicación, las dudas de configuración y las propuestas se atienden en los [canales de ayuda](../SUPPORT.es.md), salvo que tengan un impacto de seguridad.

Cuenta lo que sepas; no necesitas tener un exploit completo para avisar:

- Versión o commit de PepoMote, dispositivo y sistema operativo afectados, tanto del mando como del receptor cuando corresponda.
- Descripción del problema, su posible impacto y el acceso que necesitaría un atacante.
- Pasos para reproducirlo de forma segura, con tus dispositivos y datos de prueba.
- Un ejemplo mínimo, el fragmento relevante de un registro o una posible solución, si los tienes.

No envíes tokens reales de emparejamiento, contraseñas, claves privadas ni datos de otras personas. Usa valores identificados como datos de prueba. Realiza pruebas solo en sistemas propios o para los que tengas permiso.

## Versiones y seguimiento

Las correcciones de seguridad se centran en la **última versión publicada**. No se garantiza trasladarlas a versiones anteriores; si puedes hacerlo con seguridad, comprueba si el problema también afecta a la versión actual. También puedes avisar de problemas en código en desarrollo indicando el commit. La rama `main` puede contener cambios aún no publicados.

El mantenedor revisará el aviso, pedirá los datos que falten y coordinará la corrección y su publicación cuando corresponda. Es un proyecto independiente, **sin un plazo de respuesta o resolución garantizado**. Si no recibes respuesta, vuelve a escribir en el mismo aviso privado o por correo. Podemos acordar contigo cómo reconocer tu aportación; tus datos personales no se publicarán sin tu permiso. No existe un programa de recompensas económicas por vulnerabilidades.

## Modelo de seguridad actual

### Qué protege el emparejamiento

El receptor utiliza un token aleatorio de emparejamiento, compartido mediante el QR y guardado por los dispositivos emparejados. Ayuda a evitar conexiones accidentales o intentos casuales no autorizados desde otros dispositivos de la red local. El receptor de escritorio también admite un código temporal de un solo uso; consulta la [especificación del protocolo](../protocol/PROTOCOL.md).

### Qué no protege

- **PepoMote no cifra el tráfico de la aplicación.** El emparejamiento y el control viajan en claro por TCP/UDP entre los dispositivos.
- Quien pueda inspeccionar ese tráfico puede leerlo y capturar un token. Con ese token y acceso a la red, podría hacerse pasar por un mando e inyectar entradas.
- El emparejamiento no autentica criptográficamente al otro dispositivo. No debes considerar confidenciales los datos de control ni el contenido de pantalla transmitido en una red no fiable.

Una VPN configurada puede proteger el tráfico dentro de su túnel, pero no añade cifrado ni autenticación de dispositivos al propio PepoMote. La protección depende de la VPN y de la configuración de red.

### Precauciones prácticas

- Usa una red Wi-Fi doméstica de confianza con WPA2/WPA3 o tu propio punto de acceso privado. Evita redes abiertas o compartidas, como hoteles, universidades y oficinas, salvo que dispongas de una configuración protegida adecuada.
- El receptor puede escuchar en todas las interfaces de red. Permite únicamente el acceso que necesite tu configuración; en Windows, concede acceso del firewall para **redes privadas**. No expongas directamente a internet los puertos de PepoMote ni los de entrada de los emuladores.
- Mantén privados los QR, tokens y códigos temporales de emparejamiento. Ocúltalos, junto con los datos personales, antes de compartir capturas, archivos de configuración o diagnósticos.
- Mantén actualizados los dos dispositivos y descarga PepoMote desde las [versiones oficiales](https://github.com/pepitolas13/PepoMote/releases).

El cifrado de la aplicación y una autenticación más sólida son áreas de mejora, **no funciones del protocolo actual**. Consulta [cómo contribuir](../CONTRIBUTING.es.md) si quieres ayudar en ese trabajo.
