//! The window: title bar, the module list, the detail panel of the selected module, the
//! support unit with its message window, the log and the settings popup.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CursorIcon, Id, Order, Painter, Pos2, Rect, Sense, Stroke,
    Ui, ViewportCommand,
};

use crate::game::cheats::{CheatId, Values};
use crate::game::respawn;
use crate::scene::{self, Scene};
use crate::settings::{Scenery, Settings};
use crate::sprites::Drone;
use crate::theme::{self, ic, BAD, DARK, INK, INK_FAINT, INK_SOFT, OK, ON_DARK, PAPER_HI, WAIT};
use crate::trainer::{self, LogEntry, LogKind, Status, Trainer, Wanted};
use crate::widgets;

pub struct App {
    settings: Settings,
    dirty_since: Option<f64>,
    trainer: Trainer,
    scene: Scene,
    drone: Drone,
    selected: usize,
    clock: f64,
    last_time: f64,
    /// The newest log entry already seen (to pulse the unit and update its message).
    seen: Option<Instant>,
    message: Option<(String, f64)>,
    show_settings: bool,
    show_credits: bool,
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
        theme::fonts(&cc.egui_ctx);
        theme::apply(&cc.egui_ctx);
        let trainer = Trainer::start(wanted(&settings));
        #[cfg_attr(not(debug_assertions), allow(unused_mut))]
        let mut app = Self {
            settings,
            dirty_since: None,
            trainer,
            scene: Scene::default(),
            drone: Drone::default(),
            selected: 0,
            clock: 0.0,
            last_time: 0.0,
            seen: None,
            message: None,
            show_settings: false,
            show_credits: false,
        };
        // Debug builds: `YORHA_PREVIEW=module:3` or `settings` opens straight into that view
        // (for screenshots).
        #[cfg(debug_assertions)]
        if let Ok(v) = std::env::var("YORHA_PREVIEW") {
            if let Some(n) = v
                .strip_prefix("module:")
                .and_then(|n| n.parse::<usize>().ok())
            {
                app.selected = n.clamp(1, 4) - 1;
            }
            app.show_settings = v == "settings";
            app.show_credits = v == "credits";
        }
        app
    }

    fn changed(&mut self, now: f64) {
        self.dirty_since.get_or_insert(now);
    }

    /// Syncs with the worker: our wishes go in, its state comes out.
    fn exchange(&mut self, now: f64) -> View {
        let mut hotkey_change = false;
        let view = {
            let mut s = self.trainer.shared.lock().unwrap();
            if std::mem::take(&mut s.toggled_by_hotkey) {
                hotkey_change = true;
                for (k, id) in CheatId::ALL.iter().enumerate() {
                    self.settings.set_enabled(*id, s.wanted.enabled[k]);
                }
            }
            s.wanted = wanted(&self.settings);
            View {
                status: s.status.clone(),
                active: s.active,
                supported: s.supported,
                log: s.log.iter().take(30).cloned().collect(),
                kills: s.kills,
                respawns: s.respawns,
            }
        };
        if hotkey_change {
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

struct Module {
    name: &'static str,
    key: &'static str,
    summary: &'static str,
}

fn module(id: CheatId) -> Module {
    match id {
        CheatId::AutoCollect => Module {
            name: "AUTO-COLLECT",
            key: "F1",
            summary: "Dropped items fly to you as if the Auto-Collect Items plug-in chip were \
                      equipped — without taking a single chip slot.",
        },
        CheatId::Range => Module {
            name: "COLLECTION RANGE",
            key: "F2",
            summary: "Extends how far auto-collect reaches. Also picks up distant items whose \
                      model the game has already unloaded.",
        },
        CheatId::Respawn => Module {
            name: "ENEMY RESPAWN",
            key: "F3",
            summary: "Defeated enemies come back after a short delay. Ideal for farming \
                      materials, money and experience. If they keep dying the instant they \
                      return, pick AT ITS SPAWN POINT so they don't land in your attacks.",
        },
        CheatId::ItemCap => Module {
            name: "STACK LIMIT",
            key: "F4",
            summary: "Lifts the 99-per-item limit. Items with a limit of their own, such as key \
                      items, are left untouched.",
        },
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
    let view = app.exchange(now);

    // Something new in the log: the unit pulses and reports it.
    if let Some(first) = view.log.first() {
        if app.seen != Some(first.at) {
            if app.seen.is_some() {
                app.drone.pulse(t);
            }
            app.seen = Some(first.at);
            app.message = Some((describe(&first.kind).0, now));
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
    atmosphere(&painter, screen);

    let bar = Rect::from_min_size(screen.min, vec2(screen.width(), 44.0));
    let drag = ui.interact(bar, Id::unique("title-drag"), Sense::click_and_drag());
    if drag.drag_started() {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }
    let gear = title_bar(app, ui, &painter, bar, &view, t);

    let left = Rect::from_min_max(
        pos2(screen.left() + 30.0, bar.bottom() + 24.0),
        pos2(screen.left() + 30.0 + 420.0, screen.bottom() - 40.0),
    );
    let modules_bottom = module_list(app, ui, &painter, left, &view, now);
    let detail = Rect::from_min_max(pos2(left.left(), modules_bottom + 20.0), left.max);
    detail_panel(app, ui, &painter, detail, now);

    let log = Rect::from_min_max(
        pos2(
            screen.right() - 30.0 - 400.0,
            screen.bottom() - 40.0 - 176.0,
        ),
        pos2(screen.right() - 30.0, screen.bottom() - 40.0),
    );
    log_panel(&painter, log, &view);
    support_unit(
        app,
        &painter,
        Rect::from_min_max(
            pos2(left.right() + 24.0, bar.bottom()),
            pos2(log.left() - 16.0, screen.bottom() - 40.0),
        ),
        &view,
        t,
        now,
        pointer,
    );
    footer(&painter, screen);

    if app.show_credits {
        credits_popup(app, &ctx, gear.translate(vec2(-46.0, 0.0)));
    }
    if app.show_settings {
        settings_popup(app, &ctx, gear, now);
    }

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

/// Vignette and faint scan lines over the art.
fn atmosphere(painter: &Painter, r: Rect) {
    let bottom = Rect::from_min_max(pos2(r.left(), r.bottom() - 140.0), r.max);
    scene::gradient(
        painter,
        bottom,
        Color32::TRANSPARENT,
        Color32::from_black_alpha(150),
    );
    let side = Rect::from_min_max(r.min, pos2(r.left() + 520.0, r.bottom()));
    let mut m = egui::Mesh::default();
    m.colored_vertex(side.left_top(), Color32::from_black_alpha(90));
    m.colored_vertex(side.right_top(), Color32::TRANSPARENT);
    m.colored_vertex(side.left_bottom(), Color32::from_black_alpha(90));
    m.colored_vertex(side.right_bottom(), Color32::TRANSPARENT);
    m.add_triangle(0, 1, 2);
    m.add_triangle(1, 2, 3);
    painter.add(m);
    let mut y = r.top();
    while y < r.bottom() {
        painter.line_segment(
            [pos2(r.left(), y), pos2(r.right(), y)],
            Stroke::new(1.0, Color32::from_black_alpha(14)),
        );
        y += 3.0;
    }
}

// ---------------------------------------------------------------------------------------
// Title bar.

/// Returns the settings button's rect (the popup hangs under it).
fn title_bar(
    app: &mut App,
    ui: &mut Ui,
    painter: &Painter,
    bar: Rect,
    view: &View,
    t: f32,
) -> Rect {
    let ctx = ui.ctx().clone();
    painter.rect_filled(bar, 0.0, DARK);
    painter.line_segment(
        [
            bar.left_bottom() + vec2(0.0, 3.0),
            bar.right_bottom() + vec2(0.0, 3.0),
        ],
        Stroke::new(1.0, ON_DARK.gamma_multiply(0.35)),
    );

    let logo = pos2(bar.left() + 26.0, bar.center().y);
    scene::glow(
        painter,
        logo,
        18.0,
        theme::GLOW,
        0.35 + 0.15 * (t * 1.5).sin(),
    );
    let d = |r: f32| {
        vec![
            logo + vec2(0.0, -r),
            logo + vec2(r, 0.0),
            logo + vec2(0.0, r),
            logo + vec2(-r, 0.0),
        ]
    };
    painter.add(egui::Shape::convex_polygon(
        d(8.0),
        Color32::TRANSPARENT,
        Stroke::new(1.5, ON_DARK),
    ));
    painter.add(egui::Shape::convex_polygon(d(3.5), ON_DARK, Stroke::NONE));
    let title = theme::spaced(
        painter,
        pos2(logo.x + 22.0, bar.center().y),
        Align2::LEFT_CENTER,
        "YORHA TRAINER",
        theme::hud(16.0),
        ON_DARK,
        4.0,
    );
    painter.line_segment(
        [
            pos2(title.right() + 16.0, bar.top() + 14.0),
            pos2(title.right() + 16.0, bar.bottom() - 14.0),
        ],
        Stroke::new(1.0, ON_DARK.gamma_multiply(0.4)),
    );
    theme::spaced(
        painter,
        pos2(title.right() + 32.0, bar.center().y),
        Align2::LEFT_CENTER,
        "NIER:AUTOMATA  ·  STEAM",
        theme::hud(11.0),
        ON_DARK.gamma_multiply(0.6),
        2.5,
    );

    let close = Rect::from_min_size(
        pos2(bar.right() - 46.0, bar.top()),
        vec2(46.0, bar.height()),
    );
    if widgets::window_button(ui, close, ic::CLOSE, true, "close") {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
    let min = close.translate(vec2(-46.0, 0.0));
    if widgets::window_button(ui, min, ic::MINIMIZE, false, "min") {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }
    let gear = min.translate(vec2(-46.0, 0.0));
    if widgets::window_button(ui, gear, ic::SETTINGS, false, "settings") {
        app.show_settings = !app.show_settings;
        app.show_credits = false;
    }
    let info = gear.translate(vec2(-46.0, 0.0));
    if widgets::window_button(ui, info, ic::INFO, false, "credits") {
        app.show_credits = !app.show_credits;
        app.show_settings = false;
    }

    // Connection status.
    let (text, color) = match &view.status {
        Status::Waiting => ("WAITING FOR THE GAME".to_string(), WAIT),
        Status::Attached { pid } => (format!("CONNECTED  ·  PID {pid}"), OK),
        Status::NoAccess { .. } => ("NO ACCESS — RUN AS ADMINISTRATOR".to_string(), BAD),
    };
    let font = theme::hud(11.5);
    let probe = theme::spaced(
        &painter.with_clip_rect(Rect::NOTHING),
        Pos2::ZERO,
        Align2::LEFT_TOP,
        &text,
        font.clone(),
        ON_DARK,
        1.8,
    );
    let chip = Rect::from_min_size(
        pos2(
            info.left() - 18.0 - probe.width() - 32.0,
            bar.center().y - 12.0,
        ),
        vec2(probe.width() + 32.0, 24.0),
    );
    painter.rect_stroke(
        chip,
        0.0,
        Stroke::new(1.0, ON_DARK.gamma_multiply(0.35)),
        egui::StrokeKind::Inside,
    );
    let dot = pos2(chip.left() + 13.0, chip.center().y);
    scene::glow(painter, dot, 10.0, color, 0.6 + 0.4 * (t * 3.0).sin());
    painter.rect_filled(Rect::from_center_size(dot, vec2(6.0, 6.0)), 0.0, color);
    theme::spaced(
        painter,
        pos2(chip.left() + 24.0, chip.center().y),
        Align2::LEFT_CENTER,
        &text,
        font,
        ON_DARK,
        1.8,
    );
    gear
}

// ---------------------------------------------------------------------------------------
// Module list.

/// Draws the list; returns where it ends.
fn module_list(
    app: &mut App,
    ui: &mut Ui,
    painter: &Painter,
    area: Rect,
    view: &View,
    now: f64,
) -> f32 {
    const ROW: f32 = 58.0;
    const HEAD: f32 = 30.0;
    let active = view.active.iter().filter(|a| **a).count();
    let r = Rect::from_min_size(area.min, vec2(area.width(), HEAD + 4.0 * ROW + 10.0));
    widgets::panel(painter, r);
    widgets::header(
        painter,
        Rect::from_min_size(r.min, vec2(r.width(), HEAD)),
        "MODULES",
        &format!("{active} / 4 ACTIVE"),
    );
    let attached = matches!(view.status, Status::Attached { .. });

    for (k, id) in CheatId::ALL.iter().enumerate() {
        let m = module(*id);
        let i = trainer::index(*id);
        let row = Rect::from_min_size(
            pos2(r.left() + 6.0, r.top() + HEAD + 5.0 + ROW * k as f32),
            vec2(r.width() - 12.0, ROW),
        );
        let toggle_rect = Rect::from_min_size(
            pos2(row.right() - 88.0, row.center().y - 12.0),
            vec2(80.0, 24.0),
        );

        // Selecting the row (the switch handles its own clicks).
        let resp = ui
            .interact(row, Id::unique(("module-row", k)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if resp.clicked() && !toggle_rect.contains(resp.interact_pointer_pos().unwrap_or_default())
        {
            app.selected = k;
        }
        let selected = app.selected == k;
        let sel = ui
            .ctx()
            .animate_bool_with_time(Id::unique(("row-sel", k)), selected, 0.12);
        if resp.hovered() && !selected {
            painter.rect_filled(row, 0.0, PAPER_HI);
        }
        if sel > 0.0 {
            let w = row.width() * sel;
            painter.rect_filled(
                Rect::from_min_size(row.min, vec2(w, row.height())),
                0.0,
                DARK,
            );
            // The cursor outside the panel.
            let cy = row.center().y;
            let cx = r.left() - 14.0;
            painter.rect_filled(
                Rect::from_center_size(pos2(cx, cy), vec2(8.0, 8.0)),
                0.0,
                ON_DARK.gamma_multiply(sel),
            );
            painter.line_segment(
                [pos2(cx + 6.0, cy), pos2(r.left() - 2.0, cy)],
                Stroke::new(1.0, ON_DARK.gamma_multiply(sel)),
            );
        }
        let fg = INK.lerp_to_gamma(ON_DARK, sel);
        let soft = INK_SOFT.lerp_to_gamma(ON_DARK.gamma_multiply(0.7), sel);

        painter.text(
            pos2(row.left() + 14.0, row.center().y),
            Align2::LEFT_CENTER,
            format!("{:02}", k + 1),
            theme::hud(22.0),
            soft,
        );
        theme::spaced(
            painter,
            pos2(row.left() + 54.0, row.top() + 19.0),
            Align2::LEFT_CENTER,
            m.name,
            theme::hud(14.0),
            fg,
            2.0,
        );
        let (state, color) = if !view.supported[i] {
            ("NOT FOUND IN THIS VERSION", BAD)
        } else if view.active[i] {
            ("ACTIVE", OK)
        } else if app.settings.enabled(*id) && !attached {
            ("ARMED · WAITING FOR THE GAME", WAIT)
        } else if app.settings.enabled(*id) {
            ("APPLYING…", WAIT)
        } else {
            ("OFF", soft)
        };
        let dot = pos2(row.left() + 57.0, row.top() + 39.0);
        painter.rect_filled(Rect::from_center_size(dot, vec2(5.0, 5.0)), 0.0, color);
        theme::spaced(
            painter,
            pos2(dot.x + 8.0, dot.y),
            Align2::LEFT_CENTER,
            state,
            theme::hud(10.0),
            soft,
            1.4,
        );
        if app.settings.hotkeys {
            let key = Rect::from_min_size(
                pos2(toggle_rect.left() - 34.0, row.center().y - 10.0),
                vec2(26.0, 20.0),
            );
            painter.rect_stroke(key, 0.0, Stroke::new(1.0, soft), egui::StrokeKind::Inside);
            painter.text(
                key.center(),
                Align2::CENTER_CENTER,
                m.key,
                theme::hud(10.5),
                soft,
            );
        }
        let mut on = app.settings.enabled(*id);
        if widgets::toggle(
            ui,
            toggle_rect,
            &mut on,
            view.supported[i],
            ("module-toggle", k),
        ) {
            app.settings.set_enabled(*id, on);
            app.selected = k;
            app.changed(now);
        }
        if k < 3 && !selected && app.selected != k + 1 {
            painter.line_segment(
                [
                    pos2(row.left() + 14.0, row.bottom()),
                    pos2(row.right() - 14.0, row.bottom()),
                ],
                Stroke::new(1.0, INK_FAINT),
            );
        }
    }
    r.bottom()
}

// ---------------------------------------------------------------------------------------
// Detail of the selected module.

fn detail_panel(app: &mut App, ui: &mut Ui, painter: &Painter, r: Rect, now: f64) {
    let id = CheatId::ALL[app.selected];
    let m = module(id);
    let inner_w = r.width() - 36.0;
    let galley = painter.layout(m.summary.to_string(), theme::body(13.0), INK, inner_w);
    let text_h = galley.size().y;
    // The panel is as tall as its content.
    let controls = match id {
        CheatId::AutoCollect => 78.0,
        CheatId::Range => 100.0,
        CheatId::Respawn => 134.0,
        CheatId::ItemCap => 118.0,
    };
    let height = 30.0 + 14.0 + text_h + 32.0 + controls;
    let r = Rect::from_min_size(r.min, vec2(r.width(), height.min(r.height())));
    widgets::panel(painter, r);
    let head = Rect::from_min_size(r.min, vec2(r.width(), 30.0));
    widgets::header(
        painter,
        head,
        &format!("{:02}  ·  {}", app.selected + 1, m.name),
        if app.settings.hotkeys { m.key } else { "" },
    );
    let inner = r.shrink2(vec2(18.0, 0.0));
    let text_top = head.bottom() + 14.0;
    painter.galley(pos2(inner.left(), text_top), galley, INK);
    let mut y = text_top + text_h + 16.0;
    widgets::rule(painter, pos2(inner.left(), y), pos2(inner.right(), y));
    y += 16.0;

    let s = &mut app.settings;
    let mut changed = false;
    let label = |painter: &Painter, y: f32, name: &str, value: &str, note: &str| {
        theme::spaced(
            painter,
            pos2(inner.left(), y),
            Align2::LEFT_CENTER,
            name,
            theme::hud(11.5),
            INK_SOFT,
            2.0,
        );
        let v = painter.text(
            pos2(inner.right(), y),
            Align2::RIGHT_CENTER,
            value,
            theme::hud(20.0),
            INK,
        );
        if !note.is_empty() {
            theme::spaced(
                painter,
                pos2(v.left() - 10.0, y + 2.0),
                Align2::RIGHT_CENTER,
                note,
                theme::hud(10.0),
                INK_SOFT,
                1.2,
            );
        }
    };
    match id {
        CheatId::AutoCollect => {
            for (k, line) in [
                "No plug-in chip needed: keep every slot for combat chips.",
                "Combine with COLLECTION RANGE (F2) to vacuum a whole area.",
                "Works on materials, money, recovery items and drops alike.",
            ]
            .iter()
            .enumerate()
            {
                let ly = y + 6.0 + k as f32 * 24.0;
                painter.rect_filled(
                    Rect::from_center_size(pos2(inner.left() + 3.0, ly), vec2(5.0, 5.0)),
                    0.0,
                    INK_SOFT,
                );
                painter.text(
                    pos2(inner.left() + 16.0, ly),
                    Align2::LEFT_CENTER,
                    *line,
                    theme::body(12.5),
                    INK,
                );
            }
        }
        CheatId::Range => {
            label(
                painter,
                y + 4.0,
                "REACH",
                &format!("{:.0}", s.range),
                "GAME DEFAULT 4.5",
            );
            let sl = Rect::from_min_size(pos2(inner.left(), y + 26.0), vec2(inner.width(), 22.0));
            changed |= widgets::slider(ui, sl, &mut s.range, 4.0, 500.0, true, 1.0, "range-slider");
            let presets = ["25", "50", "100", "250", "500"];
            let values = [25.0, 50.0, 100.0, 250.0, 500.0];
            let sel = values.iter().position(|v| (*v - s.range).abs() < 0.5);
            let seg = Rect::from_min_size(pos2(inner.left(), y + 64.0), vec2(inner.width(), 28.0));
            if let Some(k) = widgets::segmented(ui, seg, &presets, sel, "range-presets") {
                s.range = values[k];
                changed = true;
            }
        }
        CheatId::Respawn => {
            label(
                painter,
                y + 4.0,
                "DELAY",
                &format!("{:.1} s", s.respawn_delay),
                "",
            );
            let sl = Rect::from_min_size(pos2(inner.left(), y + 26.0), vec2(inner.width(), 22.0));
            changed |= widgets::slider(
                ui,
                sl,
                &mut s.respawn_delay,
                0.0,
                30.0,
                false,
                0.5,
                "delay-slider",
            );
            let seg = Rect::from_min_size(pos2(inner.left(), y + 62.0), vec2(inner.width(), 28.0));
            let sel = Some(if s.respawn_at_death { 0 } else { 1 });
            if let Some(k) = widgets::segmented(
                ui,
                seg,
                &["WHERE IT DIED", "AT ITS SPAWN POINT"],
                sel,
                "respawn-where",
            ) {
                s.respawn_at_death = k == 0;
                changed = true;
            }
            changed |= widgets::checkbox(
                ui,
                pos2(inner.left(), y + 104.0),
                "Respawn automatically",
                &mut s.respawn_auto,
                "respawn-auto",
            );
            let label = if s.hotkeys {
                "SPAWN LAST AGAIN  ·  F6"
            } else {
                "SPAWN LAST AGAIN"
            };
            let b = Rect::from_min_size(pos2(inner.right() - 190.0, y + 99.0), vec2(190.0, 28.0));
            if widgets::button(ui, b, label, "respawn-last") {
                app.trainer.respawn_last();
            }
        }
        CheatId::ItemCap => {
            label(
                painter,
                y + 4.0,
                "LIMIT PER ITEM",
                &group(s.item_cap),
                "GAME DEFAULT 99",
            );
            let values = [999u32, 9_999, 99_999, 999_999];
            let labels = ["999", "9,999", "99,999", "999,999"];
            let sel = values.iter().position(|v| *v == s.item_cap);
            let seg = Rect::from_min_size(pos2(inner.left(), y + 30.0), vec2(inner.width(), 30.0));
            if let Some(k) = widgets::segmented(ui, seg, &labels, sel, "cap-presets") {
                s.item_cap = values[k];
                changed = true;
            }
            painter.text(
                pos2(inner.left(), y + 80.0),
                Align2::LEFT_TOP,
                "Applies the next time an item is added. Lowering it later\ndoesn't remove what you already carry.",
                theme::body(12.0),
                INK_SOFT,
            );
        }
    }
    if changed {
        app.changed(now);
    }
}

fn group(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (k, c) in s.chars().enumerate() {
        if k > 0 && (s.len() - k).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

// ---------------------------------------------------------------------------------------
// Support unit, its message window and the log.

#[allow(clippy::too_many_arguments)]
fn support_unit(
    app: &mut App,
    painter: &Painter,
    area: Rect,
    view: &View,
    t: f32,
    now: f64,
    pointer: Option<Pos2>,
) {
    let color = match view.status {
        Status::Attached { .. } => theme::GLOW,
        Status::Waiting => Color32::from_rgb(0xff, 0xd2, 0x8a),
        Status::NoAccess { .. } => Color32::from_rgb(0xff, 0x8a, 0x7a),
    };
    // The message window sits at the top of the free strip (where both pictures have sky or
    // fog), and the unit floats just below it, clear of the characters.
    let (text, since) = match &app.message {
        Some((text, at)) if now - at < 8.0 => (text.clone(), now - at),
        _ => (idle_line(&view.status).to_string(), 99.0),
    };
    let shown: String = text.chars().take((since * 55.0) as usize).collect();
    let w = (area.width() - 4.0).clamp(200.0, 280.0);
    let galley = painter.layout(text.clone(), theme::body(12.5), INK, w - 28.0);
    let h = galley.size().y + 50.0;
    let win = Rect::from_min_size(pos2(area.left(), area.top() + 24.0), vec2(w, h));
    widgets::panel(painter, win);
    widgets::header(
        painter,
        Rect::from_min_size(win.min, vec2(win.width(), 24.0)),
        "SUPPORT UNIT",
        "",
    );
    let typed = painter.layout(shown, theme::body(12.5), INK, w - 28.0);
    painter.galley(pos2(win.left() + 14.0, win.top() + 34.0), typed, INK);
    if since < 99.0 && (now * 2.0) as i64 % 2 == 0 {
        painter.rect_filled(
            Rect::from_min_size(
                pos2(win.right() - 16.0, win.bottom() - 12.0),
                vec2(6.0, 6.0),
            ),
            0.0,
            INK,
        );
    }

    let at = pos2(win.left() + 76.0, win.bottom() + 120.0);
    // A line from the window down to the unit.
    let from = pos2(at.x, win.bottom() + 5.0);
    let to = pos2(at.x, at.y - 56.0);
    painter.line_segment([from, to], Stroke::new(1.0, ON_DARK.gamma_multiply(0.7)));
    painter.rect_filled(Rect::from_center_size(to, vec2(4.0, 4.0)), 0.0, ON_DARK);
    app.drone.paint(painter, at, 1.1, t, pointer, color);
}

fn idle_line(status: &Status) -> &'static str {
    match status {
        Status::Waiting => "Standing by. Launch NIER:AUTOMATA and I'll link up automatically.",
        Status::Attached { .. } => "Link established. Select a module and switch it on.",
        Status::NoAccess { .. } => {
            "Windows is blocking access to the game process. Run the trainer as administrator."
        }
    }
}

fn log_panel(painter: &Painter, r: Rect, view: &View) {
    widgets::panel(painter, r);
    let head = Rect::from_min_size(r.min, vec2(r.width(), 28.0));
    widgets::header(
        painter,
        head,
        "LOG",
        &format!("DEFEATED {}  ·  RESPAWNED {}", view.kills, view.respawns),
    );
    let mut y = head.bottom() + 12.0;
    if view.log.is_empty() {
        painter.text(
            pos2(r.left() + 16.0, y),
            Align2::LEFT_TOP,
            "No events yet.",
            theme::body(12.5),
            INK_SOFT,
        );
        return;
    }
    let clip = painter.with_clip_rect(r.shrink(4.0));
    for e in &view.log {
        if y > r.bottom() - 22.0 {
            break;
        }
        let (text, tone) = describe(&e.kind);
        let color = match tone {
            Tone::Good => OK,
            Tone::Bad => BAD,
            Tone::Plain => INK,
        };
        clip.text(
            pos2(r.left() + 16.0, y),
            Align2::LEFT_TOP,
            ago(e.at),
            egui::FontId::monospace(11.0),
            INK_SOFT,
        );
        clip.rect_filled(
            Rect::from_center_size(pos2(r.left() + 66.0, y + 7.5), vec2(4.0, 4.0)),
            0.0,
            color,
        );
        clip.text(
            pos2(r.left() + 76.0, y),
            Align2::LEFT_TOP,
            text,
            theme::body(12.5),
            INK,
        );
        y += 21.0;
    }
}

enum Tone {
    Good,
    Bad,
    Plain,
}

fn describe(kind: &LogKind) -> (String, Tone) {
    let name = |id: CheatId| {
        let n = module(id).name;
        let mut s = n.to_lowercase();
        if let Some(f) = s.get_mut(0..1) {
            f.make_ascii_uppercase();
        }
        s
    };
    match kind {
        LogKind::Attached(pid) => (format!("Linked to the game (PID {pid})."), Tone::Good),
        LogKind::Detached => ("The game closed. Link lost.".into(), Tone::Plain),
        LogKind::On(id) => (format!("{} enabled.", name(*id)), Tone::Good),
        LogKind::Off(id) => (format!("{} disabled.", name(*id)), Tone::Plain),
        LogKind::Error(id, e) => (format!("{}: {e}", name(*id)), Tone::Bad),
        LogKind::Killed(o) => (format!("Enemy {o:05X} defeated."), Tone::Plain),
        LogKind::Spawned(o) => (format!("Enemy {o:05X} is back."), Tone::Good),
        LogKind::SpawnFailed(o) => (format!("Couldn't bring enemy {o:05X} back."), Tone::Bad),
        LogKind::SpawnSkipped(o) => (
            format!("Enemy {o:05X}: the area already respawned it."),
            Tone::Plain,
        ),
    }
}

fn ago(at: Instant) -> String {
    let s = at.elapsed().as_secs();
    format!("{:02}:{:02}", s / 60 % 100, s % 60)
}

fn footer(painter: &Painter, screen: Rect) {
    let y = screen.bottom() - 18.0;
    let c = ON_DARK.gamma_multiply(0.6);
    theme::spaced(
        painter,
        pos2(screen.left() + 30.0, y),
        Align2::LEFT_CENTER,
        "BY OZ VESSALIUS (@SILHATOTE)  ·  SINGLE-PLAYER ONLY  ·  NOT AFFILIATED WITH SQUARE ENIX OR PLATINUMGAMES",
        theme::hud(9.5),
        c,
        1.5,
    );
    theme::spaced(
        painter,
        pos2(screen.right() - 30.0, y),
        Align2::RIGHT_CENTER,
        concat!("V", env!("CARGO_PKG_VERSION")),
        theme::hud(10.0),
        c,
        1.5,
    );
}

// ---------------------------------------------------------------------------------------
// Settings popup.

fn settings_popup(app: &mut App, ctx: &egui::Context, gear: Rect, now: f64) {
    let w = 300.0;
    let custom = app.settings.scenery == Scenery::Custom;
    let h = 30.0 + 22.0 + 3.0 * 34.0 + if custom { 52.0 } else { 0.0 } + 92.0;
    let r = Rect::from_min_size(
        pos2(gear.right() - w - 8.0, gear.bottom() + 12.0),
        vec2(w, h),
    );
    let mut changed = false;
    let mut close = false;
    egui::Area::new(Id::unique("settings-popup"))
        .order(Order::Foreground)
        .fixed_pos(r.min)
        .show(ctx, |ui| {
            ui.set_min_size(r.size());
            let painter = ui.painter().clone();
            widgets::panel(&painter, r);
            widgets::header(
                &painter,
                Rect::from_min_size(r.min, vec2(w, 30.0)),
                "SETTINGS",
                "",
            );
            let inner = r.shrink2(vec2(14.0, 0.0));
            let mut y = r.top() + 44.0;
            theme::spaced(
                &painter,
                pos2(inner.left(), y),
                Align2::LEFT_CENTER,
                "BACKGROUND",
                theme::hud(11.0),
                INK_SOFT,
                2.0,
            );
            y += 14.0;
            for (k, (value, label)) in [
                (Scenery::Night, "NIGHT LILIES"),
                (Scenery::Ruins, "RUINS IN THE FOG"),
                (Scenery::Custom, "MY OWN PICTURE"),
            ]
            .into_iter()
            .enumerate()
            {
                let row = Rect::from_min_size(pos2(inner.left(), y), vec2(inner.width(), 32.0));
                let resp = ui
                    .interact(row, Id::unique(("scenery", k)), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand);
                let on = app.settings.scenery == value;
                if resp.clicked() && !on {
                    app.settings.scenery = value;
                    changed = true;
                }
                if on {
                    painter.rect_filled(row, 0.0, DARK);
                } else if resp.hovered() {
                    painter.rect_filled(row, 0.0, PAPER_HI);
                }
                let fg = if on { ON_DARK } else { INK };
                painter.rect_filled(
                    Rect::from_center_size(pos2(row.left() + 12.0, row.center().y), vec2(6.0, 6.0)),
                    0.0,
                    fg,
                );
                theme::spaced(
                    &painter,
                    pos2(row.left() + 26.0, row.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    theme::hud(12.0),
                    fg,
                    1.8,
                );
                y += 34.0;
            }
            if custom {
                let b = Rect::from_min_size(pos2(inner.left(), y + 4.0), vec2(150.0, 28.0));
                if widgets::button(ui, b, "CHOOSE PICTURE…", "choose-picture") {
                    if let Some(path) = crate::picker::pick_image() {
                        app.settings.custom_background = Some(path);
                        changed = true;
                    }
                }
                let name = app
                    .settings
                    .custom_background
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "none".into());
                let clip = painter.with_clip_rect(Rect::from_min_max(
                    pos2(b.right() + 10.0, b.top()),
                    pos2(inner.right(), b.bottom()),
                ));
                clip.text(
                    pos2(b.right() + 10.0, b.center().y),
                    Align2::LEFT_CENTER,
                    name,
                    theme::body(12.0),
                    INK_SOFT,
                );
                y += 52.0;
            }
            y += 10.0;
            widgets::rule(&painter, pos2(inner.left(), y), pos2(inner.right(), y));
            y += 14.0;
            changed |= widgets::checkbox(
                ui,
                pos2(inner.left(), y),
                "Animations",
                &mut app.settings.animated,
                "set-anim",
            );
            y += 28.0;
            changed |= widgets::checkbox(
                ui,
                pos2(inner.left(), y),
                "Hotkeys (F1–F4, F6)",
                &mut app.settings.hotkeys,
                "set-keys",
            );
        });
    // Clicking anywhere else closes it.
    let clicked_outside = ctx.input(|i| {
        i.pointer.any_click()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !r.contains(p) && !gear.contains(p))
    });
    if clicked_outside {
        close = true;
    }
    if close {
        app.show_settings = false;
    }
    if changed {
        app.changed(now);
    }
}

// ---------------------------------------------------------------------------------------
// Credits popup.

pub const REPO_URL: &str = "https://github.com/Silhatote/yorha-trainer";

fn credits_popup(app: &mut App, ctx: &egui::Context, anchor: Rect) {
    let w = 380.0;
    let inner_w = w - 32.0;
    // Sections: (label, lines).
    let sections: [(&str, &str); 4] = [
        ("CREATED BY", "Oz Vessalius (@Silhatote)"),
        (
            "BACKGROUND ART",
            "NieR:Automata artwork. All rights belong to its creators. Upscaled with Upscayl \
             (Real-ESRGAN).",
        ),
        (
            "BUILT WITH",
            "Rust · egui / eframe · windows-rs · Segoe UI and Bahnschrift",
        ),
        (
            "RESEARCH",
            "Game reverse engineered with Cheat Engine. Notes in docs/RESEARCH.md.",
        ),
    ];
    let disclaimer = "NieR:Automata is a trademark of Square Enix, developed by PlatinumGames. \
                      This fan-made tool is not affiliated with or endorsed by them. For \
                      single-player use only.";
    let (galleys, disclaimer_g) = ctx.fonts_mut(|f| {
        let g: Vec<_> = sections
            .iter()
            .map(|(_, text)| f.layout(text.to_string(), theme::body(12.5), INK, inner_w))
            .collect();
        let d = f.layout(disclaimer.to_string(), theme::body(11.0), INK_SOFT, inner_w);
        (g, d)
    });
    let body_h: f32 = galleys.iter().map(|g| g.size().y + 30.0).sum();
    let h = 30.0 + 76.0 + body_h + 18.0 + disclaimer_g.size().y + 24.0;
    let r = Rect::from_min_size(pos2(anchor.right() - w, anchor.bottom() + 12.0), vec2(w, h));
    egui::Area::new(Id::unique("credits-popup"))
        .order(Order::Foreground)
        .fixed_pos(r.min)
        .show(ctx, |ui| {
            ui.set_min_size(r.size());
            let painter = ui.painter().clone();
            widgets::panel(&painter, r);
            widgets::header(
                &painter,
                Rect::from_min_size(r.min, vec2(w, 30.0)),
                "CREDITS",
                "",
            );
            let left = r.left() + 16.0;
            let mut y = r.top() + 50.0;
            let title = theme::spaced(
                &painter,
                pos2(left, y),
                Align2::LEFT_CENTER,
                "YORHA TRAINER",
                theme::hud(18.0),
                INK,
                3.5,
            );
            theme::spaced(
                &painter,
                pos2(title.right() + 12.0, y + 2.0),
                Align2::LEFT_CENTER,
                concat!("V", env!("CARGO_PKG_VERSION")),
                theme::hud(11.0),
                INK_SOFT,
                1.5,
            );
            // The repository link.
            let link = Rect::from_min_size(pos2(left, y + 16.0), vec2(inner_w, 26.0));
            let resp = ui
                .interact(link, Id::unique("credits-link"), Sense::click())
                .on_hover_cursor(CursorIcon::PointingHand);
            if resp.hovered() {
                painter.rect_filled(link, 0.0, PAPER_HI);
            }
            theme::spaced(
                &painter,
                pos2(link.left() + 6.0, link.center().y),
                Align2::LEFT_CENTER,
                "GITHUB.COM/SILHATOTE/YORHA-TRAINER",
                theme::hud(11.0),
                if resp.hovered() { INK } else { INK_SOFT },
                1.4,
            );
            if resp.clicked() {
                ctx.open_url(egui::OpenUrl::new_tab(REPO_URL));
            }
            y += 56.0;
            widgets::rule(&painter, pos2(left, y), pos2(r.right() - 16.0, y));
            y += 12.0;
            for ((label, _), g) in sections.iter().zip(galleys) {
                theme::spaced(
                    &painter,
                    pos2(left, y + 6.0),
                    Align2::LEFT_CENTER,
                    label,
                    theme::hud(10.5),
                    INK_SOFT,
                    2.0,
                );
                let gh = g.size().y;
                painter.galley(pos2(left, y + 16.0), g, INK);
                y += gh + 30.0;
            }
            widgets::rule(&painter, pos2(left, y), pos2(r.right() - 16.0, y));
            painter.galley(pos2(left, y + 10.0), disclaimer_g, INK_SOFT);
        });
    let clicked_outside = ctx.input(|i| {
        i.pointer.any_click()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !r.contains(p) && !anchor.contains(p))
    });
    if clicked_outside {
        app.show_credits = false;
    }
}

/// The window icon (the same picture as the executable's icon).
pub fn icon_image() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")).unwrap_or_default()
}
