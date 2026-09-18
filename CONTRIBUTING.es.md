# Cómo contribuir a PepoMote

[English](CONTRIBUTING.md) · **Español**

Gracias por ayudar a mejorar PepoMote. La idea es sencilla: coger el móvil, conectarlo y disfrutar jugando. Un fallo bien explicado, una prueba en un dispositivo que no tenemos o una guía más clara pueden aportar tanto como el código. No hace falta tener experiencia programando para participar.

**Escribe en español o en inglés, como te resulte más cómodo.** No necesitas traducir tu incidencia ni tu pull request. Te pedimos que respetes nuestro [código de conducta](CODE_OF_CONDUCT.es.md).

## Dónde acudir

| Quieres… | Empieza aquí |
| --- | --- |
| Ayuda para instalar, conectar o configurar un mando | [Guía de ayuda](SUPPORT.es.md), [Discussions](https://github.com/pepitolas13/PepoMote/discussions) o [Discord](https://discord.gg/Vx3MPuMPxb) |
| Avisar de un fallo que se puede reproducir | [Notificar un fallo](https://github.com/pepitolas13/PepoMote/issues/new?template=02-bug-es.yml) |
| Proponer una mejora | [Proponer una función](https://github.com/pepitolas13/PepoMote/issues/new?template=04-feature-es.yml) |
| Mejorar una guía, una traducción o la accesibilidad | [Incidencia de documentación o accesibilidad](https://github.com/pepitolas13/PepoMote/issues/new?template=05-docs-accessibility.yml), o una pull request pequeña directamente |
| Comunicar una vulnerabilidad | [Cómo avisar en privado](docs/SECURITY.es.md) |

Busca primero entre las incidencias y conversaciones existentes. Si alguien ya ha avisado del mismo problema, añade allí los datos de tu dispositivo o los pasos para reproducirlo. Si solo quieres apoyar una idea, basta con una reacción.

## Formas de ayudar

- **Probar dispositivos y juegos reales.** Cuenta qué móvil, receptor, sistemas operativos, versiones de PepoMote, emulador y núcleo has usado. Incluye lo que funciona y lo que falla. Las pruebas simuladas no cubren todas las combinaciones.
- **Mejorar los primeros minutos.** Unas instrucciones más claras para instalar, emparejar o entender los botones pueden ahorrar muchos intentos.
- **Mejorar la accesibilidad y las traducciones.** Explica qué tarea resulta difícil y cómo ayudaría el cambio. En los textos de la interfaz, mantén el español y el inglés al día cuando puedas; si solo hablas uno de los dos, pide ayuda.
- **Resolver un problema concreto.** Mira las incidencias con [good first issue](https://github.com/pepitolas13/PepoMote/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22) o [help wanted](https://github.com/pepitolas13/PepoMote/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22). Si están vacías, cuenta en Discussions en qué te gustaría trabajar.

Antes de dedicar mucho tiempo a una función grande, una dependencia nueva o un cambio de protocolo, abre una incidencia. Así podemos acordar qué problema resolvemos, las plataformas implicadas y el mantenimiento que necesitará. Las correcciones pequeñas y las mejoras de documentación pueden ir directamente a una pull request.

## Cómo se organiza el proyecto

| Carpeta | Contenido |
| --- | --- |
| `desktop/` | Receptor para Windows, Linux y macOS, en Rust |
| `android/` | Mando y servidor Android, en Kotlin |
| `ios/` | Mando para iPhone y iPad, en Swift |
| `mobile-linux/` | Mando para móviles Linux, en Rust |
| `pmp/` | Códec compartido del protocolo, en Rust |
| `protocol/` | Especificación del protocolo, vectores de prueba y distribuciones de mandos |
| `docs/` | Guías, solución de problemas y notas de versiones |
| `packaging/` | Scripts de empaquetado e instalación |

## Preparar y comprobar tu cambio

Haz un fork del repositorio, clona tu copia, crea una rama desde `main` y trabaja en un cambio concreto. Solo necesitas las herramientas del componente que vayas a tocar. Si únicamente cambias documentación, no hace falta compilar la aplicación.

**Receptor y protocolo compartido:** instala Rust estable. Windows también necesita las herramientas de compilación de C++ de MSVC; macOS, las herramientas de línea de comandos de Xcode. Los paquetes de Linux están en el [workflow del receptor](.github/workflows/desktop.yml). Desde la raíz del repositorio:

```sh
cargo build --release --manifest-path desktop/Cargo.toml
cargo test --release --manifest-path desktop/Cargo.toml
cargo test --release --manifest-path pmp/Cargo.toml
```

Para cambios en el receptor o en la integración con emuladores, sigue también la [guía de pruebas de extremo a extremo aisladas](desktop/e2e/README.md). Distingue las comprobaciones con clientes simulados de las realizadas con dispositivos o emuladores reales.

**Android:** utiliza JDK 17 y Android SDK 36 con build tools 36.0.0, como el [workflow de Android](.github/workflows/android.yml). Desde `android/`:

```sh
./gradlew testDebugUnitTest lintRelease assembleDebug
```

En PowerShell de Windows, usa `./gradlew.bat testDebugUnitTest lintRelease assembleDebug`. La compilación de depuración no necesita las claves de firma de las versiones publicadas.

**iOS:** necesitas un Mac con Xcode 16 o posterior y XcodeGen. Desde `ios/`, ejecuta `xcodegen generate`, abre `PepoMote.xcodeproj`, elige el esquema `PepoMote` y un simulador de iPhone o iPad disponible, y ejecuta **Product → Test**. El [workflow de iOS](.github/workflows/ios.yml) incluye el equivalente por línea de comandos. Para cambiar la configuración del proyecto, edita `project.yml`; el proyecto de Xcode se genera a partir de él.

**Móviles Linux:** utiliza Rust estable en Linux y los paquetes indicados en el [workflow de Linux móvil](.github/workflows/mobile-linux.yml). Desde la raíz del repositorio:

```sh
cargo build --release --manifest-path mobile-linux/Cargo.toml
cargo test --release --manifest-path mobile-linux/Cargo.toml
cargo test --release --manifest-path pmp/Cargo.toml
```

Si no tienes una herramienta, un dispositivo o una plataforma, indícalo en la pull request. Cuenta qué has comprobado y qué queda pendiente; una limitación bien explicada ayuda más que un resultado supuesto.

## Enviar una pull request

1. Explica el problema y cómo lo mejora tu cambio. Enlaza la incidencia si existe; no hace falta abrir una para una corrección pequeña.
2. Deja fuera las refactorizaciones ajenas al cambio, los archivos generados y los binarios de las versiones. Sigue el estilo del código que te rodea.
3. Si corriges un fallo, añade o actualiza una prueba de regresión cuando pueda cubrirlo de forma útil. Incluye las comprobaciones manuales pertinentes y sus resultados.
4. Si cambias la interfaz, aporta una captura o una grabación breve cuando ayude, con contenido propio. Revisa español e inglés y los tamaños de móvil y tableta que afecte el cambio.
5. Si cambias el protocolo o la configuración de emuladores, considera todos los mandos y receptores, la compatibilidad con versiones existentes y la copia y restauración de los ajustes del usuario. Actualiza [la especificación](protocol/PROTOCOL.md) y los vectores de prueba cuando corresponda.
6. Abre la pull request contra `main`. Puedes marcarla como borrador si buscas una primera revisión. Indica las plataformas sin probar y el trabajo pendiente.

Durante la revisión puede haber preguntas o una propuesta de empezar con una versión más pequeña. Valoramos la utilidad, la fiabilidad, la accesibilidad y el mantenimiento a largo plazo. Daniel (PepoTech) mantiene el proyecto; las revisiones y la ayuda se atienden según el tiempo disponible, sin un plazo de respuesta garantizado.

## Cuidar los datos y el trabajo de otras personas

Antes de compartir registros o capturas, elimina los QR de emparejamiento, tokens, contraseñas, direcciones privadas y datos que identifiquen a otras personas. Nunca subas claves de firma ni credenciales. Para vulnerabilidades, utiliza la [política de seguridad](docs/SECURITY.es.md).

Aporta solo código y recursos que tengas derecho a compartir. El proyecto utiliza GPL-3.0-or-later para el código, CC-BY-SA 4.0 para sus recursos visuales y sonoros originales y SIL OFL 1.1 para Nunito; consulta la [licencia](LICENSE) y las [reglas de recursos y atribución](docs/LEGAL.md). Conserva los avisos de terceros. No aportes juegos, ROMs, BIOS, recursos propietarios de consolas ni enlaces a descargas no autorizadas.

Puedes utilizar herramientas, incluidos asistentes de IA. La responsabilidad de entender tu aportación, comprobar sus fuentes y probar su funcionamiento sigue siendo tuya. Si una limitación de la herramienta afecta a la revisión, explícala.

Gracias por dedicarle tiempo. Una mejora pequeña puede hacer mucho más agradable la primera partida de alguien con PepoMote.
