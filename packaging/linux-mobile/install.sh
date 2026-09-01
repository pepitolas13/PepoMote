#!/bin/sh
# PepoMote para Linux móvil — instalación limpia, sin FUSE ni root obligatorio.
#   sh install.sh PepoMote-Mobile-aarch64.AppImage       (Mobian, Debian, Fedora, Arch… — glibc)
#   sh install.sh PepoMote-Mobile-aarch64-musl.tar.gz    (postmarketOS / Alpine — musl)
# POSIX sh a propósito: postmarketOS no trae bash. Deja la app en
# ~/.local/opt/PepoMote-Mobile con lanzador e icono (todo viene dentro del
# paquete: no hace falta el repo); la regla udev (opcional, pide sudo/doas)
# permite subir la frecuencia del IMU.
set -eu

PKG="${1:-}"
if [ -z "$PKG" ] || [ ! -f "$PKG" ]; then
    echo "Uso: $0 PepoMote-Mobile-aarch64.AppImage | PepoMote-Mobile-aarch64-musl.tar.gz"
    exit 1
fi
PKG="$(cd "$(dirname "$PKG")" && pwd)/$(basename "$PKG")"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DEST="$HOME/.local/opt/PepoMote-Mobile"
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor/256x256/apps"

rm -rf "$DEST"
mkdir -p "$DEST" "$APPS" "$ICONS" "$HOME/.local/bin"

case "$PKG" in
    *.AppImage)
        echo "==> Extrayendo el AppImage (así no hace falta FUSE)"
        chmod +x "$PKG"
        tmp="$(mktemp -d)"
        if (cd "$tmp" && "$PKG" --appimage-extract >/dev/null 2>&1) && [ -x "$tmp/squashfs-root/usr/bin/PepoMote-Mobile" ]; then
            cp -R "$tmp/squashfs-root/." "$DEST/"
            BIN="$DEST/usr/bin/PepoMote-Mobile"
        else
            echo "   (no se pudo extraer: se copia el AppImage tal cual; necesita libfuse2)"
            cp "$PKG" "$DEST/PepoMote-Mobile.AppImage"
            BIN="$DEST/PepoMote-Mobile.AppImage"
        fi
        rm -rf "$tmp"
        ;;
    *.tar.gz|*.tgz)
        echo "==> Desempaquetando"
        tar -xzf "$PKG" -C "$DEST" --strip-components=1
        BIN="$DEST/PepoMote-Mobile"
        ;;
    *)
        echo "No sé instalar '$PKG'"; exit 1 ;;
esac
chmod +x "$BIN"

# Recursos: primero los que vienen dentro del paquete; si no, los del repo
res() {
    if [ -f "$DEST/$1" ]; then echo "$DEST/$1"
    elif [ -f "$SCRIPT_DIR/$1" ]; then echo "$SCRIPT_DIR/$1"
    elif [ -f "$SCRIPT_DIR/../linux/$2" ]; then echo "$SCRIPT_DIR/../linux/$2"
    fi
}

echo "==> Lanzador e icono"
ln -sf "$BIN" "$HOME/.local/bin/PepoMote-Mobile"
ICON="$(res dev.pepotech.PepoMote.png pepomote.png)"
[ -n "$ICON" ] && cp "$ICON" "$ICONS/dev.pepotech.PepoMote.png"
DESKTOP="$(res dev.pepotech.PepoMote.desktop -)"
if [ -n "$DESKTOP" ]; then
    sed "s|^Exec=.*|Exec=$BIN|" "$DESKTOP" > "$APPS/dev.pepotech.PepoMote.desktop"
else
    printf '[Desktop Entry]\nName=PepoMote\nExec=%s\nIcon=dev.pepotech.PepoMote\nTerminal=false\nType=Application\nCategories=Game;Utility;\nX-Purism-FormFactor=Workstation;Mobile;\n' "$BIN" > "$APPS/dev.pepotech.PepoMote.desktop"
fi
command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$APPS" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

RULES="$(res 90-pepomote-iio.rules -)"
SUDO=""
command -v sudo >/dev/null 2>&1 && SUDO=sudo
[ -z "$SUDO" ] && command -v doas >/dev/null 2>&1 && SUDO=doas
if [ -n "$RULES" ] && [ -n "$SUDO" ]; then
    echo "==> Regla udev para subir la frecuencia del IMU (opcional; pide contraseña una vez, Ctrl+C para saltarla)"
    if $SUDO install -m 0644 "$RULES" /etc/udev/rules.d/90-pepomote-iio.rules 2>/dev/null; then
        $SUDO udevadm control --reload-rules 2>/dev/null && $SUDO udevadm trigger --subsystem-match=iio 2>/dev/null || true
    fi
fi

echo
echo "Listo: abre 'PepoMote' desde el lanzador de aplicaciones."
echo "Primera vez: Conectar → elige tu PC → teclea el código de 4 dígitos que hay bajo el QR del receptor."
