# RELEASE — cómo publicar una versión

## Una sola vez: preparar GitHub

1. Crear el repo y subir:
   ```
   gh auth login
   gh repo create pepitolas13/PepoMote --public --source C:\PepoMote --push
   ```
2. Secrets del repo (Settings → Secrets and variables → Actions):
   - `KEYSTORE_B64`: el keystore de Android en base64 →
     `[Convert]::ToBase64String([IO.File]::ReadAllBytes("C:\PepoMote\android\pepomote.jks")) | Set-Clipboard`
   - `KEYSTORE_PASSWORD`, `KEY_PASSWORD`: la contraseña del keystore
   - `KEY_ALIAS`: `pepomote`
   - `MACOS_SIGN_P12_B64`, `MACOS_SIGN_P12_PASSWORD`: el certificado con el
     que se firma el receptor de macOS (abajo). Sin ellos la CI firma ad hoc
     y los usuarios tendrían que volver a dar los permisos en cada versión.
   - Opcionales, solo con una cuenta Developer ID de pago:
     `APPLE_NOTARY_KEY_B64` (clave `.p8` de App Store Connect en base64),
     `APPLE_NOTARY_KEY_ID`, `APPLE_NOTARY_ISSUER`. Con ellos la release
     notariza el DMG y macOS lo abre a la primera, sin «Abrir igualmente».

## El certificado de macOS

Un certificado autofirmado de firma de código, válido diez años. Lo que
importa es que sea **siempre el mismo**: macOS ata los permisos de
Accesibilidad y Grabación de pantalla a la identidad de firma, así que
cambiarlo obliga a los usuarios a volver a darlos (avisar en las notas de la
release si pasa). Se genera en Windows con el OpenSSL de Git Bash
(`MSYS2_ARG_CONV_EXCL='*'` evita que MSYS convierta `/CN=` en una ruta):

```bash
export MSYS2_ARG_CONV_EXCL='*'
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -subj "/CN=PepoTech/O=PepoTech" \
  -addext "extendedKeyUsage=codeSigning" -addext "keyUsage=digitalSignature" -addext "basicConstraints=CA:FALSE" \
  -keyout pepomote-sign.key -out pepomote-sign.crt
# -legacy: el `security import` de macOS no traga el PBES2 por defecto de OpenSSL 3
openssl pkcs12 -export -legacy -inkey pepomote-sign.key -in pepomote-sign.crt -name PepoTech \
  -out pepomote-sign.p12 -passout "pass:LA_CONTRASEÑA"
base64 -w0 pepomote-sign.p12 | gh secret set MACOS_SIGN_P12_B64
gh secret set MACOS_SIGN_P12_PASSWORD --body "LA_CONTRASEÑA"
```

Los archivos viven **fuera del repo** (`C:\Users\DAN\PepoMote-secrets\`,
con `pepomote-sign.password.txt`) y `.gitignore` excluye `*.p12`, `*.key`,
`*.crt`, `*.pem`. **Guarda el .p12 y la contraseña en el gestor de
contraseñas**: si se pierden, el siguiente certificado hará que todos los
usuarios de Mac tengan que volver a dar los permisos.

## Cada versión

1. Subir `versionCode`/`versionName` (android/app/build.gradle.kts),
   `MARKETING_VERSION`/`CURRENT_PROJECT_VERSION` (ios/project.yml) y
   `version` (desktop/Cargo.toml, mobile-linux/Cargo.toml, pmp/Cargo.toml,
   y sus Cargo.lock). Commit. El `Info.plist` de macOS toma la versión de
   desktop/Cargo.toml al empaquetar.
   El emisor de Linux móvil puede ir por delante del tag: la versión del
   `.deb` (`pepomote-mobile_<versión>_arm64.deb`) tiene que ser
   **estrictamente mayor** que la del último publicado, o apt y «Software»
   no ofrecen la actualización (v1.1.1 publicó el 1.1.2; v1.1.2, el 1.1.3).
2. Tag y push:
   ```
   git tag v1.0.0
   git push origin main --tags
   ```
3. GitHub Actions construye `PepoMote.exe`, `PepoMote-x86_64.AppImage`,
   `PepoMote-linux-x86_64.tar.gz`, `PepoMote-macOS.dmg` (firmado con el
   certificado del repo), `PepoMote.apk` firmado, `PepoMote.ipa` con su
   `altstore.json` y el emisor para Linux móvil
   (`pepomote-mobile_<versión>_arm64.deb` para Mobian,
   `PepoMote-Mobile-aarch64.AppImage` para glibc y
   `PepoMote-Mobile-aarch64-musl.tar.gz` para postmarketOS, en runners ARM),
   genera `SHA256SUMS.txt` y publica el Release solo. Los jobs de macOS, iOS
   y musl no bloquean la release si fallan.
4. Escribir las notas del Release en GitHub (qué cambia y qué archivo
   descargar según plataforma): `gh release edit vX.Y.Z --notes-file notas.md`.
   El receptor de macOS va como **beta** mientras no se pruebe en un Mac
   real: decirlo en las notas y en la tabla de descargas.
5. El aviso de versión nueva de todas las apps se basa en la redirección de
   `releases/latest`: una release marcada como pre-release o borrador no se
   anuncia; la release definitiva, sí, en cuanto se publica.

## El keystore

`android/pepomote.jks` + `android/keystore.properties` (AMBOS fuera de git).
**Si se pierde el keystore o su contraseña, no se pueden publicar
actualizaciones del APK que instalen encima de la anterior.** Guarda copia
del .jks y la contraseña en un sitio seguro (gestor de contraseñas).

## Checklist legal antes de publicar (docs/LEGAL.md)

- Cero assets/marcas/sonidos de Nintendo; "Wii" solo nominativo.
- Capturas del README: solo UI propia.
