#!/usr/bin/env bash
# PepoMote para Linux móvil — instalación limpia, sin FUSE ni root obligatorio.
#   ./install.sh PepoMote-Mobile-aarch64.AppImage       (Mobian, Debian, Fedora, Arch… — glibc)
#   ./install.sh PepoMote-Mobile-aarch64-musl.tar.gz    (postmarketOS / Alpine — musl)
# Deja la app en ~/.local/opt/PepoMote-Mobile con su lanzador e icono; la
# regla udev (opcional, pide sudo) permite subir la frecuencia del IMU.
set -euo pipefail

PKG="${1:-}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEST="$HOME/.local/opt/PepoMote-Mobile"
APPS="$HOME/.local/share/applications"
ICONS="$HOME/.local/share/icons/hicolor/256x256/apps"

if [[ -z "$PKG" || ! -f "$PKG" ]]; then
    echo "Uso: $0 PepoMote-Mobile-aarch64.AppImage | PepoMote-Mobile-aarch64-musl.tar.gz"
    exit 1
fi

rm -rf "$DEST"
mkdir -p "$DEST" "$APPS" "$ICONS" "$HOME/.local/bin"

case "$PKG" in
    *.AppImage)
        echo "==> Extrayendo el AppImage (así no hace falta FUSE)"
        chmod +x "$PKG"
        tmp="$(mktemp -d)"
        if (cd "$tmp" && "$OLDPWD/$PKG" --appimage-extract >/dev/null 2>&1) && [[ -x "$tmp/squashfs-root/usr/bin/PepoMote-Mobile" ]]; then
            cp -a "$tmp/squashfs-root/." "$DEST/"
            BIN="$DEST/usr/bin/PepoMote-Mobile"
        else
            echo "   (no se pudo extraer: se copia el AppImage tal cual, necesita libfuse2)"
            install -m 0755 "$PKG" "$DEST/PepoMote-Mobile.AppImage"
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

echo "==> Lanzador e icono"
ln -sf "$BIN" "$HOME/.local/bin/PepoMote-Mobile"
ICON_SRC="$SCRIPT_DIR/../linux/pepomote.png"
[[ -f "$DEST/dev.pepotech.PepoMote.png" ]] && ICON_SRC="$DEST/dev.pepotech.PepoMote.png"
[[ -f "$ICON_SRC" ]] && install -m 0644 "$ICON_SRC" "$ICONS/dev.pepotech.PepoMote.png"
DESKTOP_SRC="$SCRIPT_DIR/dev.pepotech.PepoMote.desktop"
[[ -f "$DEST/dev.pepotech.PepoMote.desktop" ]] && DESKTOP_SRC="$DEST/dev.pepotech.PepoMote.desktop"
sed "s|^Exec=.*|Exec=$BIN|" "$DESKTOP_SRC" > "$APPS/dev.pepotech.PepoMote.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$APPS" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

if [[ -f "$SCRIPT_DIR/90-pepomote-iio.rules" ]] && command -v sudo >/dev/null; then
    echo "==> Regla udev para subir la frecuencia del IMU (opcional; se pide sudo una vez, Ctrl+C para saltarla)"
    if sudo install -m 0644 "$SCRIPT_DIR/90-pepomote-iio.rules" /etc/udev/rules.d/90-pepomote-iio.rules; then
        sudo udevadm control --reload-rules && sudo udevadm trigger --subsystem-match=iio || true
    fi
fi

echo
echo "Listo: abre 'PepoMote' desde el lanzador de aplicaciones."
echo "Primera vez: Conectar → elige tu PC → teclea el código de 4 dígitos que hay bajo el QR del receptor."
