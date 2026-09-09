# RELEASE — cómo publicar una versión

## Una sola vez: preparar GitHub

1. Crear el repo y subir:
   ```
   gh auth login
   gh repo create PepoTech/PepoMote --public --source C:\PepoMote --push
   ```
2. Secrets del repo (Settings → Secrets and variables → Actions):
   - `KEYSTORE_B64`: el keystore en base64 →
     `[Convert]::ToBase64String([IO.File]::ReadAllBytes("C:\PepoMote\android\pepomote.jks")) | Set-Clipboard`
   - `KEYSTORE_PASSWORD`, `KEY_PASSWORD`: la contraseña del keystore
   - `KEY_ALIAS`: `pepomote`

## Cada versión

1. Subir `versionCode`/`versionName` (android/app/build.gradle.kts) y
   `version` (desktop/Cargo.toml, mobile-linux/Cargo.toml, pmp/Cargo.toml,
   y sus Cargo.lock). Commit.
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
   `PepoMote.apk` firmado y el emisor para Linux móvil
   (`pepomote-mobile_<versión>_arm64.deb` para Mobian,
   `PepoMote-Mobile-aarch64.AppImage` para glibc y
   `PepoMote-Mobile-aarch64-musl.tar.gz` para postmarketOS, en runners ARM),
   genera `SHA256SUMS.txt` y publica el Release solo.
4. Escribir las notas del Release en GitHub (qué cambia y qué archivo
   descargar según plataforma): `gh release edit vX.Y.Z --notes-file notas.md`.

## El keystore

`android/pepomote.jks` + `android/keystore.properties` (AMBOS fuera de git).
**Si se pierde el keystore o su contraseña, no se pueden publicar
actualizaciones del APK que instalen encima de la anterior.** Guarda copia
del .jks y la contraseña en un sitio seguro (gestor de contraseñas).

## Checklist legal antes de publicar (docs/LEGAL.md)

- Cero assets/marcas/sonidos de Nintendo; "Wii" solo nominativo.
- Capturas del README: solo UI propia.
