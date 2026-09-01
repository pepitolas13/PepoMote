# LEGAL — checklist de originalidad

PepoMote es un proyecto independiente. No está afiliado, respaldado ni patrocinado por Nintendo. Esta checklist se audita antes de cada release (obligatoria en h5).

## Reglas

1. **Cero assets de Nintendo**: ninguna imagen, modelo, sonido, sample, fuente tipográfica, icono o textura procedente de Nintendo o de la Wii (ni extraída, ni redibujada píxel a píxel).
2. **Marcas**: "Wii", "Wiimote", "Nintendo" no aparecen en el nombre de la app, del paquete (`dev.pepotech.pepomote`), de los binarios ni de los repositorios. En descripciones solo uso **nominativo** para describir compatibilidad: "juega juegos de Wii vía el emulador Dolphin", "estilo inspirado en la estética de la Wii".
3. **Cursor**: no se replica el cursor-mano de la Wii. Usamos el cursor del sistema o diseño propio (aro + punto).
4. **Sonidos**: 100 % sintetizados por `assets/sounds-src/gen_sounds.py`. Prohibido samplear menús de la consola.
5. **Tipografía**: Nunito (SIL OFL). No usar la fuente de la Wii ni clones.
6. **Paleta y layout**: inspiración de alto nivel (fondos claros, tarjetas redondeadas, azul) — no copia de pantallas concretas del menú de la Wii.
7. **Dolphin**: es un proyecto GPL independiente; solo nos conectamos por su interfaz DSU pública. No distribuimos Dolphin ni juegos, ni enlazamos a ROMs/ISOs. La documentación asume que el usuario posee sus juegos.
8. **Capturas del README**: solo UI propia. Si aparece Dolphin, sin artwork de juegos de Nintendo.

## Licencias de terceros incluidas

| Cosa | Licencia | Dónde |
|---|---|---|
| Nunito | SIL OFL 1.1 | `android/.../res/font/nunito.ttf`, `desktop/Nunito.ttf` |
| Crates Rust / libs Android | según cada una (MIT/Apache-2.0 típicamente) | `cargo license` / Gradle licenses en h5 |
