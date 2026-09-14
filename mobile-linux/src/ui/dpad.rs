//! Cruceta de una sola pieza, como la del Mando de Wii: una cruz con las
//! puntas redondeadas y una marca en relieve en cada brazo, sin flechas. El
//! dedo se sigue mientras está apoyado: deslizarlo cambia de dirección sin
//! levantarlo, las esquinas entre dos brazos son diagonales y el centro
//! muerto es un círculo pequeño. Mismos números que Android (DpadModel.kt y
//! PadCross.kt) e iOS. La forma del hit-test es el cuadrado entero: las
//! esquinas fuera de la cruz también valen (diagonal).

use crate::buttons::Buttons;
use crate::theme;
use crate::ui::touch::{Canvas, Shape};
use egui::{Color32, Pos2, Rect, Rounding, Stroke, Vec2};

/// Radio del centro muerto, en medias anchuras de brazo.
pub const DEAD: f32 = 0.6;

/// Los cuatro bits de cruceta, en el orden de los brazos (↑ → ↓ ←).
pub const ARMS: [u32; 4] = [pmp::BTN_DPAD_UP, pmp::BTN_DPAD_RIGHT, pmp::BTN_DPAD_DOWN, pmp::BTN_DPAD_LEFT];

/// Bits de cruceta para un dedo en `d` (desde el centro, +y abajo) en una
/// cruz de media anchura `half`: dentro del centro muerto, ninguno; en el
/// cuadrado central, el del eje dominante; sobre un brazo, solo el suyo
/// (aunque el dedo se salga por la punta); en una esquina entre dos brazos,
/// los dos.
pub fn bits(d: Vec2, half: f32) -> u32 {
    let a = half / 3.0;
    let dead = a * DEAD;
    if d.x * d.x + d.y * d.y <= dead * dead {
        return 0;
    }
    let (ax, ay) = (d.x.abs(), d.y.abs());
    let horizontal = if d.x > 0.0 { pmp::BTN_DPAD_RIGHT } else { pmp::BTN_DPAD_LEFT };
    let vertical = if d.y > 0.0 { pmp::BTN_DPAD_DOWN } else { pmp::BTN_DPAD_UP };
    if ax <= a && ay <= a {
        if ax > ay {
            horizontal
        } else {
            vertical
        }
    } else if ay <= a {
        horizontal
    } else if ax <= a {
        vertical
    } else {
        horizontal | vertical
    }
}

/// Pasa lo pulsado de `held` a `now`: suelta lo que ya no está y pulsa lo
/// nuevo. Devuelve los bits recién pulsados (el «clic»).
pub fn apply(buttons: &Buttons, held: &mut u32, now: u32) -> u32 {
    if now == *held {
        return 0;
    }
    for bit in ARMS {
        let was = *held & bit != 0;
        let is = now & bit != 0;
        if was != is {
            buttons.set(bit, is);
        }
    }
    let fresh = now & !*held;
    *held = now;
    fresh
}

/// Geometría de la cruz: media anchura `half` → brazos de media anchura `a`
/// (un tercio) y puntas redondeadas con `r`. Las esquinas interiores van
/// vivas (la cruz se pinta como tres rectángulos).
#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub half: f32,
    pub a: f32,
    pub r: f32,
}

impl Geometry {
    pub fn new(half: f32) -> Self {
        let a = half / 3.0;
        Self { half, a, r: a * 0.55 }
    }

    /// Los tres rectángulos que forman la cruz centrada en `c`, con su
    /// redondeo: los dos brazos laterales (metidos 2 px en el centro para que
    /// no se vea la costura) y, encima, la barra vertical.
    pub fn bars(&self, c: Pos2) -> [(Rect, Rounding); 3] {
        let (h, a, r) = (self.half, self.a, self.r);
        let lap = 2.0;
        [
            (
                Rect::from_min_max(c + Vec2::new(-h, -a), c + Vec2::new(-a + lap, a)),
                Rounding { nw: r, sw: r, ne: 0.0, se: 0.0 },
            ),
            (
                Rect::from_min_max(c + Vec2::new(a - lap, -a), c + Vec2::new(h, a)),
                Rounding { ne: r, se: r, nw: 0.0, sw: 0.0 },
            ),
            (Rect::from_min_max(c + Vec2::new(-a, -h), c + Vec2::new(a, h)), Rounding::same(r)),
        ]
    }

    /// Brazo `dir` (0 ↑, 1 →, 2 ↓, 3 ←): de la arista del cuadrado central a
    /// la punta, con la punta redondeada.
    pub fn arm(&self, c: Pos2, dir: usize) -> (Rect, Rounding) {
        let (h, a, r) = (self.half, self.a, self.r);
        match dir {
            0 => (
                Rect::from_min_max(c + Vec2::new(-a, -h), c + Vec2::new(a, -a)),
                Rounding { nw: r, ne: r, sw: 0.0, se: 0.0 },
            ),
            1 => (
                Rect::from_min_max(c + Vec2::new(a, -a), c + Vec2::new(h, a)),
                Rounding { ne: r, se: r, nw: 0.0, sw: 0.0 },
            ),
            2 => (
                Rect::from_min_max(c + Vec2::new(-a, a), c + Vec2::new(a, h)),
                Rounding { sw: r, se: r, nw: 0.0, ne: 0.0 },
            ),
            _ => (
                Rect::from_min_max(c + Vec2::new(-h, -a), c + Vec2::new(-a, a)),
                Rounding { nw: r, sw: r, ne: 0.0, se: 0.0 },
            ),
        }
    }

    /// Marca en relieve del brazo `dir`, a lo largo del brazo y cerca de la punta (grosor `t`).
    pub fn mark(&self, c: Pos2, dir: usize, t: f32) -> Rect {
        let len = self.a * 0.7;
        let at = self.half * 0.72;
        let (center, size) = match dir {
            0 => (c + Vec2::new(0.0, -at), Vec2::new(t, len)),
            1 => (c + Vec2::new(at, 0.0), Vec2::new(len, t)),
            2 => (c + Vec2::new(0.0, at), Vec2::new(t, len)),
            _ => (c + Vec2::new(-at, 0.0), Vec2::new(len, t)),
        };
        Rect::from_center_size(center, size)
    }

    /// Contorno: 12 esquinas en sentido horario desde la interior superior
    /// izquierda, con las 8 puntas redondeadas (curva cuadrática aplanada en
    /// 5 puntos) y las 4 interiores vivas.
    pub fn outline(&self, c: Pos2) -> Vec<Pos2> {
        let (h, a, r) = (self.half, self.a, self.r);
        let corners = [
            (-a, -a),
            (-a, -h),
            (a, -h),
            (a, -a),
            (h, -a),
            (h, a),
            (a, a),
            (a, h),
            (-a, h),
            (-a, a),
            (-h, a),
            (-h, -a),
        ];
        let pts: Vec<Pos2> = corners.iter().map(|(x, y)| c + Vec2::new(*x, *y)).collect();
        let n = pts.len();
        let mut out = Vec::with_capacity(n * 5);
        for i in 0..n {
            let (prev, cur, next) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            if i % 3 == 0 {
                out.push(cur);
                continue;
            }
            let rr = r.min(prev.distance(cur) / 2.0).min(cur.distance(next) / 2.0);
            let start = cur + (prev - cur).normalized() * rr;
            let end = cur + (next - cur).normalized() * rr;
            for k in 0..=4 {
                let t = k as f32 / 4.0;
                out.push(start.lerp(cur, t).lerp(cur.lerp(end, t), t));
            }
        }
        out
    }
}

/// Pinta la cruceta centrada en `c` con media anchura `half` (los brazos de
/// `pressed` iluminados) y devuelve su forma para el hit-test: el cuadrado
/// entero, esquinas incluidas (diagonales).
pub fn draw(cv: &Canvas, c: Pos2, half: f32, s: f32, pressed: u32) -> Shape {
    let g = Geometry::new(half);
    // Sombra: la misma cruz un poco más abajo, en negro translúcido
    for (rc, rd) in g.bars(c + Vec2::new(0.0, 1.5 * s)) {
        cv.rect_corners(rc, rd, Color32::from_black_alpha(28), Stroke::NONE);
    }
    for (rc, rd) in g.bars(c) {
        cv.rect_corners(rc, rd, theme::card(), Stroke::NONE);
    }
    for (dir, bit) in ARMS.iter().enumerate() {
        if pressed & bit != 0 {
            let (rc, rd) = g.arm(c, dir);
            cv.rect_corners(rc, rd, theme::glow(), Stroke::NONE);
        }
    }
    cv.closed_line(&g.outline(c), Stroke::new(1.0_f32, theme::card_border()));
    let t = 2.0_f32.max(g.a * 0.14);
    for dir in 0..4 {
        cv.rounded_rect(g.mark(c, dir, t), t / 2.0, theme::card_border(), Stroke::NONE);
    }
    Shape::Rect(Rect::from_center_size(c, Vec2::splat(2.0 * half)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: f32 = 90.0; // brazos de media anchura 30, centro muerto de radio 18

    fn b(x: f32, y: f32) -> u32 {
        bits(Vec2::new(x, y), H)
    }

    #[test]
    fn centro_muerto_pequeno() {
        assert_eq!(b(0.0, 0.0), 0);
        assert_eq!(b(10.0, 10.0), 0, "a 14 del centro");
        assert_eq!(b(-17.0, 0.0), 0);
        // Justo fuera del círculo: ya es una dirección (antes, todo el cuadrado central de 60 era muerto)
        assert_eq!(b(19.0, 0.0), pmp::BTN_DPAD_RIGHT);
        assert_eq!(b(12.0, 14.0), pmp::BTN_DPAD_DOWN, "a 18,4: en el cuadrado central manda el eje dominante");
        assert_eq!(b(15.0, 12.0), pmp::BTN_DPAD_RIGHT);
        assert_eq!(b(-25.0, 0.0), pmp::BTN_DPAD_LEFT);
        assert_eq!(b(0.0, -25.0), pmp::BTN_DPAD_UP);
    }

    #[test]
    fn sobre_un_brazo_solo_su_direccion() {
        assert_eq!(b(0.0, -60.0), pmp::BTN_DPAD_UP);
        assert_eq!(b(60.0, 0.0), pmp::BTN_DPAD_RIGHT);
        assert_eq!(b(60.0, 20.0), pmp::BTN_DPAD_RIGHT, "el dedo descentrado en el brazo no añade la diagonal");
        assert_eq!(b(60.0, -30.0), pmp::BTN_DPAD_RIGHT, "hasta el borde del brazo");
        assert_eq!(b(20.0, 60.0), pmp::BTN_DPAD_DOWN);
        assert_eq!(b(-60.0, -29.0), pmp::BTN_DPAD_LEFT);
        // Fuera de la cruceta por la punta: sigue siendo esa dirección (el pulgar resbala)
        assert_eq!(b(200.0, 5.0), pmp::BTN_DPAD_RIGHT);
        assert_eq!(b(-10.0, -400.0), pmp::BTN_DPAD_UP);
    }

    #[test]
    fn las_esquinas_son_diagonales() {
        assert_eq!(b(50.0, 50.0), pmp::BTN_DPAD_DOWN | pmp::BTN_DPAD_RIGHT);
        assert_eq!(b(-40.0, 35.0), pmp::BTN_DPAD_DOWN | pmp::BTN_DPAD_LEFT);
        assert_eq!(b(-35.0, -40.0), pmp::BTN_DPAD_UP | pmp::BTN_DPAD_LEFT);
        assert_eq!(b(31.0, -31.0), pmp::BTN_DPAD_UP | pmp::BTN_DPAD_RIGHT, "justo pasada la esquina del cuadrado central");
        assert_eq!(b(200.0, 200.0), pmp::BTN_DPAD_DOWN | pmp::BTN_DPAD_RIGHT, "también fuera de la cruceta");
    }

    #[test]
    fn deslizar_cambia_de_direccion_sin_levantar() {
        let buttons = Buttons::new();
        let mut held = 0;
        assert_eq!(apply(&buttons, &mut held, pmp::BTN_DPAD_RIGHT), pmp::BTN_DPAD_RIGHT, "clic al pulsar");
        assert_eq!(buttons.physical(), pmp::BTN_DPAD_RIGHT);
        assert_eq!(apply(&buttons, &mut held, pmp::BTN_DPAD_RIGHT), 0, "sin cambio, sin clic");
        assert_eq!(
            apply(&buttons, &mut held, pmp::BTN_DPAD_RIGHT | pmp::BTN_DPAD_DOWN),
            pmp::BTN_DPAD_DOWN,
            "la diagonal añade abajo"
        );
        assert_eq!(buttons.physical(), pmp::BTN_DPAD_RIGHT | pmp::BTN_DPAD_DOWN);
        assert_eq!(apply(&buttons, &mut held, pmp::BTN_DPAD_DOWN), 0, "soltar derecha no hace clic");
        assert_eq!(buttons.physical(), pmp::BTN_DPAD_DOWN);
        assert_eq!(apply(&buttons, &mut held, pmp::BTN_DPAD_LEFT), pmp::BTN_DPAD_LEFT, "de abajo a izquierda: clic");
        assert_eq!(buttons.physical(), pmp::BTN_DPAD_LEFT);
        assert_eq!(apply(&buttons, &mut held, 0), 0);
        assert_eq!(buttons.physical(), 0);
        assert_eq!(held, 0);
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn geometria_de_la_cruz() {
        let g = Geometry::new(90.0);
        let c = Pos2::new(100.0, 100.0);
        assert!(near(g.a, 30.0) && near(g.r, 16.5));
        // Las tres barras cubren la cruz entera y no se salen de ella
        let bars = g.bars(c);
        assert!(near(bars[0].0.left(), 10.0) && near(bars[0].0.right(), 72.0), "brazo izquierdo metido 2 px: {:?}", bars[0].0);
        assert!(near(bars[1].0.left(), 128.0) && near(bars[1].0.right(), 190.0));
        assert!(near(bars[2].0.top(), 10.0) && near(bars[2].0.bottom(), 190.0) && near(bars[2].0.width(), 60.0));
        assert!(near(bars[0].1.nw, 16.5) && near(bars[0].1.ne, 0.0), "las puntas redondeadas, el centro vivo");
        // Brazos: de la arista del cuadrado central a la punta
        let (up, rd) = g.arm(c, 0);
        assert!(near(up.top(), 10.0) && near(up.bottom(), 70.0) && near(up.width(), 60.0));
        assert!(near(rd.nw, 16.5) && near(rd.sw, 0.0));
        let (right, _) = g.arm(c, 1);
        assert!(near(right.left(), 130.0) && near(right.right(), 190.0) && near(right.height(), 60.0));
        let (down, _) = g.arm(c, 2);
        assert!(near(down.top(), 130.0) && near(down.bottom(), 190.0));
        let (left, _) = g.arm(c, 3);
        assert!(near(left.left(), 10.0) && near(left.right(), 70.0));
        // Marca del brazo izquierdo: a lo largo, cerca de la punta
        let m = g.mark(c, 3, 4.0);
        assert!(near(m.center().x, 100.0 - 64.8) && near(m.center().y, 100.0));
        assert!(near(m.width(), 21.0) && near(m.height(), 4.0));
        // Contorno: 4 esquinas vivas + 8 puntas de 5 puntos, dentro del cuadrado
        let o = g.outline(c);
        assert_eq!(o.len(), 4 + 8 * 5);
        assert_eq!(o[0], Pos2::new(70.0, 70.0), "empieza en la esquina interior superior izquierda");
        for p in &o {
            assert!(p.x >= 10.0 - 1e-3 && p.x <= 190.0 + 1e-3 && p.y >= 10.0 - 1e-3 && p.y <= 190.0 + 1e-3, "{p:?}");
        }
        // La punta superior izquierda pasa por su curva: ningún punto del contorno toca la esquina viva (70, 10)
        assert!(o.iter().all(|p| p.distance(Pos2::new(70.0, 10.0)) > 1.0));
    }
}
