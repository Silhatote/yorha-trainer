//! The scenery behind the interface: the pictures in `assets/` (the lily field at night and
//! the ruins in the fog), slowly drifting, with animated lights and dust on top. Without the
//! pictures the same two scenes are painted procedurally. The user can also pick their own.

use std::f32::consts::{PI, TAU};
use std::path::{Path, PathBuf};

use eframe::egui::{self, pos2, vec2, Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke};

use crate::settings::Scenery;

/// Stable pseudorandom number in 0..1 for an index (same value every frame).
pub fn hash(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x & 0xFFFF) as f32 / 65535.0
}

fn rgb(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Normal blending with opacity `a`.
fn alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}

/// Additive light (premultiplied color with zero alpha adds to what is below).
pub fn light(c: Color32, k: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (c.r() as f32 * k) as u8,
        (c.g() as f32 * k) as u8,
        (c.b() as f32 * k) as u8,
        0,
    )
}

/// A vertical gradient over `r`.
pub fn gradient(painter: &Painter, r: Rect, top: Color32, bottom: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(r.left_top(), top);
    m.colored_vertex(r.right_top(), top);
    m.colored_vertex(r.left_bottom(), bottom);
    m.colored_vertex(r.right_bottom(), bottom);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 2, 3);
    painter.add(m);
}

/// A soft round glow made of a few additive rings.
pub fn glow(painter: &Painter, c: Pos2, radius: f32, color: Color32, k: f32) {
    const RINGS: usize = 7;
    for i in 0..RINGS {
        let f = 1.0 - i as f32 / RINGS as f32;
        painter.circle_filled(c, radius * f, light(color, k * 0.22 * (1.0 - f * 0.6)));
    }
}

/// The pictures in `assets/` (empty when they weren't there at build time).
mod pictures {
    include!(concat!(env!("OUT_DIR"), "/backgrounds.rs"));
}

/// A built-in picture, decoded the first time it's shown.
#[derive(Default)]
struct Picture {
    tex: Option<egui::TextureHandle>,
    failed: bool,
}

impl Picture {
    fn get(
        &mut self,
        ctx: &egui::Context,
        name: &str,
        bytes: &[u8],
    ) -> Option<&egui::TextureHandle> {
        if self.tex.is_none() && !self.failed {
            self.tex = decode(ctx, name, bytes);
            self.failed = self.tex.is_none();
        }
        self.tex.as_ref()
    }
}

fn decode(ctx: &egui::Context, name: &str, bytes: &[u8]) -> Option<egui::TextureHandle> {
    if bytes.is_empty() {
        return None;
    }
    let img = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture(name, color, egui::TextureOptions::LINEAR))
}

/// Draws a picture as "cover" around a focal point, slightly zoomed and drifting slowly.
fn living_picture(
    painter: &Painter,
    rect: Rect,
    tex: &egui::TextureHandle,
    focus: egui::Vec2,
    t: f32,
) {
    let art = tex.size_vec2();
    let phase = 1.0 - (((t / 60.0) % 2.0) - 1.0).abs(); // 0 → 1 → 0 over two minutes
    let phase = phase * phase * (3.0 - 2.0 * phase);
    let scale = 1.04 + 0.05 * phase;
    let drift = vec2(-0.01 + 0.02 * phase, 0.008 - 0.016 * phase);
    let s = (rect.width() / art.x).max(rect.height() / art.y) * scale;
    let view = rect.size() / (art * s);
    let min = vec2(
        ((1.0 - view.x) * focus.x + drift.x).clamp(0.0, (1.0 - view.x).max(0.0)),
        ((1.0 - view.y) * focus.y + drift.y).clamp(0.0, (1.0 - view.y).max(0.0)),
    );
    let uv = Rect::from_min_size(pos2(min.x, min.y), view);
    painter.image(tex.id(), rect, uv, Color32::WHITE);
}

#[derive(Default)]
pub struct Scene {
    night_picture: Picture,
    ruins_picture: Picture,
    custom: Option<(PathBuf, egui::TextureHandle)>,
    failed: Option<PathBuf>,
}

impl Scene {
    /// Paints the scenery into `rect` at time `t` (seconds).
    pub fn paint(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        rect: Rect,
        scenery: Scenery,
        custom: Option<&Path>,
        t: f32,
    ) {
        match scenery {
            Scenery::Night => match self.night_picture.get(ctx, "night", pictures::NIGHT) {
                Some(tex) => {
                    // Portrait picture: show the band with the moon's glow and the field.
                    living_picture(painter, rect, tex, vec2(0.5, 0.52), t);
                    gradient(
                        painter,
                        rect,
                        Color32::from_black_alpha(40),
                        Color32::from_black_alpha(10),
                    );
                    motes(painter, rect, t, rgb(170, 205, 255), 55);
                }
                None => night(painter, rect, t),
            },
            Scenery::Ruins => match self.ruins_picture.get(ctx, "ruins", pictures::RUINS) {
                Some(tex) => {
                    living_picture(painter, rect, tex, vec2(0.5, 0.5), t);
                    dust(painter, rect, t);
                }
                None => ruins(painter, rect, t),
            },
            Scenery::Custom => {
                if !self.custom_picture(ctx, painter, rect, custom) {
                    night(painter, rect, t);
                }
                motes(painter, rect, t, rgb(200, 220, 255), 40);
            }
        }
    }

    /// Whether the user's picture could be shown.
    fn custom_picture(
        &mut self,
        ctx: &egui::Context,
        painter: &Painter,
        rect: Rect,
        path: Option<&Path>,
    ) -> bool {
        let Some(path) = path else {
            return false;
        };
        if self.custom.as_ref().is_none_or(|(p, _)| p != path) {
            self.custom = None;
            if self.failed.as_deref() == Some(path) {
                return false;
            }
            match load_picture(ctx, path) {
                Some(tex) => self.custom = Some((path.to_path_buf(), tex)),
                None => {
                    self.failed = Some(path.to_path_buf());
                    return false;
                }
            }
        }
        let Some((_, tex)) = &self.custom else {
            return false;
        };
        // "Cover": fill the window, cropping the excess around the center.
        let art = tex.size_vec2();
        let s = (rect.width() / art.x).max(rect.height() / art.y);
        let view = rect.size() / (art * s);
        let min = (vec2(1.0, 1.0) - view) * 0.5;
        let uv = Rect::from_min_size(pos2(min.x, min.y), view);
        painter.image(tex.id(), rect, uv, Color32::WHITE);
        // A veil so the panels stay readable over any picture.
        gradient(
            painter,
            rect,
            Color32::from_black_alpha(70),
            Color32::from_black_alpha(130),
        );
        true
    }
}

fn load_picture(ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
    let img = image::open(path).ok()?;
    // Big photos would waste GPU memory: 2560 px wide is plenty for a background.
    let img = if img.width() > 2560 {
        img.resize(
            2560,
            2560 * img.height() / img.width(),
            image::imageops::FilterType::Triangle,
        )
    } else {
        img
    };
    let img = img.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    Some(ctx.load_texture("custom-background", color, egui::TextureOptions::LINEAR))
}

// ---------------------------------------------------------------------------------------
// Night: the lily field.

fn night(painter: &Painter, r: Rect, t: f32) {
    let (w, h) = (r.width(), r.height());
    let horizon = r.top() + h * 0.63;

    // Sky.
    let mid = r.top() + h * 0.45;
    gradient(
        painter,
        Rect::from_x_y_ranges(r.x_range(), r.top()..=mid),
        rgb(3, 5, 18),
        rgb(16, 20, 50),
    );
    gradient(
        painter,
        Rect::from_x_y_ranges(r.x_range(), mid..=horizon),
        rgb(16, 20, 50),
        rgb(38, 42, 82),
    );

    // Stars, twinkling; a few bright ones with a cross.
    for i in 0..260u32 {
        let x = r.left() + hash(i * 3) * w;
        let y = r.top() + hash(i * 3 + 1).powf(1.4) * (horizon - r.top()) * 0.95;
        let base = 0.25 + 0.75 * hash(i * 3 + 2);
        let tw = 0.55 + 0.45 * (t * (0.6 + 2.2 * hash(i + 999)) + i as f32).sin();
        let a = base * tw;
        let size = 0.5 + 1.3 * hash(i * 7 + 5).powf(3.0);
        painter.circle_filled(pos2(x, y), size, light(rgb(225, 230, 255), a));
        if i % 37 == 0 {
            let l = 5.0 + 6.0 * a;
            let c = light(rgb(200, 215, 255), a * 0.6);
            painter.line_segment([pos2(x - l, y), pos2(x + l, y)], Stroke::new(1.0, c));
            painter.line_segment([pos2(x, y - l), pos2(x, y + l)], Stroke::new(1.0, c));
            glow(painter, pos2(x, y), 9.0, rgb(170, 200, 255), a);
        }
    }

    // The moon.
    let moon = pos2(r.left() + w * 0.3, r.top() + h * 0.3);
    let mr = h.min(w) * 0.045;
    glow(painter, moon, mr * 9.0, rgb(120, 140, 220), 0.7);
    glow(painter, moon, mr * 3.0, rgb(200, 210, 255), 0.9);
    painter.circle_filled(moon, mr, rgb(232, 230, 240));
    for k in 0..6u32 {
        let a = hash(k + 300) * TAU;
        let d = hash(k + 310) * mr * 0.6;
        let cr = mr * (0.1 + 0.16 * hash(k + 320));
        painter.circle_filled(
            moon + vec2(a.cos(), a.sin()) * d,
            cr,
            alpha(rgb(190, 188, 205), 0.6),
        );
    }

    // Clouds drifting across the moon.
    for i in 0..9u32 {
        let speed = 4.0 + 6.0 * hash(i + 50);
        let span = w * 1.6;
        let x = r.left() - w * 0.3 + ((hash(i + 60) * span + t * speed) % span);
        let y = r.top() + h * (0.18 + 0.32 * hash(i + 70));
        let len = w * (0.12 + 0.18 * hash(i + 80));
        for k in 0..7 {
            let f = k as f32 / 6.0;
            let cx = x + (f - 0.5) * len;
            let cy = y + (f * PI).sin() * -h * 0.012;
            let cr = h * (0.02 + 0.025 * (f * PI).sin());
            painter.circle_filled(pos2(cx, cy), cr, alpha(rgb(30, 34, 70), 0.22));
        }
    }

    // The ruined city: a far layer and a near one, with a few lit windows.
    skyline(painter, r, horizon, 11, h * 0.24, rgb(26, 30, 64), t, 0.6);
    skyline(
        painter,
        r,
        horizon + 2.0,
        23,
        h * 0.16,
        rgb(13, 16, 38),
        t,
        1.0,
    );

    // The field.
    gradient(
        painter,
        Rect::from_x_y_ranges(r.x_range(), horizon..=r.bottom()),
        rgb(14, 18, 44),
        rgb(4, 6, 14),
    );
    // A haze of light low over the flowers.
    gradient(
        painter,
        Rect::from_x_y_ranges(r.x_range(), horizon..=(horizon + h * 0.12)),
        light(rgb(90, 120, 200), 0.18),
        light(rgb(90, 120, 200), 0.0),
    );

    let ground = r.bottom() - horizon;
    // Distant glints.
    for i in 0..180u32 {
        let u = hash(i + 2000).powf(2.2);
        let y = horizon + u * ground;
        let x = r.left() + hash(i + 2100) * w;
        let a = (0.3 + 0.5 * hash(i + 2200)) * (0.7 + 0.3 * (t * 1.3 + i as f32).sin());
        let s = 0.8 + u * 3.0;
        painter.circle_filled(pos2(x, y), s, light(rgb(190, 215, 255), a));
        glow(painter, pos2(x, y), s * 4.0, rgb(120, 160, 255), a * 0.6);
    }
    // Lilies, from the back to the front.
    const LILIES: u32 = 40;
    for i in 0..LILIES {
        let u = (i as f32 + hash(i + 3000)) / LILIES as f32;
        let u = u.powf(1.25);
        let y = horizon + ground * (0.06 + 0.98 * u);
        let x = r.left() + hash(i * 5 + 3100) * w;
        let size = h * (0.010 + 0.055 * u * u);
        let phase = hash(i + 3200) * TAU;
        lily(painter, pos2(x, y), size, t, phase, 0.55 + 0.45 * u);
    }
    motes(painter, r, t, rgb(170, 205, 255), 60);
}

/// A row of ruined buildings standing on `base`.
#[allow(clippy::too_many_arguments)]
fn skyline(
    painter: &Painter,
    r: Rect,
    base: f32,
    seed: u32,
    max_h: f32,
    color: Color32,
    t: f32,
    lit: f32,
) {
    let mut x = r.left() - 20.0;
    let mut i = 0u32;
    while x < r.right() + 20.0 {
        let k = seed * 1000 + i;
        let bw = 18.0 + 60.0 * hash(k);
        let bh = max_h * (0.25 + 0.75 * hash(k + 1).powf(1.5));
        let top = base - bh;
        // Broken roofs: a slanted top on some of them.
        let cut = if hash(k + 2) > 0.55 {
            bh * 0.18 * hash(k + 3)
        } else {
            0.0
        };
        let left_high = hash(k + 4) > 0.5;
        let pts = vec![
            pos2(x, base),
            pos2(x, top + if left_high { 0.0 } else { cut }),
            pos2(x + bw, top + if left_high { cut } else { 0.0 }),
            pos2(x + bw, base),
        ];
        painter.add(Shape::convex_polygon(pts, color, Stroke::NONE));
        // An antenna or a mast on the tall ones.
        if bh > max_h * 0.7 && hash(k + 5) > 0.5 {
            let mx = x + bw * (0.3 + 0.4 * hash(k + 6));
            painter.line_segment(
                [pos2(mx, top), pos2(mx, top - bh * 0.25)],
                Stroke::new(1.5, color),
            );
        }
        // Windows.
        let cols = (bw / 9.0) as u32;
        let rows = (bh / 11.0) as u32;
        for cx in 0..cols {
            for cy in 1..rows {
                let wk = k * 131 + cx * 17 + cy;
                if hash(wk) > 0.93 {
                    let flicker = 0.6 + 0.4 * (t * (0.5 + hash(wk + 1)) + wk as f32).sin().max(0.0);
                    let c = if hash(wk + 2) > 0.5 {
                        rgb(255, 214, 150)
                    } else {
                        rgb(170, 210, 255)
                    };
                    let p = pos2(x + 4.0 + cx as f32 * 9.0, top + cy as f32 * 11.0);
                    painter.rect_filled(
                        Rect::from_min_size(p, vec2(3.0, 4.0)),
                        0.0,
                        light(c, 0.55 * lit * flicker),
                    );
                }
            }
        }
        x += bw + 2.0 + 10.0 * hash(k + 7);
        i += 1;
    }
}

/// A glowing lily seen from above and in front.
pub fn lily(painter: &Painter, c: Pos2, size: f32, t: f32, phase: f32, k: f32) {
    let sway = (t * 0.8 + phase).sin() * 0.07;
    let c = c + vec2((t * 0.55 + phase).sin() * size * 0.04, 0.0);
    glow(painter, c, size * 1.6, rgb(120, 165, 255), 0.45 * k);
    // Back petals, then front petals, squashed vertically for the perspective.
    for (layer, offset, color, len) in [
        (0, PI / 6.0, rgb(150, 175, 235), 0.9),
        (1, 0.0, rgb(226, 234, 255), 1.0),
    ] {
        for p in 0..3 {
            let a = -PI / 2.0 + offset + p as f32 * TAU / 3.0 + sway;
            let pts = petal(c, a, size * len, size * 0.34);
            let fill = alpha(color, if layer == 0 { 0.75 } else { 0.92 } * k + 0.08);
            painter.add(Shape::convex_polygon(
                pts,
                fill,
                Stroke::new(0.6, light(rgb(200, 220, 255), 0.5 * k)),
            ));
        }
    }
    // Stamens.
    for s in 0..5 {
        let a = -PI / 2.0 + (s as f32 - 2.0) * 0.32 + sway;
        let tip = c + vec2(a.cos(), a.sin() * 0.62) * size * 0.55;
        painter.line_segment(
            [c, tip],
            Stroke::new(0.8, alpha(rgb(235, 225, 190), 0.7 * k)),
        );
        painter.circle_filled(tip, size * 0.035 + 0.6, light(rgb(255, 236, 190), 0.9 * k));
    }
    glow(painter, c, size * 0.45, rgb(220, 235, 255), 0.9 * k);
}

/// A pointed petal pointing at angle `a`.
fn petal(c: Pos2, a: f32, len: f32, width: f32) -> Vec<Pos2> {
    const N: usize = 9;
    let dir = vec2(a.cos(), a.sin());
    let side = vec2(-dir.y, dir.x);
    let squash = |v: egui::Vec2| vec2(v.x, v.y * 0.62);
    let mut left = Vec::with_capacity(N + 1);
    let mut right = Vec::with_capacity(N + 1);
    for s in 0..=N {
        let f = s as f32 / N as f32;
        let along = dir * (len * f);
        // (sin(π) comes out as a tiny negative number: clamp before the fractional power.)
        let half = width * 0.5 * (f * PI).sin().max(0.0).powf(0.75) * (1.0 - 0.35 * f);
        left.push(c + squash(along + side * half));
        right.push(c + squash(along - side * half));
    }
    right.reverse();
    left.extend(right);
    left
}

/// Little lights floating up.
pub fn motes(painter: &Painter, r: Rect, t: f32, color: Color32, n: u32) {
    for i in 0..n {
        let speed = 6.0 + 14.0 * hash(i + 5000);
        let span = r.height() + 40.0;
        let y = r.bottom() + 20.0 - ((hash(i + 5100) * span + t * speed) % span);
        let x = r.left() + hash(i + 5200) * r.width() + (t * 0.4 + i as f32).sin() * 14.0;
        let a = (0.35 + 0.5 * hash(i + 5300)) * (0.6 + 0.4 * (t * 2.0 + i as f32 * 1.7).sin());
        let s = 0.8 + 1.6 * hash(i + 5400);
        painter.circle_filled(pos2(x, y), s, light(color, a));
        glow(painter, pos2(x, y), s * 5.0, color, a * 0.5);
    }
}

// ---------------------------------------------------------------------------------------
// Day: overgrown ruins in the fog.

fn ruins(painter: &Painter, r: Rect, t: f32) {
    let (w, h) = (r.width(), r.height());
    gradient(painter, r, rgb(233, 228, 210), rgb(196, 204, 196));
    // The sun behind the haze.
    glow(
        painter,
        pos2(r.left() + w * 0.78, r.top() + h * 0.16),
        h * 0.5,
        rgb(255, 250, 230),
        0.5,
    );

    let far = rgb(128, 150, 156);
    let mid = rgb(92, 120, 128);
    let near = rgb(58, 86, 94);

    // Far towers.
    tower_water(
        painter,
        pos2(r.left() + w * 0.36, r.top() + h * 0.08),
        h * 0.5,
        alpha(far, 0.55),
    );
    tower_spire(
        painter,
        pos2(r.left() + w * 0.83, r.top() + h * 0.02),
        h * 0.62,
        alpha(far, 0.6),
    );
    painter.circle_filled(
        pos2(r.left() + w * 0.66, r.top() + h * 0.24),
        h * 0.06,
        alpha(far, 0.45),
    );
    painter.rect_filled(
        Rect::from_min_max(
            pos2(r.left() + w * 0.62, r.top() + h * 0.24),
            pos2(r.left() + w * 0.70, r.top() + h * 0.6),
        ),
        0.0,
        alpha(far, 0.45),
    );
    fog(painter, r, r.top() + h * 0.35, h * 0.3, 0.55);

    // Ferris wheels: a small one deep in the fog, a big one in front.
    ferris(
        painter,
        pos2(r.left() + w * 0.53, r.top() + h * 0.6),
        h * 0.13,
        t * 0.01,
        alpha(mid, 0.45),
    );
    trees(
        painter,
        r,
        r.top() + h * 0.78,
        h * 0.32,
        41,
        alpha(mid, 0.85),
        t,
    );
    fog(painter, r, r.top() + h * 0.55, h * 0.25, 0.4);
    ferris(
        painter,
        pos2(r.left() + w * 0.13, r.top() + h * 0.4),
        h * 0.36,
        -t * 0.006,
        alpha(near, 0.88),
    );
    trees(painter, r, r.top() + h * 0.92, h * 0.3, 77, near, t);

    // Foreground grass, swaying.
    let ground = r.top() + h * 0.9;
    painter.rect_filled(
        Rect::from_min_max(pos2(r.left(), ground), r.max),
        0.0,
        rgb(24, 34, 38),
    );
    for i in 0..220u32 {
        let x = r.left() + hash(i + 7000) * w;
        let gh = h * (0.02 + 0.06 * hash(i + 7100));
        let bend = (t * 1.1 + x * 0.01).sin() * gh * 0.25;
        painter.line_segment(
            [pos2(x, ground + 3.0), pos2(x + bend, ground - gh)],
            Stroke::new(1.6, rgb(24, 34, 38)),
        );
    }
    dust(painter, r, t);
}

/// Dust drifting in the light.
fn dust(painter: &Painter, r: Rect, t: f32) {
    let (w, h) = (r.width(), r.height());
    for i in 0..60u32 {
        let x = r.left() + ((hash(i + 8000) * w + t * (3.0 + 6.0 * hash(i + 8100))) % w);
        let y = r.top() + hash(i + 8200) * h * 0.85 + (t * 0.5 + i as f32).sin() * 10.0;
        let a = 0.25 + 0.35 * (t + i as f32).sin().abs();
        painter.circle_filled(pos2(x, y), 1.2, alpha(rgb(255, 252, 240), a));
    }
}

fn fog(painter: &Painter, r: Rect, top: f32, height: f32, k: f32) {
    let fogc = rgb(232, 230, 218);
    let mid = top + height * 0.5;
    gradient(
        painter,
        Rect::from_x_y_ranges(r.x_range(), top..=mid),
        alpha(fogc, 0.0),
        alpha(fogc, k),
    );
    gradient(
        painter,
        Rect::from_x_y_ranges(r.x_range(), mid..=(top + height)),
        alpha(fogc, k),
        alpha(fogc, 0.0),
    );
}

fn tower_water(painter: &Painter, top: Pos2, height: f32, c: Color32) {
    let tank = Rect::from_center_size(
        top + vec2(0.0, height * 0.12),
        vec2(height * 0.32, height * 0.18),
    );
    painter.rect_filled(tank, 4.0, c);
    let legs_top = tank.bottom();
    let stroke = Stroke::new(3.0, c);
    for k in [-1.0f32, -0.35, 0.35, 1.0] {
        let x0 = top.x + k * height * 0.12;
        let x1 = top.x + k * height * 0.2;
        painter.line_segment([pos2(x0, legs_top), pos2(x1, top.y + height)], stroke);
    }
    for f in [0.35f32, 0.6, 0.85] {
        let y = legs_top + (top.y + height - legs_top) * f;
        let half = height * (0.12 + 0.08 * f);
        painter.line_segment(
            [pos2(top.x - half, y), pos2(top.x + half, y)],
            Stroke::new(1.5, c),
        );
    }
}

fn tower_spire(painter: &Painter, top: Pos2, height: f32, c: Color32) {
    let half = height * 0.05;
    painter.rect_filled(
        Rect::from_min_max(
            pos2(top.x - half, top.y + height * 0.12),
            pos2(top.x + half, top.y + height),
        ),
        0.0,
        c,
    );
    painter.add(Shape::convex_polygon(
        vec![
            pos2(top.x, top.y),
            pos2(top.x + half * 1.3, top.y + height * 0.13),
            pos2(top.x - half * 1.3, top.y + height * 0.13),
        ],
        c,
        Stroke::NONE,
    ));
    for f in [0.22f32, 0.4] {
        let y = top.y + height * f;
        painter.rect_filled(
            Rect::from_center_size(pos2(top.x, y), vec2(half * 4.0, height * 0.025)),
            0.0,
            c,
        );
    }
}

fn ferris(painter: &Painter, c: Pos2, radius: f32, angle: f32, color: Color32) {
    let stroke = Stroke::new((radius * 0.02).max(1.5), color);
    painter.circle_stroke(c, radius, Stroke::new((radius * 0.035).max(2.0), color));
    painter.circle_stroke(c, radius * 0.93, stroke);
    painter.circle_filled(c, radius * 0.06, color);
    const SPOKES: usize = 18;
    for k in 0..SPOKES {
        let a = angle + k as f32 * TAU / SPOKES as f32;
        let d = vec2(a.cos(), a.sin());
        painter.line_segment([c, c + d * radius], stroke);
        if k % 2 == 0 {
            let cab = c + d * radius + vec2(0.0, radius * 0.06);
            painter.rect_filled(
                Rect::from_center_size(cab, vec2(radius * 0.08, radius * 0.07)),
                1.0,
                color,
            );
        }
    }
    // Legs.
    let foot = radius * 1.35;
    let leg = Stroke::new((radius * 0.03).max(2.0), color);
    painter.line_segment([c, c + vec2(-radius * 0.45, foot)], leg);
    painter.line_segment([c, c + vec2(radius * 0.45, foot)], leg);
}

#[allow(clippy::too_many_arguments)]
fn trees(painter: &Painter, r: Rect, base: f32, max_h: f32, seed: u32, color: Color32, t: f32) {
    let mut x = r.left() - 40.0;
    let mut i = 0u32;
    while x < r.right() + 40.0 {
        let k = seed * 997 + i;
        let th = max_h * (0.45 + 0.55 * hash(k));
        let crown = th * (0.35 + 0.2 * hash(k + 1));
        let sway = (t * 0.5 + k as f32).sin() * crown * 0.03;
        painter.line_segment(
            [pos2(x, base), pos2(x + sway, base - th * 0.6)],
            Stroke::new(crown * 0.08 + 1.0, color),
        );
        for b in 0..7u32 {
            let a = hash(k * 13 + b) * TAU;
            let d = hash(k * 13 + b + 7) * crown * 0.7;
            let cc = pos2(x + sway + a.cos() * d, base - th * 0.68 + a.sin() * d * 0.6);
            painter.circle_filled(cc, crown * (0.35 + 0.25 * hash(k * 13 + b + 3)), color);
        }
        x += crown * (0.9 + 0.8 * hash(k + 2));
        i += 1;
    }
}
