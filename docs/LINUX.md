# PepoMote en Linux (receptor)

El receptor para Linux es el mismo que el de Windows y macOS: el móvil mueve
el cursor del PC, hace de mando de Wii en Dolphin y de GamePad de Wii U en
Cemu. Funciona en **X11 y Wayland**, en x86_64 con glibc 2.35 o más nueva
(Ubuntu 22.04, Debian 12, Fedora 36, openSUSE Leap 15.6, Arch, CachyOS,
Manjaro y posteriores).

## Instalar

- **AppImage** (`PepoMote-x86_64.AppImage`): dale permiso de ejecución
  (`chmod +x`) y ábrelo. No lleva bibliotecas dentro: usa las del sistema
  (abajo). Sin FUSE: `./PepoMote-x86_64.AppImage --appimage-extract-and-run`.
- **tar.gz** (`PepoMote-linux-x86_64.tar.gz`): descomprime y ejecuta
  `./install.sh`. Instala el binario en `~/.local/bin`, el lanzador del menú,
  la regla udev de uinput y abre el puerto del firewall (pide `sudo`).
  `install.sh PepoMote-x86_64.AppImage` hace lo mismo con el AppImage.

### Bibliotecas que necesita

| Para | Debian / Ubuntu | Fedora | Arch |
|---|---|---|---|
| La ventana (EGL/GL, xkbcommon) | `libegl1 libgl1 libgl1-mesa-dri libxkbcommon0 libxkbcommon-x11-0 libwayland-egl1` | `mesa-libEGL mesa-libGL mesa-dri-drivers libxkbcommon libxkbcommon-x11 libwayland-egl` | `mesa libxkbcommon libxkbcommon-x11 wayland` |
| La campanita de conexión | `libasound2` | `alsa-lib` | `alsa-lib` |

Vienen en cualquier escritorio normal. `PepoMote --diag` dice cuáles faltan
(sección «Bibliotecas») y la versión de glibc.

## Primer arranque

1. **Firewall**: muchas distros traen ufw o firewalld activados y descartan
   todo lo que entra; el receptor arranca perfecto pero el móvil no llega. Lo
   detecta y lo abre él mismo (abajo).
2. **Cursor**: en Sway, Hyprland, niri, river, labwc y demás compositores
   wlroots el cursor es un puntero virtual de Wayland y no hace falta nada.
   En **GNOME, KDE y cualquier sesión X11** va por `/dev/uinput`, que
   necesita una regla udev (una sola vez).
3. **El diálogo de contraseña**: la ventana enseña arriba una tarjeta
   «Configuración del sistema pendiente» con lo que falta y lo que va a
   pedir. A los tres segundos (o al pulsar «Reparar ahora») aparece el
   diálogo del sistema «Se requiere autenticación para ejecutar /bin/sh como
   superusuario»: es PepoMote (pkexec) aplicando dos cosas, cada una por su
   cuenta:
   - la regla udev `/etc/udev/rules.d/99-pepomote.rules` (`uaccess`: acceso
     para el usuario de la sesión, sin grupos), el módulo uinput en
     `/etc/modules-load.d/pepomote.conf` y acceso inmediato con `setfacl`;
   - el puerto 26761 TCP y UDP y mDNS (5353/udp) en ufw o en firewalld.

   Al terminar sale «Listo: …» (o «Aplicado en parte: …» diciendo qué falló).
   «Ahora no» lo deja para el botón; si cancelas el diálogo, no se insiste en
   el siguiente arranque. Si no hubo forma de autenticar, sí se vuelve a
   ofrecer.
4. **Sin diálogo** (sin `pkexec`, o una sesión sin agente de polkit, típico de
   un gestor de ventanas «a pelo»): la ventana enseña los comandos para pegar
   en un terminal, con botón «Copiar comando».

## Firewall

El receptor solo dice «bloqueando» cuando lo ve: en firewalld consulta
`firewall-cmd --query-port`; en ufw lee `/etc/ufw/user.rules` si es legible.
Si no lo es (va 0640 root en casi todas las distros) dice «no puedo leer sus
reglas: si el móvil no conecta, el puerto está cerrado», y en cuanto la
reparación abre el puerto, o un móvil entra, queda apuntado en
`settings.json` (`firewall_opened_port`) y el aviso no vuelve. A mano:

- ufw: `sudo ufw allow 26761/tcp && sudo ufw allow 26761/udp && sudo ufw allow 5353/udp`
- firewalld: `sudo firewall-cmd --permanent --add-port=26761/tcp --add-port=26761/udp --add-service=mdns && sudo firewall-cmd --reload`
- iptables o nftables a pelo: el receptor no los detecta; abre 26761 TCP y UDP.

## La ventana

Al arrancar, el receptor abre su ventana con la GPU (EGL en Wayland; GLX o
EGL en X11). Si no puede (driver sin OpenGL, máquina virtual sin 3D, una
biblioteca que falta, backend Wayland roto con XWayland sano), **se relanza
solo**, hasta dos veces: primero con render por software
(`LIBGL_ALWAYS_SOFTWARE=1`, Mesa llvmpipe) y después, si la sesión es Wayland
con XWayland, por X11. Si nada funciona, avisa con una notificación de
escritorio y deja el motivo en el log:

```
Ventana: intento 1 · backend automático · GL de la GPU falló antes de pintar: …
Ventana: relanzo (intento 2 · backend automático · GL por software)
Ventana: primer fotograma pintado (intento 2 · backend automático · GL por software)
```

Para forzarlo a mano: `LIBGL_ALWAYS_SOFTWARE=1`, `PEPOMOTE_UI_BACKEND=x11` (o
`wayland`), `PEPOMOTE_NO_UI_FALLBACK=1` (sin relanzamientos, para ver el
error tal cual).

- **Cerrar la ventana = salir** (en Linux no hay icono de bandeja). Para
  dejarlo corriendo, minimízala.
- **Instancia única**: si lanzas PepoMote con otro ya abierto, el nuevo le
  pide al primero que se muestre (la barra avisa) y se retira; si el primero
  no contesta, el nuevo arranca igual.
- Un pánico en un hilo secundario (red, pantallas, firewall…) queda en el log
  y en la ventana, pero no la cierra; los vigilantes se relanzan solos.

## Log y diagnóstico

- `~/.config/pepomote/receptor.log` (y `.1`): arranque, sesión (Wayland/X11),
  ventana e intentos, inyección elegida, móviles, firewall, reparación,
  pánicos (hilo, mensaje, archivo:línea) y los errores de eframe/winit.
- `PepoMote --diag` (o `./PepoMote-x86_64.AppImage --diag`): informe completo
  (sistema, sesión, compositor, `/dev/uinput`, audio, firewall, puertos,
  bibliotecas, glibc y las últimas líneas del log) para pegar en un issue. Se
  guarda también como `diagnostico.txt` junto al log. Funciona aunque la
  ventana no pueda abrirse.

## Variables de entorno

| Variable | Qué hace |
|---|---|
| `PEPOMOTE_INJECT=wayland` \| `uinput` | Fuerza el backend del cursor |
| `PEPOMOTE_UI_BACKEND=x11` \| `wayland` | Fuerza el backend de la ventana |
| `LIBGL_ALWAYS_SOFTWARE=1` | Render por software (Mesa) |
| `PEPOMOTE_NO_UI_FALLBACK=1` | Sin relanzamientos automáticos de la ventana |
| `PEPOMOTE_NO_AUTOFIX=1` | Sin oferta automática de reparación (pkexec) |
| `PEPOMOTE_NO_UPDATE_CHECK=1` | Sin consulta diaria de versión nueva |
| `PEPOMOTE_PORT`, `PEPOMOTE_DSU_PORT` | Puertos (26761 y 26760) |
| `PEPOMOTE_SCREEN` | Pantalla de apuntado (nombre de xrandr o xdg-output) |
| `PEPOMOTE_CONFIG_DIR` | Otra carpeta de configuración (pruebas) |
| `PEPOMOTE_DEBUG=1` | Trazas de pantallas y puntero en stderr |
| `PEPOMOTE_SMOKE=<ms>` | Prueba: salir solo pasado ese tiempo desde el primer fotograma |
| `PEPOMOTE_NO_UI=1` | Prueba: red e inyección sin ventana |
