//! Colors and fonts: dark glass with warm beige lines at night, warm paper panels with dark
//! ink by day — the look of an android's field interface.

use std::sync::Arc;

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText,
    Shadow, Stroke, TextStyle,
};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub text: Color32,
    pub title: Color32,
    pub muted: Color32,
    pub dim: Color32,
    pub panel: Color32,
    pub panel_strong: Color32,
    pub line: Color32,
    pub line_strong: Color32,
    /// Lit state (cheat on): the cold glow of the lilies.
    pub glow: Color32,
    pub warn: Color32,
    pub danger: Color32,
    pub field: Color32,
    pub shadow: Color32,
}

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

/// Night: dark glass, beige ink, cold blue glow.
pub const NIGHT: Palette = Palette {
    dark: true,
    text: Color32::from_rgb(0xdc, 0xd6, 0xbf),
    title: Color32::from_rgb(0xf1, 0xec, 0xd8),
    muted: Color32::from_rgb(0xa9, 0xa5, 0x93),
    dim: Color32::from_rgb(0x7c, 0x7a, 0x6e),
    panel: rgba(10, 12, 22, 0.66),
    panel_strong: rgba(14, 16, 28, 0.9),
    line: rgba(220, 214, 191, 0.16),
    line_strong: rgba(220, 214, 191, 0.42),
    glow: Color32::from_rgb(0xa8, 0xd8, 0xff),
    warn: Color32::from_rgb(0xe6, 0xbd, 0x6e),
    danger: Color32::from_rgb(0xe0, 0x74, 0x62),
    field: rgba(4, 6, 12, 0.8),
    shadow: rgba(0, 0, 0, 0.45),
};

/// Day: warm paper panels and dark ink.
pub const DAY: Palette = Palette {
    dark: false,
    text: Color32::from_rgb(0x4b, 0x48, 0x3e),
    title: Color32::from_rgb(0x36, 0x34, 0x2c),
    muted: Color32::from_rgb(0x6e, 0x6a, 0x5c),
    dim: Color32::from_rgb(0x8d, 0x88, 0x77),
    panel: rgba(222, 217, 196, 0.8),
    panel_strong: rgba(228, 223, 203, 0.95),
    line: rgba(75, 72, 62, 0.2),
    line_strong: rgba(75, 72, 62, 0.5),
    glow: Color32::from_rgb(0x2f, 0x6f, 0x8a),
    warn: Color32::from_rgb(0xa4, 0x6c, 0x16),
    danger: Color32::from_rgb(0xa8, 0x3e, 0x2c),
    field: rgba(245, 241, 226, 0.95),
    shadow: rgba(40, 38, 30, 0.22),
};

impl Palette {
    /// Adjusts the egui style (the real widgets: sliders, checkboxes, number fields).
    pub fn apply(&self, ctx: &egui::Context) {
        let p = *self;
        ctx.all_styles_mut(|style| {
            let v = &mut style.visuals;
            *v = if p.dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            v.override_text_color = Some(p.text);
            v.panel_fill = Color32::TRANSPARENT;
            v.window_fill = p.panel_strong;
            v.window_stroke = Stroke::new(1.0, p.line_strong);
            v.window_corner_radius = CornerRadius::same(4);
            v.menu_corner_radius = CornerRadius::same(4);
            v.extreme_bg_color = p.field;
            v.faint_bg_color = p.line;
            v.selection.bg_fill = p.glow.gamma_multiply(0.45);
            v.selection.stroke = Stroke::new(1.0, p.title);
            v.popup_shadow = Shadow {
                offset: [0, 6],
                blur: 18,
                spread: 0,
                color: p.shadow,
            };
            v.window_shadow = v.popup_shadow;
            v.slider_trailing_fill = true;

            let r = CornerRadius::same(2);
            let w = &mut v.widgets;
            w.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
            w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
            w.noninteractive.corner_radius = r;
            w.inactive.bg_fill = p.line;
            w.inactive.weak_bg_fill = p.line.gamma_multiply(0.5);
            w.inactive.bg_stroke = Stroke::new(1.0, p.line);
            w.inactive.fg_stroke = Stroke::new(1.5, p.text);
            w.inactive.corner_radius = r;
            w.hovered.bg_fill = p.line_strong;
            w.hovered.weak_bg_fill = p.line;
            w.hovered.bg_stroke = Stroke::new(1.0, p.line_strong);
            w.hovered.fg_stroke = Stroke::new(1.5, p.title);
            w.hovered.corner_radius = r;
            w.active.bg_fill = p.glow.gamma_multiply(0.5);
            w.active.weak_bg_fill = p.glow.gamma_multiply(0.3);
            w.active.bg_stroke = Stroke::new(1.0, p.glow);
            w.active.fg_stroke = Stroke::new(2.0, p.title);
            w.active.corner_radius = r;
            w.open = w.active;

            style.spacing.item_spacing = egui::vec2(8.0, 6.0);
            style.spacing.button_padding = egui::vec2(10.0, 4.0);
            style.spacing.slider_rail_height = 4.0;
            style.spacing.slider_width = 180.0;
            style.spacing.scroll.bar_width = 5.0;
            style.spacing.scroll.floating = true;
            style.spacing.menu_margin = Margin::same(6);
            style.interaction.selectable_labels = false;
        });
    }
}

/// Wide geometric face for headings and HUD labels.
pub const HUD: &str = "hud";
pub const ICONS: &str = "icons";

fn read(name: &str) -> Option<Vec<u8>> {
    let dir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
    std::fs::read(std::path::Path::new(&dir).join("Fonts").join(name)).ok()
}

/// Segoe UI for text, Bahnschrift for headings and labels, and the Segoe Fluent icons.
/// Japanese comes from the system fonts.
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
    let has_jp = add("jp", &["YuGothR.ttc", "msgothic.ttc", "meiryo.ttc"]);

    let base = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut prop = base;
    if has_segoe {
        prop.insert(0, "segoe".into());
    }
    if has_jp {
        prop.push("jp".into());
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
            .insert(TextStyle::Body, FontId::proportional(13.5));
        s.text_styles
            .insert(TextStyle::Button, FontId::proportional(13.5));
        s.text_styles
            .insert(TextStyle::Small, FontId::proportional(11.5));
        s.text_styles.insert(
            TextStyle::Heading,
            FontId::new(22.0, FontFamily::Name(HUD.into())),
        );
    });
}

pub fn hud(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(HUD.into()))
}

pub fn hud_text(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text)
        .family(FontFamily::Name(HUD.into()))
        .size(size)
}

pub fn icon_font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(ICONS.into()))
}

/// Icons from Segoe Fluent Icons / Segoe MDL2 Assets.
pub mod ic {
    pub const MINIMIZE: char = '\u{E921}';
    pub const CLOSE: char = '\u{E8BB}';
    pub const SETTINGS: char = '\u{E713}';
    pub const MAGNET: char = '\u{E896}';
    pub const RADAR: char = '\u{E81D}';
    pub const REFRESH: char = '\u{E72C}';
    pub const STACK: char = '\u{E8F1}';
    pub const FOLDER: char = '\u{E8B7}';
}
