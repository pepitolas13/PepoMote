# Driver del mando virtual (Windows)

`ViGEmBus_1.22.0_x64_x86_arm64.exe` es el instalador **oficial y sin
modificar** de ViGEmBus, el driver de bus que crea el mando de Xbox 360
virtual con el que funcionan el mando universal y la vibración de los juegos.
Va embebido en `PepoMote.exe` (`desktop/src/rumble/vigem_setup.rs`) y
PepoMote lo instala él mismo la primera vez que arranca, con la única ventana
de permiso de administrador que Windows exige para cualquier driver.

- Origen: <https://github.com/nefarius/ViGEmBus/releases/tag/v1.22.0>
  (última y definitiva versión: el proyecto se retiró en 2023 por un conflicto
  de marca; las instalaciones existentes siguen funcionando).
- Tamaño: 6 278 576 bytes.
- SHA-256: `89220a7865076b342892f98865f3499fb7c4cfd673159e89d352c360fd014c6a`
  (el receptor lo comprueba en sus tests: `vigem_setup::el_instalador_embebido_es_el_oficial`).
- Firma Authenticode válida de «Nefarius Software Solutions e.U.» (DigiCert
  Trusted G4 Code Signing), huella `1F431092EC96A80B41AB5317F53AC02EA6F9B89B`.
- Licencia: BSD-3-Clause, `ViGEmBus-LICENSE.txt` (se redistribuye el binario
  tal cual, con su aviso, como pide la licencia).
- Instalación silenciosa (Advanced Installer): `/exenoui /qn /norestart`.

**No se puede tocar ni recompilar:** es un driver de núcleo firmado. Windows
10/11 solo carga drivers con firma de Microsoft (atestación), y esa firma es
la de Nefarius sobre estos bytes exactos. Cualquier cambio dejaría el driver
sin cargar. Lo que sí es nuestro es el cliente (crate `vigem-client`, Rust
puro, por IOCTL) y todo lo que hay encima.
