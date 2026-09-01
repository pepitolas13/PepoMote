//! Icono procedural (aro + punto, el logo) — sin assets binarios.

/// RGBA size×size con el aro azul y el punto central sobre fondo transparente.
pub fn logo_rgba(size: u32) -> Vec<u8> {
    let s = size as f32;
    let c = s / 2.0;
    let r_outer = s * 0.42;
    let r_ring = s * 0.10;
    let r_dot = s * 0.13;
    let blue = (0x3F, 0xA9, 0xF5);

    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - c;
            let dy = y as f32 + 0.5 - c;
            let d = (dx * dx + dy * dy).sqrt();
            let ring = (d - r_outer).abs() < r_ring;
            let dot = d < r_dot;
            if ring || dot {
                out.extend_from_slice(&[blue.0, blue.1, blue.2, 255]);
            } else {
                out.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    out
}
