# Actualizaciones y novedades

PepoMote consulta automáticamente las versiones publicadas y presenta sus novedades dentro de la app. **Instalar** inicia la descarga y la comprobación; **Más tarde** permite seguir usando la app. Desde Ajustes se puede volver a consultar y abrir el aviso.

La comprobación usa `https://github.com/pepitolas13/PepoMote/releases/latest/download/update.json`. No envía códigos de enlace, dispositivos conectados ni datos de las partidas. GitHub recibe una petición HTTPS normal, incluida la dirección IP, como en cualquier descarga. El aviso automático se puede desactivar en Ajustes. Las descargas grandes solo empiezan al pedir instalar.

## Qué ocurre en cada plataforma

| Plataforma | Instalación |
|---|---|
| Windows | Descarga verificada, cierre de PepoMote, sustitución del ejecutable y reapertura. Conserva una copia para recuperarse si falla el arranque. |
| Linux de escritorio y Linux móvil | Selecciona el ejecutable o AppImage apropiado para la arquitectura y el sistema. Las instalaciones administradas por la distribución se actualizan con su gestor de paquetes. |
| macOS | Actualiza el paquete `.app` completo. La identidad de firma debe mantenerse entre versiones para conservar los permisos del sistema. |
| Android | Comprueba tamaño, SHA-256, nombre del paquete, versión y firma antes de abrir el instalador de Android. El sistema pide confirmar la instalación y, la primera vez, permitir instalar desde PepoMote. |
| iPhone y iPad | Muestra las novedades y permite continuar en SideStore o AltStore. La tienda debe tener configurada la fuente de PepoMote y poder firmar la actualización. Con Sideloadly se usa el ordenador. |

PepoMote no presenta «actualizado» solo porque se haya abierto un instalador o una tienda. El sistema operativo puede pedir permisos o impedir una instalación; ese paso no puede eliminarse desde la app.

## Si una actualización falla

Una descarga incompleta o con huella incorrecta no sustituye la app. Comprueba la conexión y vuelve a intentarlo. Si una carpeta de instalación no permite escribir, utiliza una carpeta de tu usuario o el instalador correspondiente. Los ajustes y dispositivos enlazados se almacenan aparte del ejecutable.

Si GitHub no responde, se reintentará la comprobación. También puedes solicitarla desde Ajustes. Las versiones anteriores sin `update.json` siguen disponibles en [Descargas](https://github.com/pepitolas13/PepoMote/releases); no ofrecen el nuevo instalador automático.

**Primer despliegue:** las apps antiguas solo pueden mostrar el aviso que ya llevan incorporado. Es necesario instalar una vez la versión que incluye este sistema. Las siguientes publicaciones completas podrán usarlo. Implementar el sistema en el código no actualiza por sí solo las instalaciones existentes.

## Preparar las notas de una versión

Junto a `docs/releases/vX.Y.Z.md`, crea `docs/releases/vX.Y.Z.json` con entre una y ocho frases por idioma, de hasta 280 caracteres cada una, sin saltos de línea ni caracteres de control o formato invisibles. Empieza por el cambio que más le importe al usuario. `docs/releases/unreleased.json` contiene el borrador para la próxima versión; revisa sus frases y cópialo con el número definitivo al publicar.

```json
{
  "es": ["Descubre las novedades y actualiza desde PepoMote."],
  "en": ["See what's new and update from PepoMote."]
}
```

La publicación valida las frases, todos los paquetes y sus versiones. Después calcula las huellas y tamaños reales y genera `update.json`. La fuente de SideStore/AltStore incluye las mismas novedades y la huella SHA-256 del IPA, sin texto fijo de una versión anterior. Nunca edites a mano las huellas ni cambies archivos de una versión publicada: publica otra versión.

El formato se define en `packaging/updates/manifest.py`. Los clientes rechazan esquemas desconocidos, versiones inadecuadas, URLs ajenas al repositorio, tamaños excesivos y paquetes incompletos. Las huellas comprueban integridad; la procedencia del manifiesto depende de HTTPS y del repositorio oficial, no de una firma criptográfica independiente.

## Validación antes de distribuir

Ejecuta las pruebas de contrato, las pruebas y compilaciones de cada plataforma y el flujo de actualización en dispositivos reales. Incluye: aceptar y posponer, perder Internet durante la descarga, cancelar, reintentar, rechazar permisos, conservar ajustes, archivo dañado, carpeta sin escritura y fallo de arranque. Un aviso visible y una compilación correcta no bastan para demostrar una instalación completa.

La CI de Windows ejecuta además `desktop/tests/update-handoff.ps1` contra el instalador recién compilado: en una carpeta aislada comprueba el intercambio con la app, la sustitución, el arranque correcto y la recuperación con reapertura de la versión anterior si falla la nueva. Esta prueba no modifica la instalación del usuario.

En Windows no se pueden ejecutar las pruebas de interfaz ni de instalación nativa de iOS y macOS. Los runners correspondientes y los dispositivos reales deben validar esos recorridos antes de publicar. No existe una garantía de cero fallos en cualquier equipo; la descarga verificada y la recuperación reducen el riesgo y permiten volver a intentarlo.
