//! Compact visual system for the redesigned NotOffice shell.
//!
//! Everything here is painted with egui's own shapes: no image assets, no
//! extra crates, no per-frame allocations beyond the handful of small shape
//! vectors egui tessellates anyway. The goal is a quiet, paper-like workspace
//! where chrome takes as little room as possible and the document stays the
//! brightest thing on screen.

use eframe::egui::{
    self, Color32, FontFamily, FontId, Margin, Response, RichText, Rounding, Sense, Stroke,
    TextStyle, Ui, Vec2, Widget,
};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Palette — warm paper neutrals with a single deep-pine accent.
// ---------------------------------------------------------------------------

/// Window / canvas behind the page.
pub const CANVAS: Color32 = Color32::from_rgb(0xEE, 0xEB, 0xE4);
/// Bars and menus.
pub const CHROME: Color32 = Color32::from_rgb(0xFA, 0xF8, 0xF4);
/// Cards, menus, dialogs.
pub const CARD: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
/// The paper itself.
pub const PAGE: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);

pub const INK: Color32 = Color32::from_rgb(0x25, 0x23, 0x1F);
pub const MUTED: Color32 = Color32::from_rgb(0x6D, 0x67, 0x5D);
pub const FAINT: Color32 = Color32::from_rgb(0x97, 0x91, 0x85);

pub const LINE: Color32 = Color32::from_rgb(0xE1, 0xDC, 0xD1);
pub const LINE_SOFT: Color32 = Color32::from_rgb(0xE9, 0xE5, 0xDC);

pub const ACCENT: Color32 = Color32::from_rgb(0x17, 0x6B, 0x5A);
pub const ACCENT_HOVER: Color32 = Color32::from_rgb(0x11, 0x57, 0x49);
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0xE2, 0xEF, 0xEA);
pub const ACCENT_INK: Color32 = Color32::from_rgb(0x0E, 0x4A, 0x3F);

pub const WARN: Color32 = Color32::from_rgb(0xA8, 0x5A, 0x0B);
pub const WARN_SOFT: Color32 = Color32::from_rgb(0xFB, 0xF0, 0xDE);
pub const DANGER: Color32 = Color32::from_rgb(0xA8, 0x28, 0x1E);

pub const HOVER: Color32 = Color32::from_rgb(0xF0, 0xEC, 0xE4);
pub const PRESSED: Color32 = Color32::from_rgb(0xE8, 0xE3, 0xD9);

/// Height of the command bar and document strip, in points.
pub const BAR_HEIGHT: f32 = 44.0;
pub const STRIP_HEIGHT: f32 = 32.0;
pub const STATUS_HEIGHT: f32 = 24.0;

pub fn semibold_family() -> FontFamily {
    FontFamily::Name(Arc::from("semibold"))
}

fn semibold_flag() -> egui::Id {
    egui::Id::new("liteoffice.semibold-ready")
}

/// The custom family only becomes available one frame after
/// `install_windows_fonts`, because egui applies `set_fonts` at the start of
/// the next frame. Until then the shell falls back to the regular proportional
/// face; otherwise the very first frame (and headless tests) would panic on an
/// unbound family.
pub fn fonts_ready(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(semibold_flag()).unwrap_or(false))
}

pub fn mark_fonts_ready(ctx: &egui::Context) {
    ctx.data_mut(|data| data.insert_temp(semibold_flag(), true));
}

pub fn heading_family(ctx: &egui::Context) -> FontFamily {
    if fonts_ready(ctx) {
        semibold_family()
    } else {
        FontFamily::Proportional
    }
}

pub fn heading_font(ctx: &egui::Context, size: f32) -> FontId {
    FontId::new(size, heading_family(ctx))
}

// ---------------------------------------------------------------------------
// Style installation
// ---------------------------------------------------------------------------

/// Apply the compact light theme. Called once per window, before the first
/// frame is laid out.
pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let mut visuals = egui::Visuals::light();
    visuals.dark_mode = false;

    visuals.override_text_color = None;
    visuals.panel_fill = CANVAS;
    visuals.window_fill = CARD;
    visuals.window_stroke = Stroke::new(1.0_f32, LINE);
    visuals.window_rounding = Rounding::same(10.0);
    visuals.window_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 8.0),
        blur: 26.0,
        spread: 0.0,
        color: Color32::from_black_alpha(38),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: egui::vec2(0.0, 4.0),
        blur: 14.0,
        spread: 0.0,
        color: Color32::from_black_alpha(30),
    };
    visuals.menu_rounding = Rounding::same(8.0);
    visuals.extreme_bg_color = Color32::WHITE;
    visuals.faint_bg_color = Color32::from_rgb(0xF7, 0xF4, 0xEE);
    visuals.code_bg_color = Color32::from_rgb(0xF3, 0xEF, 0xE8);
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARN;
    visuals.error_fg_color = DANGER;
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(0xD9, 0x9A, 0x3F, 104);
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.text_cursor.stroke = Stroke::new(1.6_f32, INK);
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.striped = false;
    visuals.indent_has_left_vline = false;
    visuals.window_highlight_topmost = false;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = CARD;
    widgets.noninteractive.weak_bg_fill = CARD;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, LINE);
    widgets.noninteractive.rounding = Rounding::same(6.0);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, INK);

    widgets.inactive.bg_fill = Color32::from_rgb(0xF4, 0xF1, 0xEA);
    widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    widgets.inactive.bg_stroke = Stroke::new(1.0_f32, LINE_SOFT);
    widgets.inactive.rounding = Rounding::same(6.0);
    widgets.inactive.fg_stroke = Stroke::new(1.0_f32, INK);

    widgets.hovered.bg_fill = Color32::from_rgb(0xF1, 0xED, 0xE5);
    widgets.hovered.weak_bg_fill = HOVER;
    widgets.hovered.bg_stroke = Stroke::new(1.0_f32, LINE);
    widgets.hovered.rounding = Rounding::same(6.0);
    widgets.hovered.fg_stroke = Stroke::new(1.0_f32, INK);

    widgets.active.bg_fill = ACCENT_SOFT;
    widgets.active.weak_bg_fill = PRESSED;
    widgets.active.bg_stroke = Stroke::new(1.0_f32, ACCENT);
    widgets.active.rounding = Rounding::same(6.0);
    widgets.active.fg_stroke = Stroke::new(1.0_f32, INK);

    widgets.open.bg_fill = ACCENT_SOFT;
    widgets.open.weak_bg_fill = ACCENT_SOFT;
    widgets.open.bg_stroke = Stroke::new(1.0_f32, LINE);
    widgets.open.rounding = Rounding::same(6.0);
    widgets.open.fg_stroke = Stroke::new(1.0_f32, INK);

    style.visuals = visuals;

    style.text_styles = [
        (
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        ),
        (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
        (
            TextStyle::Button,
            FontId::new(13.0, FontFamily::Proportional),
        ),
        (
            TextStyle::Monospace,
            FontId::new(12.5, FontFamily::Monospace),
        ),
        (
            TextStyle::Heading,
            FontId::new(15.0, FontFamily::Proportional),
        ),
    ]
    .into();

    let spacing = &mut style.spacing;
    spacing.item_spacing = Vec2::new(6.0, 4.0);
    spacing.button_padding = Vec2::new(10.0, 4.0);
    spacing.interact_size = Vec2::new(28.0, 24.0);
    spacing.window_margin = Margin::same(14.0);
    spacing.menu_margin = Margin::same(6.0);
    spacing.indent = 14.0;
    spacing.slider_width = 110.0;
    spacing.combo_width = 104.0;
    spacing.text_edit_width = 220.0;
    spacing.tooltip_width = 320.0;
    spacing.menu_width = 320.0;
    spacing.combo_height = 340.0;
    spacing.scroll = egui::style::ScrollStyle::thin();

    ctx.set_style(style);
}

// ---------------------------------------------------------------------------
// Small building blocks
// ---------------------------------------------------------------------------

pub fn hairline(ui: &mut Ui) {
    let width = 1.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, LINE);
}

/// A small rounded tag, e.g. `DOCX` or `RECOVERED`.
pub fn chip(ui: &mut Ui, text: &str, fg: Color32, bg: Color32) -> Response {
    egui::Frame::none()
        .fill(bg)
        .rounding(Rounding::same(4.0))
        .inner_margin(Margin::symmetric(5.0, 1.0))
        .show(ui, |ui| {
            ui.label(
                RichText::new(text)
                    .size(10.0)
                    .color(fg)
                    .family(heading_family(ui.ctx())),
            );
        })
        .response
}

pub fn dot(ui: &mut Ui, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 2.6, color);
    response
}

/// The product mark: a solid rounded tile with a sheet of paper.
pub fn brand_mark(ui: &mut Ui, size: f32, filled: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let rounding = Rounding::same(size * 0.26);
    if filled {
        ui.painter().rect_filled(rect, rounding, ACCENT);
    } else {
        ui.painter()
            .rect_stroke(rect, rounding, Stroke::new(1.2_f32, ACCENT));
    }
    let paper = egui::Rect::from_center_size(rect.center(), Vec2::new(size * 0.44, size * 0.58));
    let paper_color = if filled { Color32::WHITE } else { ACCENT };
    ui.painter()
        .rect_filled(paper, Rounding::same(size * 0.06), paper_color);
    let ink = if filled { ACCENT } else { Color32::WHITE };
    let stroke = Stroke::new((size * 0.055).max(0.8), ink);
    for factor in [0.32, 0.52, 0.72] {
        let y = paper.top() + paper.height() * factor;
        let inset = paper.width() * 0.18;
        ui.painter().line_segment(
            [
                egui::pos2(paper.left() + inset, y),
                egui::pos2(paper.right() - inset, y),
            ],
            stroke,
        );
    }
}

pub fn brand(ui: &mut Ui) {
    brand_mark(ui, 20.0, true);
    ui.add_space(5.0);
    ui.label(
        RichText::new("NotOffice")
            .size(13.0)
            .family(heading_family(ui.ctx()))
            .color(INK),
    );
}

// ---------------------------------------------------------------------------
// Icons, drawn from primitives
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Folder,
    Save,
    Undo,
    Redo,
    Search,
    Printer,
    Minus,
    Plus,
    Page,
    Settings,
    Close,
    ChevronLeft,
    ChevronRight,
    Lock,
}

fn arc_points(center: egui::Pos2, radius: f32, start_deg: f32, sweep_deg: f32) -> Vec<egui::Pos2> {
    let steps = 22;
    (0..=steps)
        .map(|i| {
            let angle = (start_deg + sweep_deg * (i as f32 / steps as f32)).to_radians();
            egui::pos2(
                center.x + radius * angle.cos(),
                center.y + radius * angle.sin(),
            )
        })
        .collect()
}

fn arrow_head(painter: &egui::Painter, tip: egui::Pos2, from: egui::Pos2, color: Color32) {
    let direction = (tip - from).normalized();
    let perpendicular = egui::vec2(-direction.y, direction.x);
    let base = tip - direction * 3.4;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, base + perpendicular * 2.2, base - perpendicular * 2.2],
        color,
        Stroke::NONE,
    ));
}

fn seg(painter: &egui::Painter, points: [egui::Pos2; 2], stroke: Stroke) {
    painter.line_segment(points, stroke);
}

fn outline(painter: &egui::Painter, rect: egui::Rect, rounding: Rounding, stroke: Stroke) {
    painter.rect_stroke(rect, rounding, stroke);
}

fn disc(painter: &egui::Painter, center: egui::Pos2, radius: f32, color: Color32) {
    painter.circle_filled(center, radius, color);
}

fn ring(painter: &egui::Painter, center: egui::Pos2, radius: f32, stroke: Stroke) {
    painter.circle_stroke(center, radius, stroke);
}

pub fn paint_icon(painter: &egui::Painter, rect: egui::Rect, icon: Icon, color: Color32) {
    let stroke = Stroke::new(1.8_f32, color);
    let scale = rect.width() / 16.0;
    let point = |x: f32, y: f32| egui::pos2(rect.left() + x * scale, rect.top() + y * scale);
    let line = |points: &[egui::Pos2]| {
        painter.add(egui::Shape::line(points.to_vec(), stroke));
    };
    match icon {
        Icon::Folder => {
            painter.add(egui::Shape::closed_line(
                vec![
                    point(1.8, 5.0),
                    point(6.0, 5.0),
                    point(7.2, 6.4),
                    point(14.2, 6.4),
                    point(14.2, 13.0),
                    point(1.8, 13.0),
                ],
                stroke,
            ));
            seg(
                painter,
                [point(2.2, 7.6), point(13.8, 7.6)],
                Stroke::new(1.0_f32, color),
            );
        }
        Icon::Save => {
            outline(
                painter,
                egui::Rect::from_min_max(point(2.4, 2.2), point(13.6, 13.6)),
                Rounding::same(1.6),
                stroke,
            );
            // Filled shutter and label read as a floppy disk at 16 px.
            painter.rect_filled(
                egui::Rect::from_min_max(point(5.4, 2.2), point(10.6, 6.2)),
                Rounding::ZERO,
                color,
            );
            outline(
                painter,
                egui::Rect::from_min_max(point(4.8, 9.8), point(11.2, 13.6)),
                Rounding::ZERO,
                Stroke::new(1.2_f32, color),
            );
        }
        Icon::Undo => {
            let points = arc_points(point(8.0, 8.3), 5.0 * scale, 25.0, -300.0);
            let end = points[points.len() - 1];
            let previous = points[points.len() - 2];
            painter.add(egui::Shape::line(points, stroke));
            arrow_head(painter, end, previous, color);
        }
        Icon::Redo => {
            let points = arc_points(point(8.0, 8.3), 5.0 * scale, 155.0, 300.0);
            let end = points[points.len() - 1];
            let previous = points[points.len() - 2];
            painter.add(egui::Shape::line(points, stroke));
            arrow_head(painter, end, previous, color);
        }
        Icon::Search => {
            ring(painter, point(7.0, 7.0), 4.1 * scale, stroke);
            seg(painter, [point(10.1, 10.1), point(13.8, 13.8)], stroke);
        }
        Icon::Printer => {
            outline(
                painter,
                egui::Rect::from_min_max(point(4.8, 1.8), point(11.2, 5.8)),
                Rounding::same(1.0),
                Stroke::new(1.25_f32, color),
            );
            outline(
                painter,
                egui::Rect::from_min_max(point(2.2, 5.8), point(13.8, 10.6)),
                Rounding::same(1.4),
                stroke,
            );
            outline(
                painter,
                egui::Rect::from_min_max(point(4.8, 10.6), point(11.2, 13.8)),
                Rounding::ZERO,
                Stroke::new(1.25_f32, color),
            );
            disc(painter, point(11.2, 7.6), 0.75 * scale, color);
        }
        Icon::Minus => seg(painter, [point(4.2, 8.0), point(11.8, 8.0)], stroke),
        Icon::Plus => {
            seg(painter, [point(4.2, 8.0), point(11.8, 8.0)], stroke);
            seg(painter, [point(8.0, 4.2), point(8.0, 11.8)], stroke);
        }
        Icon::Page => {
            painter.add(egui::Shape::closed_line(
                vec![
                    point(3.4, 1.8),
                    point(9.6, 1.8),
                    point(12.6, 4.8),
                    point(12.6, 14.2),
                    point(3.4, 14.2),
                ],
                Stroke::new(1.3_f32, color),
            ));
            seg(
                painter,
                [point(9.6, 1.8), point(9.6, 4.8)],
                Stroke::new(1.1_f32, color),
            );
            seg(
                painter,
                [point(9.6, 4.8), point(12.6, 4.8)],
                Stroke::new(1.1_f32, color),
            );
        }
        Icon::Settings => {
            for (y, knob) in [(5.4, 6.4), (10.6, 9.8)] {
                seg(
                    painter,
                    [point(2.6, y), point(13.4, y)],
                    Stroke::new(1.25_f32, color),
                );
                disc(painter, point(knob, y), 1.45 * scale, CHROME);
                ring(
                    painter,
                    point(knob, y),
                    1.45 * scale,
                    Stroke::new(1.25_f32, color),
                );
            }
        }
        Icon::Close => {
            seg(painter, [point(4.4, 4.4), point(11.6, 11.6)], stroke);
            seg(painter, [point(11.6, 4.4), point(4.4, 11.6)], stroke);
        }
        Icon::ChevronLeft => line(&[point(9.8, 4.6), point(6.4, 8.0), point(9.8, 11.4)]),
        Icon::ChevronRight => line(&[point(6.2, 4.6), point(9.6, 8.0), point(6.2, 11.4)]),
        Icon::Lock => {
            outline(
                painter,
                egui::Rect::from_min_max(point(3.6, 7.4), point(12.4, 13.4)),
                Rounding::same(1.4),
                stroke,
            );
            let shackle = arc_points(point(8.0, 7.4), 2.9 * scale, 180.0, 180.0);
            painter.add(egui::Shape::line(shackle, stroke));
            disc(painter, point(8.0, 10.2), 0.9 * scale, color);
        }
    }
}

// ---------------------------------------------------------------------------
// Compact icon button
// ---------------------------------------------------------------------------

pub struct IconButton<'a> {
    icon: Icon,
    tooltip: &'a str,
    selected: bool,
    size: Vec2,
}

impl<'a> IconButton<'a> {
    pub fn new(icon: Icon, tooltip: &'a str) -> Self {
        Self {
            icon,
            tooltip,
            selected: false,
            size: Vec2::new(30.0, 26.0),
        }
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn small(mut self) -> Self {
        self.size = Vec2::new(24.0, 22.0);
        self
    }
}

impl Widget for IconButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, response) = ui.allocate_exact_size(self.size, Sense::click());
        let enabled = ui.is_enabled();
        let painter = ui.painter();
        let rounding = Rounding::same(6.0);
        let hovered = response.hovered() && enabled;

        if self.selected && enabled {
            painter.rect_filled(rect, rounding, ACCENT_SOFT);
        } else if response.is_pointer_button_down_on() && enabled {
            painter.rect_filled(rect, rounding, PRESSED);
        } else if hovered {
            painter.rect_filled(rect, rounding, HOVER);
        }

        let color = if !enabled {
            Color32::from_rgba_unmultiplied(0x6D, 0x67, 0x5D, 205)
        } else if self.selected {
            ACCENT
        } else {
            INK
        };
        paint_icon(
            painter,
            egui::Rect::from_center_size(rect.center(), Vec2::splat(18.0)),
            self.icon,
            color,
        );

        if enabled {
            response.on_hover_text(self.tooltip)
        } else {
            response.on_disabled_hover_text(self.tooltip)
        }
    }
}

/// The one accented action on a screen (Open, Restore, Print…).
pub struct PrimaryButton<'a> {
    text: &'a str,
}

impl<'a> PrimaryButton<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text }
    }
}

impl Widget for PrimaryButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let font = heading_font(ui.ctx(), 12.5);
        let galley = ui
            .painter()
            .layout_no_wrap(self.text.to_owned(), font, Color32::WHITE);
        let size = Vec2::new(galley.size().x + 24.0, 27.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let enabled = ui.is_enabled();
        let fill = if !enabled {
            Color32::from_rgb(0xB6, 0xC8, 0xC1)
        } else if response.is_pointer_button_down_on() {
            ACCENT_HOVER
        } else if response.hovered() {
            Color32::from_rgb(0x13, 0x60, 0x51)
        } else {
            ACCENT
        };
        ui.painter().rect_filled(rect, Rounding::same(6.0), fill);
        let origin = rect.center() - galley.size() * 0.5;
        ui.painter().galley(origin, galley, Color32::WHITE);
        if enabled {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

// ---------------------------------------------------------------------------
// Panels and surfaces
// ---------------------------------------------------------------------------

pub fn bar_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(CHROME)
        .inner_margin(Margin::symmetric(10.0, 0.0))
}

pub fn strip_frame() -> egui::Frame {
    egui::Frame::none().fill(CHROME).inner_margin(Margin {
        left: 12.0,
        right: 12.0,
        top: 4.0,
        bottom: 2.0,
    })
}

pub fn status_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(Color32::from_rgb(0xF5, 0xF2, 0xEC))
        .inner_margin(Margin::symmetric(12.0, 0.0))
}

/// A 1px rule at the bottom edge of the current panel.
pub fn bottom_rule(ui: &Ui) {
    let rect = ui.max_rect();
    ui.painter().line_segment(
        [
            egui::pos2(rect.left(), rect.bottom()),
            egui::pos2(rect.right(), rect.bottom()),
        ],
        Stroke::new(1.0_f32, LINE),
    );
}

/// A 1px rule at the top edge of the current panel.
pub fn top_rule(ui: &Ui) {
    let rect = ui.max_rect();
    ui.painter().line_segment(
        [
            egui::pos2(rect.left(), rect.top()),
            egui::pos2(rect.right(), rect.top()),
        ],
        Stroke::new(1.0_f32, LINE),
    );
}
