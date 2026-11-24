//! The look: warm paper panels with dark ink, thin rules and offset shadows — an android's
//! field terminal — over the background art.

use std::sync::Arc;

use eframe::egui::{
    self, pos2, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId,
    Painter, Pos2, Rect, Shadow, Stroke, TextStyle,
};

pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> Color32 {
    // Color32 stores premultiplied alpha.
    let a8 = (a * 255.0) as u16;
    Color32::from_rgba_premultiplied(
        ((r as u16 * a8) / 255) as u8,
        ((g as u16 * a8) / 255) as u8,
        ((b as u16 * a8) / 255) as u8,
        a8 as u8,
    )
}

/// Panel paper.
pub const PAPER: Color32 = rgba(216, 210, 186, 0.92);
/// A lighter sheet (hover, inputs).
pub const PAPER_HI: Color32 = rgba(232, 227, 206, 0.96);
/// Ink: text and lines on paper.
pub const INK: Color32 = Color32::from_rgb(0x48, 0x45, 0x3c);
pub const INK_SOFT: Color32 = Color32::from_rgb(0x72, 0x6d, 0x5e);
pub const INK_FAINT: Color32 = rgba(72, 69, 60, 0.28);
/// Selected rows, header strips, filled controls.
pub const DARK: Color32 = rgba(62, 59, 51, 0.95);
/// Text on dark fills.
pub const ON_DARK: Color32 = Color32::from_rgb(0xdd, 0xd7, 0xbe);
/// Shadow behind panels.
pub const SHADOW: Color32 = rgba(10, 10, 8, 0.32);

pub const OK: Color32 = Color32::from_rgb(0x5d, 0x8a, 0x6c);
pub const WAIT: Color32 = Color32::from_rgb(0xb3, 0x89, 0x3a);
pub const BAD: Color32 = Color32::from_rgb(0xa9, 0x4b, 0x3b);
/// The drone's cold light.
pub const GLOW: Color32 = Color32::from_rgb(0xa8, 0xd8, 0xff);

/// egui's own style, for the few built-in pieces still used (tooltips, scroll bars).
pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;
        *v = egui::Visuals::light();
        v.override_text_color = Some(INK);
        v.panel_fill = Color32::TRANSPARENT;
        v.window_fill = PAPER_HI;
        v.window_stroke = Stroke::new(1.0, INK_FAINT);
        v.window_corner_radius = CornerRadius::ZERO;
        v.menu_corner_radius = CornerRadius::ZERO;
        v.popup_shadow = Shadow {
            offset: [4, 4],
            blur: 0,
            spread: 0,
            color: SHADOW,
        };
        v.window_shadow = v.popup_shadow;
        style.spacing.scroll.bar_width = 4.0;
        style.spacing.scroll.floating = true;
        style.interaction.selectable_labels = false;
    });
}

/// Wide geometric face for headings and labels.
pub const HUD: &str = "hud";
pub const ICONS: &str = "icons";

fn read(name: &str) -> Option<Vec<u8>> {
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    std::fs::read(std::path::Path::new(&dir).join("Fonts").join(name)).ok()
}

/// Segoe UI for text, Bahnschrift for headings and labels, and the Segoe Fluent icons.
pub fn fonts(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let mut add = |key: &str, files: &[&str]| -> bool {
        for a in files {
            if let Some(b) = read(a) {
                defs.font_data
                    .insert(key.into(), Arc::new(FontData::from_owned(b)));
                return true;
            }
        }
        false
    };
    let has_segoe = add("segoe", &["segoeui.ttf"]);
    let has_hud = add(
        "bahnschrift",
        &["bahnschrift.ttf", "segoeuisl.ttf", "segoeui.ttf"],
    );
    let has_icons = add("fluent", &["SegoeIcons.ttf", "segmdl2.ttf"]);

    let mut prop = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    if has_segoe {
        prop.insert(0, "segoe".into());
    }
    defs.families.insert(FontFamily::Proportional, prop.clone());
    let mut hud = prop.clone();
    if has_hud {
        hud.insert(0, "bahnschrift".into());
    }
    defs.families.insert(FontFamily::Name(HUD.into()), hud);
    let mut icons = prop;
    if has_icons {
        icons.insert(0, "fluent".into());
    }
    defs.families.insert(FontFamily::Name(ICONS.into()), icons);
    ctx.set_fonts(defs);
    ctx.all_styles_mut(|s| {
        s.text_styles
            .insert(TextStyle::Body, FontId::proportional(13.0));
        s.text_styles
            .insert(TextStyle::Small, FontId::proportional(11.0));
    });
}

pub fn hud(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(HUD.into()))
}

pub fn body(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}

/// Text with extra space between the letters (egui has no letter spacing). Returns the rect.
pub fn spaced(
    painter: &Painter,
    pos: Pos2,
    align: Align2,
    text: &str,
    font: FontId,
    color: Color32,
    spacing: f32,
) -> Rect {
    let glyphs: Vec<_> = text
        .chars()
        .map(|c| painter.layout_no_wrap(c.to_string(), font.clone(), color))
        .collect();
    let count = glyphs.len().max(1) as f32;
    let width: f32 = glyphs.iter().map(|g| g.size().x).sum::<f32>() + spacing * (count - 1.0);
    let height = glyphs.iter().map(|g| g.size().y).fold(0.0, f32::max);
    let rect = align.anchor_size(pos, egui::vec2(width, height));
    let mut x = rect.left();
    for g in glyphs {
        let w = g.size().x;
        painter.galley(pos2(x, rect.top()), g, color);
        x += w + spacing;
    }
    rect
}

/// Icons from Segoe Fluent Icons / Segoe MDL2 Assets.
pub mod ic {
    pub const MINIMIZE: char = '\u{E921}';
    pub const CLOSE: char = '\u{E8BB}';
    pub const SETTINGS: char = '\u{E713}';
    pub const INFO: char = '\u{E946}';
}
