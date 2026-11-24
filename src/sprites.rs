//! The support drone: an original floating companion drawn with vector shapes. It bobs in the
//! air, its eye follows the mouse and blinks, the eye color shows the connection state, and a
//! pulse ring goes out when something happens (a cheat toggled, an enemy respawned).

use std::f32::consts::TAU;

use eframe::egui::{pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2};

use crate::scene::{glow, light};
use crate::theme::rgba;

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

const BODY: Color32 = Color32::from_rgb(0x26, 0x26, 0x2d);
const BODY_LIGHT: Color32 = Color32::from_rgb(0x3a, 0x3a, 0x44);
const TRIM: Color32 = Color32::from_rgb(0xd9, 0xd2, 0xb8);
const PLATE: Color32 = Color32::from_rgb(0x12, 0x12, 0x18);

impl Drone {
    /// Sends out a pulse ring (call when something happens).
    pub fn pulse(&mut self, t: f32) {
        self.pulse_at = t;
    }

    /// Draws the drone around `center`, `s` = scale (1.0 ≈ 80 px wide).
    pub fn paint(
        &mut self,
        painter: &Painter,
        center: Pos2,
        s: f32,
        t: f32,
        pointer: Option<Pos2>,
        eye_color: Color32,
    ) {
        let bob = (t * 1.7).sin() * 6.0 * s;
        let c = center + vec2(0.0, bob);

        // Thruster flame and its light on the "ground".
        let flicker = 0.75 + 0.25 * (t * 31.0).sin() * (t * 17.0).cos();
        let nozzle = c + vec2(0.0, 30.0 * s);
        glow(
            painter,
            nozzle + vec2(0.0, 10.0 * s),
            34.0 * s,
            eye_color,
            0.7 * flicker,
        );
        painter.add(Shape::convex_polygon(
            vec![
                nozzle + vec2(-7.0 * s, 0.0),
                nozzle + vec2(7.0 * s, 0.0),
                nozzle + vec2(0.0, (18.0 + 8.0 * flicker) * s),
            ],
            light(eye_color, 0.9),
            Stroke::NONE,
        ));
        painter.rect_filled(
            Rect::from_center_size(nozzle + vec2(0.0, -2.0 * s), vec2(18.0 * s, 6.0 * s)),
            2.0 * s,
            BODY_LIGHT,
        );

        // Side fins, flapping a little.
        for side in [-1.0f32, 1.0] {
            let flap = (t * 2.3 + side).sin() * 3.0 * s;
            let root = c + vec2(side * 38.0 * s, -4.0 * s);
            let pts = vec![
                root + vec2(0.0, -10.0 * s),
                root + vec2(side * 18.0 * s, -16.0 * s + flap),
                root + vec2(side * 22.0 * s, 6.0 * s + flap),
                root + vec2(0.0, 10.0 * s),
            ];
            painter.add(Shape::convex_polygon(
                pts,
                BODY,
                Stroke::new(1.2 * s, TRIM.gamma_multiply(0.6)),
            ));
            painter.circle_filled(
                root + vec2(side * 17.0 * s, -3.0 * s + flap),
                1.8 * s,
                light(eye_color, 0.8),
            );
        }

        // Body.
        let body = Rect::from_center_size(c, vec2(78.0 * s, 56.0 * s));
        painter.rect_filled(
            body.translate(vec2(0.0, 4.0 * s)),
            10.0 * s,
            rgba(0, 0, 0, 0.35),
        );
        painter.rect_filled(body, 10.0 * s, BODY);
        painter.rect_filled(
            Rect::from_min_size(
                body.min + vec2(4.0 * s, 3.0 * s),
                vec2(body.width() - 8.0 * s, 10.0 * s),
            ),
            6.0 * s,
            BODY_LIGHT,
        );
        painter.rect_stroke(
            body,
            10.0 * s,
            Stroke::new(1.4 * s, TRIM.gamma_multiply(0.7)),
            eframe::egui::StrokeKind::Inside,
        );
        // Panel lines.
        for k in [-1.0f32, 1.0] {
            let x = c.x + k * 26.0 * s;
            painter.line_segment(
                [
                    pos2(x, body.top() + 16.0 * s),
                    pos2(x, body.bottom() - 6.0 * s),
                ],
                Stroke::new(1.0 * s, TRIM.gamma_multiply(0.25)),
            );
        }

        // Antenna with a blinking tip.
        let base = pos2(c.x + 20.0 * s, body.top());
        let tip = base + vec2(6.0 * s, -16.0 * s);
        painter.line_segment([base, tip], Stroke::new(1.5 * s, TRIM.gamma_multiply(0.8)));
        let beat = ((t * 1.2) % 1.0 < 0.12) as u8 as f32;
        painter.circle_filled(tip, 2.4 * s, light(eye_color, 0.4 + 0.6 * beat));
        glow(painter, tip, 10.0 * s, eye_color, 0.3 + 0.7 * beat);

        // Face plate and the visor.
        let plate = Rect::from_center_size(c + vec2(0.0, 6.0 * s), vec2(46.0 * s, 26.0 * s));
        painter.rect_filled(plate, 6.0 * s, PLATE);
        painter.rect_stroke(
            plate,
            6.0 * s,
            Stroke::new(1.0 * s, TRIM.gamma_multiply(0.35)),
            eframe::egui::StrokeKind::Inside,
        );

        // The eye looks toward the pointer (smoothed) and blinks now and then.
        let want = pointer
            .map(|p| {
                let d = p - plate.center();
                let len = d.length().max(1.0);
                vec2(
                    d.x / len * (12.0 * s).min(len * 0.1 + 2.0),
                    d.y / len * (5.0 * s).min(len * 0.05 + 1.0),
                )
            })
            .unwrap_or(vec2((t * 0.5).sin() * 8.0 * s, 0.0));
        self.eye += (want - self.eye) * 0.15;
        if t > self.next_blink + 0.14 {
            self.next_blink = t + 2.0 + 3.0 * crate::scene::hash((t * 10.0) as u32);
        }
        let blinking = t > self.next_blink;
        let eye = plate.center() + self.eye;
        let eye_h = if blinking { 1.2 * s } else { 9.0 * s };
        glow(painter, eye, 22.0 * s, eye_color, 0.9);
        painter.rect_filled(
            Rect::from_center_size(eye, vec2(14.0 * s, eye_h)),
            3.0 * s,
            light(eye_color, 1.0),
        );
        painter.rect_filled(
            Rect::from_center_size(eye, vec2(6.0 * s, eye_h * 0.5)),
            2.0 * s,
            light(Color32::WHITE, 0.8),
        );
        // Scan line across the visor.
        let scan = plate.top() + ((t * 0.7) % 1.0) * plate.height();
        painter.line_segment(
            [
                pos2(plate.left() + 3.0 * s, scan),
                pos2(plate.right() - 3.0 * s, scan),
            ],
            Stroke::new(1.0 * s, light(eye_color, 0.18)),
        );

        // Pulse ring.
        let age = t - self.pulse_at;
        if (0.0..1.2).contains(&age) {
            let f = age / 1.2;
            painter.circle_stroke(
                c,
                (50.0 + 90.0 * f) * s,
                Stroke::new(2.0 * s * (1.0 - f), light(eye_color, 1.0 - f)),
            );
            for k in 0..8 {
                let a = k as f32 / 8.0 * TAU + age;
                let d = (40.0 + 70.0 * f) * s;
                painter.circle_filled(
                    c + vec2(a.cos(), a.sin()) * d,
                    2.0 * s * (1.0 - f),
                    light(eye_color, 1.0 - f),
                );
            }
        }
    }
}
