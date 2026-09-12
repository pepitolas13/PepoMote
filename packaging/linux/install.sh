#!/usr/bin/env bash
# PepoMote — instalación limpia en Linux.
# Hace dos cosas: instala la regla udev de uinput (necesita sudo una vez)
# y deja el AppImage en ~/.local/bin con su lanzador de escritorio.
set -euo pipefail

APPIMAGE="${1:-PepoMote-x86_64.AppImage}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ ! -f "$APPIMAGE" ]]; then
    echo "Uso: $0 [ruta a PepoMote-x86_64.AppImage]"
    echo "No encuentro '$APPIMAGE'."
    exit 1
fi

echo "==> Regla udev para /dev/uinput (se pide sudo una sola vez)"
sudo install -m 0644 "$SCRIPT_DIR/99-pepomote.rules" /etc/udev/rules.d/99-pepomote.rules
sudo udevadm control --reload-rules
sudo udevadm trigger /dev/uinput 2>/dev/null || true
sudo modprobe uinput 2>/dev/null || true
echo 'uinput' | sudo tee /etc/modules-load.d/pepomote.conf >/dev/null
# ACL directa para no tener que cerrar sesión (la regla uaccess cubre las siguientes)
sudo setfacl -m "u:${SUDO_UID:-$(id -u)}:rw" /dev/uinput 2>/dev/null || true

PORT="${PEPOMOTE_PORT:-26761}"
echo "==> Firewall: abriendo el puerto $PORT (TCP+UDP) si hay uno activo"
if command -v ufw >/dev/null 2>&1 && sudo ufw status 2>/dev/null | grep -q "^Status: active\|^Estado: activo"; then
    sudo ufw allow "$PORT/tcp" comment 'PepoMote' >/dev/null
    sudo ufw allow "$PORT/udp" comment 'PepoMote' >/dev/null
    # mDNS: solo hace falta para el autodescubrimiento; el QR funciona sin él
    sudo ufw allow 5353/udp comment 'mDNS (PepoMote)' >/dev/null || true
    echo "    ufw: puertos abiertos"
elif command -v firewall-cmd >/dev/null 2>&1 && sudo firewall-cmd --state >/dev/null 2>&1; then
    sudo firewall-cmd --permanent --add-port="$PORT/tcp" --add-port="$PORT/udp" >/dev/null
    sudo firewall-cmd --permanent --add-service=mdns >/dev/null 2>&1 || true
    sudo firewall-cmd --reload >/dev/null
    echo "    firewalld: puertos abiertos"
else
    echo "    sin ufw/firewalld activos — nada que abrir"
fi

echo "==> Instalando el AppImage en ~/.local/bin"
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/256x256/apps"
install -m 0755 "$APPIMAGE" "$HOME/.local/bin/PepoMote"
if [[ -f "$SCRIPT_DIR/pepomote.png" ]]; then
    install -m 0644 "$SCRIPT_DIR/pepomote.png" "$HOME/.local/share/icons/hicolor/256x256/apps/pepomote.png"
fi
sed "s|^Exec=.*|Exec=$HOME/.local/bin/PepoMote|" "$SCRIPT_DIR/PepoMote.desktop" \
    > "$HOME/.local/share/applications/PepoMote.desktop"

echo
echo "Listo. Abre 'PepoMote' desde tu lanzador de aplicaciones."
echo "En Sway, Hyprland, MangoWC y demás compositores wlroots el cursor va por Wayland: no necesita nada de uinput."
echo "Solo si /dev/uinput siguiera sin permiso (GNOME, KDE, X11): cierra sesión y vuelve a entrar (la regla uaccess se aplica al iniciar sesión)."
