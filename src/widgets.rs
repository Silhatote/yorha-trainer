//! The interface's own controls, drawn on paper with ink: panels, header strips, switches,
//! sliders, check boxes, segmented choices and buttons. Each one draws into a rect it is
//! given and uses `ui.interact` for the mouse.

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CursorIcon, Id, Painter, Pos2, Rect, Sense, Stroke,
    StrokeKind, Ui,
};

use crate::theme::{self, DARK, INK, INK_FAINT, INK_SOFT, ON_DARK, PAPER, PAPER_HI, SHADOW};

/// A sheet of paper with an offset shadow and a corner mark.
pub fn panel(painter: &Painter, r: Rect) {
    painter.rect_filled(r.translate(vec2(5.0, 5.0)), 0.0, SHADOW);
    painter.rect_filled(r, 0.0, PAPER);
    painter.rect_stroke(r, 0.0, Stroke::new(1.0, INK_FAINT), StrokeKind::Inside);
    // Corner ticks.
    let t = 10.0;
    let s = Stroke::new(1.5, INK);
    painter.line_segment([r.left_top(), r.left_top() + vec2(t, 0.0)], s);
    painter.line_segment([r.left_top(), r.left_top() + vec2(0.0, t)], s);
    painter.line_segment([r.right_bottom(), r.right_bottom() - vec2(t, 0.0)], s);
    painter.line_segment([r.right_bottom(), r.right_bottom() - vec2(0.0, t)], s);
}

/// The dark strip at the top of a panel, with a spaced title and optional text on the right.
pub fn header(painter: &Painter, r: Rect, title: &str, right: &str) {
    painter.rect_filled(r, 0.0, DARK);
    let y = r.center().y;
    painter.rect_filled(
        Rect::from_center_size(pos2(r.left() + 12.0, y), vec2(6.0, 6.0)),
        0.0,
        ON_DARK,
    );
    theme::spaced(
        painter,
        pos2(r.left() + 24.0, y),
        Align2::LEFT_CENTER,
        title,
        theme::hud(13.0),
        ON_DARK,
        2.5,
    );
    if !right.is_empty() {
        theme::spaced(
            painter,
            pos2(r.right() - 12.0, y),
            Align2::RIGHT_CENTER,
            right,
            theme::hud(11.5),
            ON_DARK.gamma_multiply(0.75),
            1.5,
        );
    }
}

/// A thin ink rule with a small square at the start.
pub fn rule(painter: &Painter, from: Pos2, to: Pos2) {
    painter.line_segment([from, to], Stroke::new(1.0, INK_FAINT));
    painter.rect_filled(Rect::from_center_size(from, vec2(4.0, 4.0)), 0.0, INK_SOFT);
}

/// ON/OFF switch: two cells and a dark block that slides to the chosen one.
pub fn toggle(
    ui: &mut Ui,
    r: Rect,
    on: &mut bool,
    enabled: bool,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> bool {
    let id = Id::unique(id);
    let resp = ui
        .interact(
            r,
            id,
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        )
        .on_hover_cursor(if enabled {
            CursorIcon::PointingHand
        } else {
            CursorIcon::NotAllowed
        });
    let mut changed = false;
    if enabled && resp.clicked() {
        *on = !*on;
        changed = true;
    }
    let x = ui.ctx().animate_bool_with_time(id.with("x"), *on, 0.14);
    let painter = ui.painter();
    painter.rect_filled(r, 0.0, PAPER_HI);
    painter.rect_stroke(
        r,
        0.0,
        Stroke::new(
            1.0,
            if resp.hovered() && enabled {
                INK
            } else {
                INK_FAINT
            },
        ),
        StrokeKind::Inside,
    );
    let half = vec2(r.width() / 2.0, r.height());
    // The block sits on OFF (left) when x = 0 and on ON (right) when x = 1.
    let block = Rect::from_min_size(r.min + vec2(half.x * x, 0.0), half).shrink(2.0);
    painter.rect_filled(block, 0.0, if enabled { DARK } else { INK_FAINT });
    for (k, label) in ["OFF", "ON"].iter().enumerate() {
        let c = pos2(r.left() + half.x * (k as f32 + 0.5), r.center().y);
        let lit = if k == 0 { 1.0 - x } else { x };
        let color = INK_SOFT.lerp_to_gamma(ON_DARK, lit);
        theme::spaced(
            painter,
            c,
            Align2::CENTER_CENTER,
            label,
            theme::hud(11.0),
            color,
            1.5,
        );
    }
    changed
}

/// A slider over a ruled track with tick marks. `log` maps the track logarithmically.
#[allow(clippy::too_many_arguments)]
pub fn slider(
    ui: &mut Ui,
    r: Rect,
    value: &mut f32,
    min: f32,
    max: f32,
    log: bool,
    step: f32,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> bool {
    let id = Id::unique(id);
    let resp = ui
        .interact(r, id, Sense::click_and_drag())
        .on_hover_cursor(CursorIcon::ResizeHorizontal);
    let to_t = |v: f32| {
        if log {
            (v.max(min).ln() - min.ln()) / (max.ln() - min.ln())
        } else {
            (v - min) / (max - min)
        }
    };
    let from_t = |t: f32| {
        let v = if log {
            (min.ln() + t * (max.ln() - min.ln())).exp()
        } else {
            min + t * (max - min)
        };
        if step > 0.0 {
            (v / step).round() * step
        } else {
            v
        }
    };
    let track = Rect::from_min_max(
        pos2(r.left() + 8.0, r.top()),
        pos2(r.right() - 8.0, r.bottom()),
    );
    let mut changed = false;
    if resp.dragged() || resp.clicked() {
        if let Some(p) = resp.interact_pointer_pos() {
            let t = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
            let v = from_t(t).clamp(min, max);
            if v != *value {
                *value = v;
                changed = true;
            }
        }
    }
    let t = to_t(*value).clamp(0.0, 1.0);
    let painter = ui.painter();
    let y = r.center().y;
    for k in 0..=20 {
        let x = track.left() + track.width() * k as f32 / 20.0;
        let h = if k % 5 == 0 { 6.0 } else { 3.0 };
        painter.line_segment(
            [pos2(x, y + 5.0), pos2(x, y + 5.0 + h)],
            Stroke::new(1.0, INK_FAINT),
        );
    }
    painter.line_segment(
        [pos2(track.left(), y), pos2(track.right(), y)],
        Stroke::new(1.0, INK_FAINT),
    );
    let tx = track.left() + track.width() * t;
    painter.line_segment([pos2(track.left(), y), pos2(tx, y)], Stroke::new(3.0, INK));
    let active = resp.dragged() || resp.hovered();
    let size = if active { 7.0 } else { 6.0 };
    let thumb = pos2(tx, y);
    painter.add(egui::Shape::convex_polygon(
        vec![
            thumb + vec2(0.0, -size),
            thumb + vec2(size, 0.0),
            thumb + vec2(0.0, size),
            thumb + vec2(-size, 0.0),
        ],
        if active { DARK } else { INK },
        Stroke::new(1.0, PAPER_HI),
    ));
    changed
}

/// A square check box with a label. Returns whether it changed.
pub fn checkbox(
    ui: &mut Ui,
    at: Pos2,
    label: &str,
    value: &mut bool,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> bool {
    let painter = ui.painter();
    let galley = painter.layout_no_wrap(label.to_string(), theme::body(12.5), INK);
    let r = Rect::from_min_size(at, vec2(22.0 + galley.size().x, 18.0));
    let resp = ui
        .interact(r, Id::unique(id), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    if resp.clicked() {
        *value = !*value;
    }
    let painter = ui.painter();
    let b = Rect::from_min_size(pos2(r.left(), r.center().y - 7.0), vec2(14.0, 14.0));
    painter.rect_filled(b, 0.0, PAPER_HI);
    painter.rect_stroke(
        b,
        0.0,
        Stroke::new(1.0, if resp.hovered() { INK } else { INK_SOFT }),
        StrokeKind::Inside,
    );
    if *value {
        painter.rect_filled(b.shrink(3.0), 0.0, DARK);
    }
    painter.galley(
        pos2(b.right() + 8.0, r.center().y - galley.size().y / 2.0),
        galley,
        INK,
    );
    resp.clicked()
}

/// A row of options; the chosen one is dark. Returns the clicked index.
pub fn segmented(
    ui: &mut Ui,
    r: Rect,
    options: &[&str],
    selected: Option<usize>,
    id: impl std::hash::Hash + std::fmt::Debug + Copy,
) -> Option<usize> {
    let n = options.len().max(1) as f32;
    let w = r.width() / n;
    let mut clicked = None;
    for (k, label) in options.iter().enumerate() {
        let cell = Rect::from_min_size(r.min + vec2(w * k as f32, 0.0), vec2(w, r.height()));
        let resp = ui
            .interact(cell, Id::unique((id, k)), Sense::click())
            .on_hover_cursor(CursorIcon::PointingHand);
        if resp.clicked() {
            clicked = Some(k);
        }
        let on = selected == Some(k);
        let painter = ui.painter();
        let fill = if on {
            DARK
        } else if resp.hovered() {
            PAPER_HI
        } else {
            Color32::TRANSPARENT
        };
        painter.rect_filled(cell.shrink(1.0), 0.0, fill);
        painter.rect_stroke(cell, 0.0, Stroke::new(1.0, INK_FAINT), StrokeKind::Inside);
        let color = if on { ON_DARK } else { INK };
        theme::spaced(
            painter,
            cell.center(),
            Align2::CENTER_CENTER,
            label,
            theme::hud(11.5),
            color,
            1.2,
        );
    }
    clicked
}

/// A rectangular button with a spaced label.
pub fn button(
    ui: &mut Ui,
    r: Rect,
    label: &str,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> bool {
    let resp = ui
        .interact(r, Id::unique(id), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    let hover = resp.hovered();
    painter.rect_filled(r, 0.0, if hover { DARK } else { PAPER_HI });
    painter.rect_stroke(r, 0.0, Stroke::new(1.0, INK), StrokeKind::Inside);
    theme::spaced(
        painter,
        r.center(),
        Align2::CENTER_CENTER,
        label,
        theme::hud(11.5),
        if hover { ON_DARK } else { INK },
        1.5,
    );
    resp.clicked()
}

/// A title-bar button (icon only).
pub fn window_button(ui: &mut Ui, r: Rect, icon: char, danger: bool, id: &str) -> bool {
    let resp = ui
        .interact(r, Id::unique(("window-button", id)), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);
    let painter = ui.painter();
    if resp.hovered() {
        painter.rect_filled(r, 0.0, if danger { theme::BAD } else { DARK });
    }
    let color = if resp.hovered() {
        ON_DARK
    } else {
        ON_DARK.gamma_multiply(0.85)
    };
    painter.text(
        r.center(),
        Align2::CENTER_CENTER,
        icon,
        theme::icon_font(11.0),
        color,
    );
    resp.clicked()
}
