//! The support unit: an original floating companion drawn with vector shapes — a faceted
//! diamond with a lit core, two tilted rings orbiting it with nodes running along them, and an
//! eye that follows the mouse and blinks. The light's color shows the connection state, and a
//! pulse goes out when something happens.

use std::f32::consts::{PI, TAU};

use eframe::egui::{vec2, Color32, Painter, Pos2, Shape, Stroke, Vec2};

use crate::scene::{glow, light};

pub struct Drone {
    eye: Vec2,
    next_blink: f32,
    pulse_at: f32,
}

impl Default for Drone {
    fn default() -> Self {
        Self {
            eye: Vec2::ZERO,
            next_blink: 2.5,
            pulse_at: -10.0,
        }
    }
}

const SHELL: Color32 = Color32::from_rgb(0x2c, 0x2b, 0x27);
const SHELL_LIGHT: Color32 = Color32::from_rgb(0x4a, 0x48, 0x41);
const EDGE: Color32 = Color32::from_rgb(0xdd, 0xd7, 0xbe);

/// Points of a tilted ellipse (the ring), with a depth value for each (1 = in front).
fn ring(c: Pos2, rx: f32, ry: f32, tilt: f32, n: usize) -> Vec<(Pos2, f32)> {
    let (st, ct) = tilt.sin_cos();
    (0..=n)
        .map(|k| {
            let a = k as f32 / n as f32 * TAU;
            let (x, y) = (a.cos() * rx, a.sin() * ry);
            (c + vec2(x * ct - y * st, x * st + y * ct), a.sin())
        })
        .collect()
}

fn draw_ring(painter: &Painter, pts: &[(Pos2, f32)], color: Color32, front: bool) {
    for w in pts.windows(2) {
        let depth = (w[0].1 + w[1].1) * 0.5;
        if (depth > 0.0) == front {
            let a = if front { 0.95 } else { 0.35 };
            painter.line_segment(
                [w[0].0, w[1].0],
                Stroke::new(if front { 1.6 } else { 1.0 }, color.gamma_multiply(a)),
            );
        }
    }
}

impl Drone {
    pub fn pulse(&mut self, t: f32) {
        self.pulse_at = t;
    }

    /// Draws the unit around `center` (scale 1 ≈ 110 px across with the rings).
    pub fn paint(
        &mut self,
        painter: &Painter,
        center: Pos2,
        s: f32,
        t: f32,
        pointer: Option<Pos2>,
        light_color: Color32,
    ) {
        let c = center + vec2((t * 0.7).sin() * 3.0 * s, (t * 1.6).sin() * 7.0 * s);

        // Light cast below it.
        let ground = center + vec2(0.0, 78.0 * s);
        for k in 0..4 {
            let f = k as f32 / 4.0;
            let w = (34.0 - 6.0 * f) * s * (1.0 - 0.15 * (t * 1.6).sin());
            painter.add(Shape::ellipse_filled(
                ground,
                vec2(w * (1.0 + f), w * 0.18 * (1.0 + f)),
                light(light_color, 0.10 * (1.0 - f)),
            ));
        }
        // A faint beam.
        painter.add(Shape::convex_polygon(
            vec![
                c + vec2(-4.0 * s, 30.0 * s),
                c + vec2(4.0 * s, 30.0 * s),
                ground + vec2(26.0 * s, 0.0),
                ground + vec2(-26.0 * s, 0.0),
            ],
            light(light_color, 0.06),
            Stroke::NONE,
        ));

        let ring_a = ring(c, 52.0 * s, 14.0 * s, -0.32 + (t * 0.4).sin() * 0.05, 64);
        let ring_b = ring(c, 44.0 * s, 10.0 * s, 0.45 + (t * 0.5).cos() * 0.05, 64);
        draw_ring(painter, &ring_a, EDGE, false);
        draw_ring(painter, &ring_b, light_color, false);

        glow(painter, c, 70.0 * s, light_color, 0.55);

        // The faceted shell: an upper and a lower diamond, lit from the top left.
        let top = c + vec2(0.0, -34.0 * s);
        let bottom = c + vec2(0.0, 30.0 * s);
        let left = c + vec2(-22.0 * s, -2.0 * s);
        let right = c + vec2(22.0 * s, -2.0 * s);
        let front = c + vec2(0.0, 2.0 * s);
        painter.add(Shape::convex_polygon(
            vec![top, right, bottom, left],
            SHELL,
            Stroke::new(1.4 * s, EDGE.gamma_multiply(0.85)),
        ));
        painter.add(Shape::convex_polygon(
            vec![top, front, left],
            SHELL_LIGHT,
            Stroke::NONE,
        ));
        painter.add(Shape::convex_polygon(
            vec![left, front, bottom],
            SHELL.gamma_multiply(0.85),
            Stroke::NONE,
        ));
        painter.line_segment(
            [top, front],
            Stroke::new(1.0 * s, EDGE.gamma_multiply(0.35)),
        );
        painter.line_segment(
            [left, right],
            Stroke::new(1.0 * s, EDGE.gamma_multiply(0.25)),
        );
        // Small fins.
        for side in [-1.0f32, 1.0] {
            let flap = (t * 2.1 + side).sin() * 2.0 * s;
            let base = c + vec2(side * 24.0 * s, -2.0 * s);
            painter.add(Shape::convex_polygon(
                vec![
                    base,
                    base + vec2(side * 12.0 * s, -8.0 * s + flap),
                    base + vec2(side * 14.0 * s, 4.0 * s + flap),
                ],
                SHELL,
                Stroke::new(1.0 * s, EDGE.gamma_multiply(0.6)),
            ));
        }

        // The eye: looks toward the pointer, blinks now and then.
        let want = pointer
            .map(|p| {
                let d = p - c;
                let len = d.length().max(1.0);
                vec2(d.x / len * 6.0 * s, d.y / len * 4.0 * s)
            })
            .unwrap_or(vec2((t * 0.5).sin() * 4.0 * s, 0.0));
        self.eye += (want - self.eye) * 0.12;
        if t > self.next_blink + 0.13 {
            self.next_blink = t + 2.2 + 3.0 * crate::scene::hash((t * 10.0) as u32);
        }
        let open = if t > self.next_blink { 0.15 } else { 1.0 };
        let eye = c + vec2(0.0, -4.0 * s) + self.eye;
        glow(painter, eye, 26.0 * s, light_color, 1.0);
        painter.add(Shape::ellipse_filled(
            eye,
            vec2(8.0 * s, 4.5 * s * open),
            light(light_color, 1.0),
        ));
        painter.add(Shape::ellipse_filled(
            eye,
            vec2(3.2 * s, 2.2 * s * open),
            light(Color32::WHITE, 0.9),
        ));

        draw_ring(painter, &ring_a, EDGE, true);
        draw_ring(painter, &ring_b, light_color, true);
        // Nodes running along the rings.
        for (pts, speed, color) in [(&ring_a, 0.09, EDGE), (&ring_b, -0.13, light_color)] {
            for k in 0..3 {
                let f = ((t * speed + k as f32 / 3.0).rem_euclid(1.0) * (pts.len() - 1) as f32)
                    as usize;
                let (p, depth) = pts[f];
                if depth > 0.0 {
                    painter.rect_filled(
                        eframe::egui::Rect::from_center_size(p, vec2(4.0 * s, 4.0 * s)),
                        0.0,
                        color,
                    );
                } else {
                    painter.circle_filled(p, 1.5 * s, color.gamma_multiply(0.4));
                }
            }
        }

        // Event pulse.
        let age = t - self.pulse_at;
        if (0.0..1.1).contains(&age) {
            let f = age / 1.1;
            let r = (40.0 + 100.0 * f) * s;
            painter.add(Shape::ellipse_stroke(
                c,
                vec2(r, r * 0.42),
                Stroke::new(2.0 * s * (1.0 - f), light(light_color, 1.0 - f)),
            ));
            for k in 0..4 {
                let a = k as f32 * PI / 2.0 + PI / 4.0;
                let p = c + vec2(a.cos(), a.sin() * 0.42) * r;
                painter.rect_filled(
                    eframe::egui::Rect::from_center_size(p, vec2(4.0, 4.0) * s),
                    0.0,
                    light(light_color, 1.0 - f),
                );
            }
        }
    }
}
