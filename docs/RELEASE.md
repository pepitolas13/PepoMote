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
   `version` (desktop/Cargo.toml). Commit.
2. Tag y push:
   ```
   git tag v1.0.0
   git push origin main --tags
   ```
3. GitHub Actions construye `PepoMote.exe`, `PepoMote-x86_64.AppImage` y
   `PepoMote.apk` firmado, genera `SHA256SUMS.txt` y publica el Release solo.

## El keystore

`android/pepomote.jks` + `android/keystore.properties` (AMBOS fuera de git).
**Si se pierde el keystore o su contraseña, no se pueden publicar
actualizaciones del APK que instalen encima de la anterior.** Guarda copia
del .jks y la contraseña en un sitio seguro (gestor de contraseñas).

## Checklist legal antes de publicar (docs/LEGAL.md)

- Cero assets/marcas/sonidos de Nintendo; "Wii" solo nominativo.
- Capturas del README: solo UI propia.
