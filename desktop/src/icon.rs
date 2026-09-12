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

/// El logo con un punto verde abajo a la derecha (bandeja: hay móviles
/// conectados). Un aro transparente de 1 px lo separa del logo.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn logo_rgba_badge(size: u32) -> Vec<u8> {
    let mut out = logo_rgba(size);
    let s = size as f32;
    let (bx, by, br) = (s * 0.78, s * 0.78, s * 0.19);
    let green = [0x7B, 0xC9, 0x4C, 255];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - bx;
            let dy = y as f32 + 0.5 - by;
            let d = (dx * dx + dy * dy).sqrt();
            let i = ((y * size + x) * 4) as usize;
            if d < br {
                out[i..i + 4].copy_from_slice(&green);
            } else if d < br + 1.0 {
                out[i..i + 4].copy_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(buf: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
    }

    #[test]
    fn el_logo_tiene_aro_y_punto_sobre_transparente() {
        let s = 32;
        let logo = logo_rgba(s);
        assert_eq!(logo.len(), (s * s * 4) as usize);
        assert_eq!(px(&logo, s, 16, 16), [0x3F, 0xA9, 0xF5, 255], "punto central");
        assert_eq!(px(&logo, s, 16, 2)[3], 255, "aro arriba");
        assert_eq!(px(&logo, s, 16, 8)[3], 0, "hueco entre punto y aro");
        assert_eq!(px(&logo, s, 0, 0)[3], 0, "esquina transparente");
    }

    #[test]
    fn el_badge_es_verde_y_recorta_el_aro_a_su_alrededor() {
        let s = 32;
        let plain = logo_rgba(s);
        let badge = logo_rgba_badge(s);
        assert_eq!(px(&badge, s, 25, 25), [0x7B, 0xC9, 0x4C, 255], "centro del badge");
        // los píxeles del aro transparente del badge que en el logo eran azules
        let (bx, by, br) = (s as f32 * 0.78, s as f32 * 0.78, s as f32 * 0.19);
        let mut checked = 0;
        for y in 0..s {
            for x in 0..s {
                let d = ((x as f32 + 0.5 - bx).powi(2) + (y as f32 + 0.5 - by).powi(2)).sqrt();
                if (br..br + 1.0).contains(&d) && px(&plain, s, x, y)[3] == 255 {
                    assert_eq!(px(&badge, s, x, y)[3], 0, "({x},{y}) debe quedar transparente");
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "el aro del badge cruza el aro del logo");
        // lejos del badge, el logo sigue igual
        assert_eq!(px(&badge, s, 16, 16), px(&plain, s, 16, 16));
        assert_eq!(px(&badge, s, 16, 2), px(&plain, s, 16, 2));
    }
}
