#!/usr/bin/env bash
# Empaqueta desktop/target/release/PepoMote (Apple Silicon) como
# dist/PepoMote.app y dist/PepoMote-macOS.dmg (arrastrar a Aplicaciones).
# Pensado para la CI (macos-15). Requiere las herramientas de macOS: sips,
# iconutil, codesign, hdiutil.
#
# Firma: IDENTITY (la importa packaging/macos/import-identity.sh desde el
# secret del repo; '-' = ad hoc). Con una identidad ESTABLE los permisos de
# Accesibilidad y Grabación de pantalla se conservan al actualizar; con ad
# hoc cada build es «otra app» para macOS y hay que volver a darlos.
# NOTARIZE=1 (solo con Developer ID de pago): hardened runtime + timestamp.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$ROOT/desktop/target/release/PepoMote"
OUT="$ROOT/dist"
APP="$OUT/PepoMote.app"
PKG="$ROOT/packaging/macos"
IDENTITY="${IDENTITY:--}"

[[ -f "$BIN" ]] || { echo "Compila antes: cd desktop && cargo build --release"; exit 1; }
VERSION="$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/desktop/Cargo.toml" | head -1)"
[[ -n "$VERSION" ]] || { echo "No encuentro la versión en desktop/Cargo.toml"; exit 1; }

rm -rf "$APP" "$OUT/dmg-root" "$OUT/PepoMote.iconset"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources" "$OUT/dmg-root"

install -m 0755 "$BIN" "$APP/Contents/MacOS/PepoMote"
sed "s/@VERSION@/$VERSION/g" "$PKG/Info.plist" > "$APP/Contents/Info.plist"
printf 'APPL????' > "$APP/Contents/PkgInfo"

# Icono: iconset con sips + iconutil a partir del PNG de 1024 (generado con
# packaging/macos/make_icon.py: la misma geometría que desktop/src/icon.rs)
ICONSET="$OUT/PepoMote.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
    sips -z "$s" "$s" "$PKG/icon-1024.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    sips -z "$((s * 2))" "$((s * 2))" "$PKG/icon-1024.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/PepoMote.icns"

# Firma
if [[ "${NOTARIZE:-0}" == 1 ]]; then
    codesign --force --deep --options runtime --timestamp --sign "$IDENTITY" "$APP"
else
    codesign --force --deep --timestamp=none --sign "$IDENTITY" "$APP"
fi
codesign --verify --deep --strict --verbose=2 "$APP"

# Updater replaces the complete signed bundle, keeping its permissions and
# resources. On notarized releases the workflow recreates this ZIP after staple.
ditto -c -k --keepParent --norsrc "$APP" "$OUT/PepoMote-macOS.zip"

# DMG: la app y un enlace a Aplicaciones para arrastrar
cp -R "$APP" "$OUT/dmg-root/"
ln -s /Applications "$OUT/dmg-root/Applications"
rm -f "$OUT/PepoMote-macOS.dmg"
hdiutil create -volname "PepoMote" -srcfolder "$OUT/dmg-root" -ov -format UDZO "$OUT/PepoMote-macOS.dmg" >/dev/null
if [[ "$IDENTITY" != "-" ]]; then
    codesign --timestamp=none --sign "$IDENTITY" "$OUT/PepoMote-macOS.dmg"
fi
rm -rf "$OUT/dmg-root" "$ICONSET"
echo "OK: $OUT/PepoMote-macOS.dmg (v$VERSION, firma: $IDENTITY)"
