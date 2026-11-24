//! The window: title bar, the cheat cards, the drone with its speech bubble, the event log
//! and the settings.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, pos2, vec2, Align, Align2, Color32, CornerRadius, CursorIcon, FontId, Id, Layout, Margin,
    Painter, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder,
    ViewportCommand,
};

use crate::game::cheats::{CheatId, Values};
use crate::game::respawn;
use crate::scene::{self, Scene};
use crate::settings::{Scenery, Settings};
use crate::sprites::Drone;
use crate::theme::{self, ic, Palette};
use crate::trainer::{self, LogEntry, LogKind, Status, Trainer, Wanted};

pub struct App {
    settings: Settings,
    dirty_since: Option<f64>,
    trainer: Trainer,
    scene: Scene,
    drone: Drone,
    palette_dark: Option<bool>,
    clock: f64,
    last_time: f64,
    /// The newest log entry already seen (to pulse the drone and update the bubble).
    seen: Option<Instant>,
    bubble: Option<(String, f64)>,
    show_settings: bool,
}

/// A copy of the worker's state for one frame.
struct View {
    status: Status,
    active: [bool; 4],
    supported: [bool; 4],
    log: Vec<LogEntry>,
    kills: u32,
    respawns: u32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let settings = Settings::load();
        crate::i18n::set_language(settings.language);
        theme::fonts(&cc.egui_ctx);
        let trainer = Trainer::start(wanted(&settings));
        Self {
            settings,
            dirty_since: None,
            trainer,
            scene: Scene::default(),
            drone: Drone::default(),
            palette_dark: None,
            clock: 0.0,
            last_time: 0.0,
            seen: None,
            bubble: None,
            show_settings: false,
        }
    }

    fn palette(&self) -> Palette {
        match self.settings.scenery {
            Scenery::Ruins => theme::DAY,
            _ => theme::NIGHT,
        }
    }

    fn changed(&mut self, now: f64) {
        self.dirty_since.get_or_insert(now);
    }

    /// Syncs with the worker: our wishes go in, its state comes out.
    fn exchange(&mut self, now: f64) -> View {
        let mut hotkey_change = None;
        let view = {
            let mut s = self.trainer.shared.lock().unwrap();
            if std::mem::take(&mut s.toggled_by_hotkey) {
                hotkey_change = Some(s.wanted.enabled);
            }
            if let Some(enabled) = hotkey_change {
                for (k, id) in CheatId::ALL.iter().enumerate() {
                    self.settings.set_enabled(*id, enabled[k]);
                }
            }
            s.wanted = wanted(&self.settings);
            View {
                status: s.status.clone(),
                active: s.active,
                supported: s.supported,
                log: s.log.iter().take(40).cloned().collect(),
                kills: s.kills,
                respawns: s.respawns,
            }
        };
        if hotkey_change.is_some() {
            self.changed(now);
        }
        view
    }
}

fn wanted(s: &Settings) -> Wanted {
    Wanted {
        enabled: CheatId::ALL.map(|id| s.enabled(id)),
        values: Values {
            range: s.range,
            item_cap: s.item_cap,
        },
        respawn: respawn::Options {
            auto: s.respawn_auto,
            delay: s.respawn_delay,
            at_death: s.respawn_at_death,
        },
        hotkeys: s.hotkeys,
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        draw(self, ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.trainer.shutdown();
        self.settings.save();
    }
}

fn draw(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let screen = ui.max_rect();
    let (now, focused, pointer) = ui.input(|i| (i.time, i.focused, i.pointer.hover_pos()));
    let dt = (now - app.last_time).clamp(0.0, 0.1);
    app.last_time = now;
    if app.settings.animated {
        app.clock += dt;
    }
    let t = app.clock as f32;

    let p = app.palette();
    if app.palette_dark != Some(p.dark) {
        p.apply(&ctx);
        app.palette_dark = Some(p.dark);
    }
    let view = app.exchange(now);

    // Something new in the log: the drone pulses and says it.
    if let Some(first) = view.log.first() {
        if app.seen != Some(first.at) {
            if app.seen.is_some() {
                app.drone.pulse(t);
            }
            app.seen = Some(first.at);
            app.bubble = Some((describe(&first.kind).0, now));
        }
    }

    let painter = ui.painter().clone();
    app.scene.paint(
        &ctx,
        &painter,
        screen,
        app.settings.scenery,
        app.settings.custom_background.as_deref(),
        t,
    );

    // A band behind the title bar so it reads over any picture.
    let solid = Rect::from_min_size(screen.min, vec2(screen.width(), 52.0));
    painter.rect_filled(solid, 0.0, p.panel_strong.gamma_multiply(0.82));
    let fade = Rect::from_min_size(solid.left_bottom(), vec2(screen.width(), 50.0));
    scene::gradient(
        &painter,
        fade,
        p.panel_strong.gamma_multiply(0.82),
        p.panel_strong.gamma_multiply(0.0),
    );

    // The title bar drags the window.
    let bar = Rect::from_min_size(screen.min, vec2(screen.width(), 56.0));
    let drag = ui.interact(bar, Id::unique("title-drag"), Sense::click_and_drag());
    if drag.drag_started() {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    title_bar(app, ui, &painter, bar, &view, &p, t);

    // Content.
    let content = Rect::from_min_max(
        pos2(screen.left() + 22.0, bar.bottom() + 6.0),
        pos2(screen.right() - 22.0, screen.bottom() - 34.0),
    );
    let left_w = (content.width() * 0.58).clamp(480.0, 660.0);
    let left = Rect::from_min_size(content.min, vec2(left_w, content.height()));
    let right = Rect::from_min_max(pos2(left.right() + 20.0, content.top()), content.max);

    cheat_column(app, ui, left, &view, &p, now);
    side_column(app, ui, &painter, right, &view, &p, t, now, pointer);
    footer(&painter, screen, &p);

    if app.show_settings {
        settings_window(app, &ctx, &p, now);
    }

    // Save a moment after the last change.
    if app.dirty_since.is_some_and(|d| now - d > 1.0) {
        app.settings.save();
        app.dirty_since = None;
    }

    let ms = match (app.settings.animated, focused) {
        (true, true) => 16,
        (true, false) => 50,
        (false, _) => 250,
    };
    ctx.request_repaint_after(Duration::from_millis(ms));
}

// ---------------------------------------------------------------------------------------
// Title bar.

fn title_bar(
    app: &mut App,
    ui: &mut Ui,
    painter: &Painter,
    bar: Rect,
    view: &View,
    p: &Palette,
    t: f32,
) {
    let ctx = ui.ctx().clone();
    // Logo: a lit diamond.
    let logo = pos2(bar.left() + 34.0, bar.center().y);
    scene::glow(painter, logo, 22.0, p.glow, 0.5 + 0.2 * (t * 1.5).sin());
    diamond(painter, logo, 9.0, p.title, Stroke::new(1.0, p.line_strong));
    diamond(painter, logo, 4.0, p.glow, Stroke::NONE);
    let title = painter.text(
        pos2(logo.x + 22.0, bar.center().y - 7.0),
        Align2::LEFT_CENTER,
        "YoRHa  TRAINER",
        theme::hud(20.0),
        p.title,
    );
    painter.text(
        pos2(title.left(), title.bottom() + 8.0),
        Align2::LEFT_CENTER,
        tr!("para NieR:Automata · Steam", "for NieR:Automata · Steam"),
        FontId::proportional(11.5),
        p.muted,
    );

    // Window buttons.
    let close = Rect::from_min_size(pos2(bar.right() - 48.0, bar.top()), vec2(48.0, 40.0));
    if window_button(ui, painter, close, "close", ic::CLOSE, true, p) {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
    let min = close.translate(vec2(-46.0, 0.0));
    if window_button(ui, painter, min, "min", ic::MINIMIZE, false, p) {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }
    let gear = min.translate(vec2(-46.0, 0.0));
    if window_button(ui, painter, gear, "settings", ic::SETTINGS, false, p) {
        app.show_settings = !app.show_settings;
    }

    // Connection status.
    let (text, color) = match &view.status {
        Status::Waiting => (
            tr!("Aguardando o jogo…", "Waiting for the game…").to_string(),
            p.warn,
        ),
        Status::Attached { pid } => (
            trf!("Conectado · PID {pid}", "Connected · PID {pid}"),
            p.glow,
        ),
        Status::NoAccess { .. } => (
            tr!(
                "Sem acesso ao jogo (abra como administrador)",
                "No access to the game (run as administrator)"
            )
            .to_string(),
            p.danger,
        ),
    };
    let font = theme::hud(13.0);
    let galley = painter.layout_no_wrap(text, font, p.title);
    let chip = Rect::from_min_size(
        pos2(
            gear.left() - 14.0 - galley.size().x - 34.0,
            bar.center().y - 13.0,
        ),
        vec2(galley.size().x + 34.0, 26.0),
    );
    painter.rect(
        chip,
        2.0,
        p.panel,
        Stroke::new(1.0, p.line_strong),
        StrokeKind::Inside,
    );
    let dot = pos2(chip.left() + 14.0, chip.center().y);
    let beat = 0.6 + 0.4 * (t * 3.0).sin();
    scene::glow(painter, dot, 12.0, color, beat);
    painter.circle_filled(dot, 3.5, color);
    painter.galley(
        pos2(chip.left() + 26.0, chip.center().y - galley.size().y / 2.0),
        galley,
        p.title,
    );
}

fn diamond(painter: &Painter, c: Pos2, r: f32, fill: Color32, stroke: Stroke) {
    painter.add(Shape::convex_polygon(
        vec![
            c + vec2(0.0, -r),
            c + vec2(r, 0.0),
            c + vec2(0.0, r),
            c + vec2(-r, 0.0),
        ],
        fill,
        stroke,
    ));
}

fn window_button(
    ui: &mut Ui,
    painter: &Painter,
    r: Rect,
    id: &str,
    icon: char,
    danger: bool,
    p: &Palette,
) -> bool {
    let resp = ui
        .interact(r, Id::unique(("window-button", id)), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    if resp.hovered() {
        let fill = if danger {
            p.danger.gamma_multiply(0.85)
        } else {
            p.line
        };
        painter.rect_filled(r, 0.0, fill);
    }
    let color = if resp.hovered() && danger {
        Color32::WHITE
    } else {
        p.title
    };
    painter.text(
        r.center(),
        Align2::CENTER_CENTER,
        icon,
        theme::icon_font(12.0),
        color,
    );
    resp.clicked()
}

// ---------------------------------------------------------------------------------------
// Cheat cards.

fn cheat_column(app: &mut App, ui: &mut Ui, rect: Rect, view: &View, p: &Palette, now: f64) {
    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
        section_header(ui, tr!("MÓDULOS", "MODULES"), p);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (k, id) in CheatId::ALL.iter().enumerate() {
                    cheat_card(app, ui, *id, k, view, p, now);
                    ui.add_space(10.0);
                }
            });
    });
}

fn section_header(ui: &mut Ui, text: &str, p: &Palette) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
    let painter = ui.painter();
    let label = painter.text(
        pos2(r.left() + 14.0, r.center().y),
        Align2::LEFT_CENTER,
        text,
        theme::hud(14.0),
        p.title,
    );
    painter.rect_filled(
        Rect::from_center_size(pos2(r.left() + 4.0, r.center().y), vec2(6.0, 6.0)),
        0.0,
        p.title,
    );
    painter.line_segment(
        [
            pos2(label.right() + 10.0, r.center().y),
            pos2(r.right(), r.center().y),
        ],
        Stroke::new(1.0, p.line_strong),
    );
}

struct CardText {
    title: &'static str,
    description: &'static str,
    icon: char,
    key: &'static str,
}

fn card_text(id: CheatId) -> CardText {
    match id {
        CheatId::AutoCollect => CardText {
            title: tr!("Coleta automática sem o chip", "Auto-Collect without the chip"),
            description: tr!(
                "Os itens no chão vêm até você como se o chip Auto-Collect Items estivesse equipado, sem ocupar espaço de chips.",
                "Dropped items fly to you as if the Auto-Collect Items chip were equipped, without using chip space."
            ),
            icon: ic::MAGNET,
            key: "F1",
        },
        CheatId::Range => CardText {
            title: tr!("Alcance da coleta", "Collection range"),
            description: tr!(
                "Aumenta a distância da coleta automática (o jogo usa 4,5). Também pega itens longe cujo modelo o jogo já descarregou.",
                "Increases the auto-collect distance (the game uses 4.5). Also picks up far items whose model the game already unloaded."
            ),
            icon: ic::RADAR,
            key: "F2",
        },
        CheatId::Respawn => CardText {
            title: tr!("Respawn de inimigos", "Enemy respawn"),
            description: tr!(
                "Quando você mata um inimigo, ele volta depois de alguns segundos. Ótimo para farmar materiais e experiência.",
                "When you kill an enemy, it comes back after a few seconds. Great for farming materials and experience."
            ),
            icon: ic::REFRESH,
            key: "F3",
        },
        CheatId::ItemCap => CardText {
            title: tr!("Limite de itens", "Item stack limit"),
            description: tr!(
                "Troca o limite de 99 por item pelo valor que você escolher. Itens com limite próprio (itens-chave) continuam iguais.",
                "Replaces the 99-per-item limit with the value you choose. Items with their own limit (key items) stay the same."
            ),
            icon: ic::STACK,
            key: "F4",
        },
    }
}

#[allow(clippy::too_many_arguments)]
fn cheat_card(
    app: &mut App,
    ui: &mut Ui,
    id: CheatId,
    k: usize,
    view: &View,
    p: &Palette,
    now: f64,
) {
    let text = card_text(id);
    let i = trainer::index(id);
    let mut on = app.settings.enabled(id);
    let active = view.active[i];
    let supported = view.supported[i];
    let attached = matches!(view.status, Status::Attached { .. });
    let lit = ui
        .ctx()
        .animate_bool_with_time(Id::unique(("card-lit", k)), active, 0.25);

    let frame = egui::Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(
            1.0,
            if active {
                p.glow.gamma_multiply(0.55)
            } else {
                p.line
            },
        ))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin {
            left: 20,
            right: 16,
            top: 12,
            bottom: 12,
        });
    let resp = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(theme::hud_text(format!("{:02}", k + 1), 26.0).color(p.dim));
            ui.add_space(4.0);
            ui.label(
                RichText::new(text.icon.to_string())
                    .font(theme::icon_font(18.0))
                    .color(if active { p.glow } else { p.title }),
            );
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(theme::hud_text(text.title, 17.0).color(p.title));
                    if app.settings.hotkeys {
                        key_badge(ui, text.key, p);
                    }
                });
                let (state, color) = if !supported {
                    (
                        tr!(
                            "Não encontrado nesta versão do jogo",
                            "Not found in this game version"
                        ),
                        p.danger,
                    )
                } else if active {
                    (tr!("Ativo no jogo", "Active in the game"), p.glow)
                } else if on && !attached {
                    (
                        tr!(
                            "Será aplicado quando o jogo abrir",
                            "Will apply when the game opens"
                        ),
                        p.warn,
                    )
                } else {
                    (tr!("Desligado", "Off"), p.dim)
                };
                ui.label(RichText::new(state).size(11.5).color(color));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if toggle(ui, &mut on, p, supported, ("toggle", k)).changed() {
                    app.settings.set_enabled(id, on);
                    app.changed(now);
                }
            });
        });
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(text.description).color(p.muted)).wrap());
        ui.add_space(4.0);
        card_controls(app, ui, id, p, now);
    });

    // The lit bar on the left edge.
    let r = resp.response.rect;
    let bar = Rect::from_min_size(r.min + vec2(0.0, 6.0), vec2(4.0, r.height() - 12.0));
    let painter = ui.painter();
    painter.rect_filled(bar, 0.0, p.line_strong.lerp_to_gamma(p.glow, lit));
    if lit > 0.01 {
        scene::glow(painter, bar.center(), 26.0, p.glow, 0.5 * lit);
    }
    // Corner ticks.
    let tick = Stroke::new(1.0, p.line_strong);
    painter.line_segment([r.right_top() + vec2(-14.0, 0.0), r.right_top()], tick);
    painter.line_segment([r.right_top(), r.right_top() + vec2(0.0, 14.0)], tick);
}

fn card_controls(app: &mut App, ui: &mut Ui, id: CheatId, p: &Palette, now: f64) {
    let s = &mut app.settings;
    let mut changed = false;
    match id {
        CheatId::AutoCollect => {}
        CheatId::Range => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("Alcance", "Reach")).color(p.text));
                changed |= ui
                    .add(
                        egui::Slider::new(&mut s.range, 4.0..=500.0)
                            .logarithmic(true)
                            .max_decimals(0),
                    )
                    .changed();
                ui.label(
                    RichText::new(tr!("(jogo: 4)", "(game: 4)"))
                        .size(11.5)
                        .color(p.dim),
                );
            });
        }
        CheatId::Respawn => {
            ui.horizontal_wrapped(|ui| {
                changed |= ui
                    .checkbox(&mut s.respawn_auto, tr!("Automático", "Automatic"))
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut s.respawn_at_death,
                        tr!("Voltar onde morreu", "Come back where it died"),
                    )
                    .changed();
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("Espera", "Delay")).color(p.text));
                changed |= ui
                    .add(
                        egui::Slider::new(&mut s.respawn_delay, 0.0..=30.0)
                            .suffix(" s")
                            .max_decimals(1),
                    )
                    .changed();
                let label = if s.hotkeys {
                    tr!("Repetir o último (F6)", "Repeat the last (F6)")
                } else {
                    tr!("Repetir o último", "Repeat the last")
                };
                if ui.button(label).clicked() {
                    app.trainer.respawn_last();
                }
            });
        }
        CheatId::ItemCap => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr!("Limite", "Limit")).color(p.text));
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut s.item_cap)
                            .range(99..=9_999_999)
                            .speed(25.0),
                    )
                    .changed();
                for v in [999u32, 9999, 99999] {
                    if ui
                        .selectable_label(s.item_cap == v, v.to_string())
                        .clicked()
                    {
                        s.item_cap = v;
                        changed = true;
                    }
                }
            });
        }
    }
    if changed {
        app.changed(now);
    }
}

fn key_badge(ui: &mut Ui, key: &str, p: &Palette) {
    let galley = ui
        .painter()
        .layout_no_wrap(key.to_string(), theme::hud(11.0), p.muted);
    let (r, _) = ui.allocate_exact_size(galley.size() + vec2(10.0, 4.0), Sense::hover());
    ui.painter()
        .rect_stroke(r, 2.0, Stroke::new(1.0, p.line_strong), StrokeKind::Inside);
    ui.painter().galley(r.min + vec2(5.0, 2.0), galley, p.muted);
}

/// A rectangular ON/OFF switch with a sliding block.
fn toggle(
    ui: &mut Ui,
    on: &mut bool,
    p: &Palette,
    enabled: bool,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> egui::Response {
    let size = vec2(64.0, 26.0);
    let (r, mut resp) = ui.allocate_exact_size(
        size,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    if enabled && resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let x = ui.ctx().animate_bool_with_time(Id::unique(id), *on, 0.16);
    let painter = ui.painter();
    let hover = resp.hovered() && enabled;
    let edge = if hover {
        p.line_strong.lerp_to_gamma(p.title, 0.4)
    } else {
        p.line_strong
    };
    painter.rect(r, 2.0, p.field, Stroke::new(1.0, edge), StrokeKind::Inside);
    let fill = Rect::from_min_size(r.min, vec2(r.width() * x, r.height()));
    painter.rect_filled(fill.shrink(2.0), 1.0, p.glow.gamma_multiply(0.35 * x));
    let knob_w = 26.0;
    let knob = Rect::from_min_size(
        r.min + vec2(2.0 + (r.width() - knob_w - 4.0) * x, 2.0),
        vec2(knob_w, r.height() - 4.0),
    );
    let knob_color = if enabled {
        p.dim.lerp_to_gamma(p.glow, x)
    } else {
        p.line_strong
    };
    painter.rect_filled(knob, 1.0, knob_color);
    if x > 0.5 {
        scene::glow(painter, knob.center(), 20.0, p.glow, (x - 0.5) * 1.2);
    }
    let label = if *on { "ON" } else { "OFF" };
    let label_x = if *on {
        r.left() + 17.0
    } else {
        r.right() - 18.0
    };
    painter.text(
        pos2(label_x, r.center().y),
        Align2::CENTER_CENTER,
        label,
        theme::hud(10.5),
        p.muted,
    );
    resp.on_hover_cursor(if enabled {
        CursorIcon::PointingHand
    } else {
        CursorIcon::NotAllowed
    })
}

// ---------------------------------------------------------------------------------------
// The drone, the bubble, the log.

#[allow(clippy::too_many_arguments)]
fn side_column(
    app: &mut App,
    ui: &mut Ui,
    painter: &Painter,
    rect: Rect,
    view: &View,
    p: &Palette,
    t: f32,
    now: f64,
    pointer: Option<Pos2>,
) {
    // The top of the column stays clear for the picture; the log sits at the bottom and the
    // drone floats just above it.
    let log_h = (rect.height() * 0.42).clamp(190.0, 300.0);
    let stage = Rect::from_min_max(
        pos2(rect.left(), rect.bottom() - log_h - 190.0),
        pos2(rect.right(), rect.bottom() - log_h - 8.0),
    );
    let eye = match view.status {
        Status::Attached { .. } => p.glow,
        Status::Waiting => p.warn,
        Status::NoAccess { .. } => p.danger,
    };
    let eye = if p.dark {
        eye
    } else {
        eye.lerp_to_gamma(Color32::from_rgb(120, 200, 255), 0.5)
    };
    let drone_at = pos2(stage.left() + 70.0, stage.top() + 105.0);
    app.drone.paint(painter, drone_at, 1.1, t, pointer, eye);

    // Speech bubble with the latest news (fades after a few seconds).
    let bubble_text = match &app.bubble {
        Some((text, at)) if now - at < 6.0 => Some((
            text.clone(),
            (1.0 - ((now - at - 5.0).max(0.0) as f32)).clamp(0.0, 1.0),
        )),
        _ => None,
    };
    let (text, a) = bubble_text.unwrap_or_else(|| (idle_line(&view.status), 1.0));
    bubble(
        painter,
        pos2(drone_at.x + 66.0, drone_at.y - 70.0),
        stage.right() - drone_at.x - 70.0,
        &text,
        p,
        a,
    );

    // Log and counters.
    let log_rect = Rect::from_min_max(pos2(rect.left(), stage.bottom() + 8.0), rect.max);
    ui.scope_builder(UiBuilder::new().max_rect(log_rect), |ui| {
        egui::Frame::new()
            .fill(p.panel)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(3))
            .inner_margin(Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(ui.available_height());
                section_header(ui, tr!("REGISTRO", "LOG"), p);
                ui.horizontal(|ui| {
                    stat(
                        ui,
                        tr!("Inimigos derrotados", "Enemies defeated"),
                        view.kills,
                        p,
                    );
                    ui.add_space(14.0);
                    stat(ui, tr!("Respawns", "Respawns"), view.respawns, p);
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if view.log.is_empty() {
                            ui.label(
                                RichText::new(tr!("Nada ainda.", "Nothing yet.")).color(p.dim),
                            );
                        }
                        for e in &view.log {
                            let (text, kind) = describe(&e.kind);
                            let color = match kind {
                                Tone::Good => p.glow,
                                Tone::Bad => p.danger,
                                Tone::Plain => p.text,
                            };
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(ago(e.at)).monospace().size(11.0).color(p.dim),
                                );
                                ui.add(egui::Label::new(RichText::new(text).color(color)).wrap());
                            });
                        }
                    });
            });
    });
}

fn stat(ui: &mut Ui, label: &str, value: u32, p: &Palette) {
    ui.label(theme::hud_text(value.to_string(), 22.0).color(p.title));
    ui.label(RichText::new(label).size(11.5).color(p.muted));
}

fn idle_line(status: &Status) -> String {
    match status {
        Status::Waiting => tr!(
            "Abra o NieR:Automata. Eu me conecto sozinho.",
            "Open NieR:Automata. I'll connect on my own."
        )
        .into(),
        Status::Attached { .. } => tr!(
            "Conectado. Ligue os módulos à esquerda.",
            "Connected. Turn on the modules on the left."
        )
        .into(),
        Status::NoAccess { .. } => tr!(
            "O Windows não me deixa mexer no jogo. Abra o trainer como administrador.",
            "Windows won't let me touch the game. Run the trainer as administrator."
        )
        .into(),
    }
}

fn bubble(painter: &Painter, at: Pos2, max_w: f32, text: &str, p: &Palette, a: f32) {
    if a <= 0.0 {
        return;
    }
    let max_w = max_w.clamp(160.0, 360.0);
    let galley = painter.layout(
        text.to_string(),
        FontId::proportional(13.0),
        p.title.gamma_multiply(a),
        max_w - 24.0,
    );
    let r = Rect::from_min_size(at, galley.size() + vec2(24.0, 18.0));
    painter.rect(
        r,
        3.0,
        p.panel_strong.gamma_multiply(a),
        Stroke::new(1.0, p.line_strong.gamma_multiply(a)),
        StrokeKind::Inside,
    );
    // Tail toward the drone.
    painter.add(Shape::convex_polygon(
        vec![
            r.left_bottom() + vec2(14.0, -1.0),
            r.left_bottom() + vec2(30.0, -1.0),
            r.left_bottom() + vec2(4.0, 16.0),
        ],
        p.panel_strong.gamma_multiply(a),
        Stroke::NONE,
    ));
    painter.rect_filled(
        Rect::from_min_size(r.min, vec2(3.0, r.height())),
        0.0,
        p.glow.gamma_multiply(a),
    );
    painter.galley(r.min + vec2(12.0, 9.0), galley, p.title);
}

enum Tone {
    Good,
    Bad,
    Plain,
}

fn cheat_name(id: CheatId) -> &'static str {
    card_text(id).title
}

fn describe(kind: &LogKind) -> (String, Tone) {
    match kind {
        LogKind::Attached(pid) => (
            trf!(
                "Conectado ao jogo (PID {pid}).",
                "Connected to the game (PID {pid})."
            ),
            Tone::Good,
        ),
        LogKind::Detached => (
            tr!("O jogo fechou.", "The game closed.").into(),
            Tone::Plain,
        ),
        LogKind::On(id) => (trf!("{} ligado.", "{} on.", cheat_name(*id)), Tone::Good),
        LogKind::Off(id) => (
            trf!("{} desligado.", "{} off.", cheat_name(*id)),
            Tone::Plain,
        ),
        LogKind::Error(id, e) => (trf!("{}: {}", "{}: {}", cheat_name(*id), e), Tone::Bad),
        LogKind::Killed(o) => (
            trf!("Inimigo {o:#07X} derrotado.", "Enemy {o:#07X} defeated."),
            Tone::Plain,
        ),
        LogKind::Spawned(o) => (
            trf!("Inimigo {o:#07X} voltou.", "Enemy {o:#07X} is back."),
            Tone::Good,
        ),
        LogKind::SpawnFailed(o) => (
            trf!(
                "Não consegui trazer {o:#07X} de volta.",
                "Couldn't bring {o:#07X} back."
            ),
            Tone::Bad,
        ),
    }
}

fn ago(at: Instant) -> String {
    let s = at.elapsed().as_secs();
    if s < 60 {
        format!("{s:>3}s")
    } else {
        format!("{:>3}m", s / 60)
    }
}

fn footer(painter: &Painter, screen: Rect, p: &Palette) {
    let y = screen.bottom() - 17.0;
    painter.text(
        pos2(screen.left() + 22.0, y),
        Align2::LEFT_CENTER,
        tr!(
            "Só para single-player. Sem afiliação com a Square Enix ou a PlatinumGames.",
            "Single-player only. Not affiliated with Square Enix or PlatinumGames."
        ),
        FontId::proportional(11.0),
        p.dim,
    );
    painter.text(
        pos2(screen.right() - 22.0, y),
        Align2::RIGHT_CENTER,
        concat!("v", env!("CARGO_PKG_VERSION")),
        theme::hud(11.0),
        p.dim,
    );
}

// ---------------------------------------------------------------------------------------
// Settings.

fn settings_window(app: &mut App, ctx: &egui::Context, p: &Palette, now: f64) {
    let mut open = true;
    let mut changed = false;
    egui::Window::new(theme::hud_text(tr!("CONFIGURAÇÕES", "SETTINGS"), 15.0).color(p.title))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .anchor(Align2::RIGHT_TOP, [-18.0, 62.0])
        .default_width(330.0)
        .show(ctx, |ui| {
            let s = &mut app.settings;
            ui.label(RichText::new(tr!("Cenário", "Scenery")).color(p.muted));
            ui.horizontal(|ui| {
                for (v, label) in [
                    (Scenery::Night, tr!("Lírios à noite", "Night lilies")),
                    (Scenery::Ruins, tr!("Ruínas na névoa", "Ruins in the fog")),
                    (Scenery::Custom, tr!("Minha imagem", "My picture")),
                ] {
                    if ui.selectable_label(s.scenery == v, label).clicked() {
                        s.scenery = v;
                        changed = true;
                    }
                }
            });
            if s.scenery == Scenery::Custom {
                ui.horizontal(|ui| {
                    if ui
                        .button(format!(
                            "{}  {}",
                            ic::FOLDER,
                            tr!("Escolher imagem…", "Choose picture…")
                        ))
                        .clicked()
                    {
                        if let Some(path) = crate::picker::pick_image() {
                            s.custom_background = Some(path);
                            changed = true;
                        }
                    }
                    let name = s
                        .custom_background
                        .as_ref()
                        .and_then(|p| p.file_name())
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| tr!("nenhuma", "none").into());
                    ui.label(RichText::new(name).color(p.dim));
                });
                ui.label(
                    RichText::new(tr!(
                        "A imagem fica só no seu computador; o trainer guarda apenas o caminho.",
                        "The picture stays on your computer; the trainer only remembers its path."
                    ))
                    .size(11.0)
                    .color(p.dim),
                );
            }
            ui.add_space(6.0);
            ui.label(RichText::new(tr!("Idioma", "Language")).color(p.muted));
            ui.horizontal(|ui| {
                use crate::i18n::Language;
                for (v, label) in [
                    (Language::Auto, tr!("Automático", "Automatic")),
                    (Language::Pt, "Português"),
                    (Language::En, "English"),
                ] {
                    if ui.selectable_label(s.language == v, label).clicked() {
                        s.language = v;
                        crate::i18n::set_language(v);
                        changed = true;
                    }
                }
            });
            ui.add_space(6.0);
            changed |= ui
                .checkbox(&mut s.animated, tr!("Animações", "Animations"))
                .changed();
            changed |= ui
                .checkbox(
                    &mut s.hotkeys,
                    tr!("Teclas de atalho (F1–F4, F6)", "Hotkeys (F1–F4, F6)"),
                )
                .changed();
        });
    if !open {
        app.show_settings = false;
    }
    if changed {
        app.changed(now);
    }
}

/// The window icon: a lit diamond, drawn in code.
pub fn icon_image() -> egui::IconData {
    const N: usize = 64;
    let mut rgba = vec![0u8; N * N * 4];
    let c = (N as f32 - 1.0) / 2.0;
    for y in 0..N {
        for x in 0..N {
            let (dx, dy) = ((x as f32 - c).abs(), (y as f32 - c).abs());
            let d = dx + dy; // diamond distance
            let i = (y * N + x) * 4;
            let (r, g, b, a) = if d < 9.0 {
                (168, 216, 255, 255)
            } else if d < 13.0 {
                (20, 22, 34, 255)
            } else if d < 26.0 {
                (232, 226, 205, 255)
            } else if d < 30.0 {
                let f = 1.0 - (d - 26.0) / 4.0;
                (168, 216, 255, (f * 160.0) as u8)
            } else {
                (0, 0, 0, 0)
            };
            rgba[i..i + 4].copy_from_slice(&[r, g, b, a]);
        }
    }
    egui::IconData {
        rgba,
        width: N as u32,
        height: N as u32,
    }
}
