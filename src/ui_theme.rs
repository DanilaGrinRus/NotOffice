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
use liteoffice::theme::Theme;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Themes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub struct Palette {
    pub canvas: Color32,
    pub chrome: Color32,
    pub card: Color32,
    pub page: Color32,
    pub ink: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub line: Color32,
    pub line_soft: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub accent_ink: Color32,
    pub on_accent: Color32,
    pub warn: Color32,
    pub warn_soft: Color32,
    pub danger: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub field_bg: Color32,
    pub field_ink: Color32,
    pub text_bg: Color32,
    pub text_ink: Color32,
    pub disabled: Color32,
    pub selection: Color32,
    pub search_hit: Color32,
    pub search_active: Color32,
    pub link: Color32,
    pub soft_bg: Color32,
}

pub fn palette_for(theme: Theme) -> Palette {
    match theme {
        Theme::Light => Palette {
            canvas: Color32::from_rgb(0xEE, 0xEB, 0xE4),
            chrome: Color32::from_rgb(0xFA, 0xF8, 0xF4),
            card: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            page: Color32::WHITE,
            ink: Color32::from_rgb(0x25, 0x23, 0x1F),
            muted: Color32::from_rgb(0x6D, 0x67, 0x5D),
            faint: Color32::from_rgb(0x7A, 0x74, 0x6A),
            line: Color32::from_rgb(0xE1, 0xDC, 0xD1),
            line_soft: Color32::from_rgb(0xE9, 0xE5, 0xDC),
            accent: Color32::from_rgb(0x17, 0x6B, 0x5A),
            accent_hover: Color32::from_rgb(0x11, 0x57, 0x49),
            accent_soft: Color32::from_rgb(0xE2, 0xEF, 0xEA),
            accent_ink: Color32::from_rgb(0x0E, 0x4A, 0x3F),
            on_accent: Color32::WHITE,
            warn: Color32::from_rgb(0xA8, 0x5A, 0x0B),
            warn_soft: Color32::from_rgb(0xFB, 0xF0, 0xDE),
            danger: Color32::from_rgb(0xA8, 0x28, 0x1E),
            hover: Color32::from_rgb(0xF0, 0xEC, 0xE4),
            pressed: Color32::from_rgb(0xE8, 0xE3, 0xD9),
            field_bg: Color32::WHITE,
            field_ink: Color32::from_rgb(0x25, 0x23, 0x1F),
            text_bg: Color32::WHITE,
            text_ink: Color32::from_rgb(0x25, 0x23, 0x1F),
            disabled: Color32::from_rgba_unmultiplied(0x5F, 0x5A, 0x52, 230),
            selection: Color32::from_rgb(0xD9, 0x77, 0x06),
            search_hit: Color32::from_rgb(0xFF, 0xF4, 0xB9),
            search_active: Color32::from_rgb(0xFF, 0xD6, 0x78),
            link: Color32::from_rgb(0x17, 0x6B, 0x5A),
            soft_bg: Color32::from_rgb(0xF1, 0xED, 0xE5),
        },
        Theme::DarkBlue => Palette {
            canvas: Color32::from_rgb(0x15, 0x22, 0x38),
            chrome: Color32::from_rgb(0x1B, 0x2C, 0x47),
            card: Color32::from_rgb(0x1F, 0x33, 0x50),
            page: Color32::WHITE,
            ink: Color32::from_rgb(0xE6, 0xED, 0xF7),
            muted: Color32::from_rgb(0xBF, 0xCC, 0xDE),
            faint: Color32::from_rgb(0x9D, 0xAF, 0xC7),
            line: Color32::from_rgb(0x31, 0x48, 0x66),
            line_soft: Color32::from_rgb(0x26, 0x3A, 0x55),
            accent: Color32::from_rgb(0x6F, 0xB3, 0xFF),
            accent_hover: Color32::from_rgb(0x8D, 0xC6, 0xFF),
            accent_soft: Color32::from_rgb(0x24, 0x3F, 0x60),
            accent_ink: Color32::from_rgb(0xD7, 0xE9, 0xFF),
            on_accent: Color32::from_rgb(0x0D, 0x1B, 0x2A),
            warn: Color32::from_rgb(0xFF, 0xC4, 0x6B),
            warn_soft: Color32::from_rgb(0x40, 0x32, 0x1D),
            danger: Color32::from_rgb(0xFF, 0x8A, 0x80),
            hover: Color32::from_rgb(0x24, 0x3B, 0x59),
            pressed: Color32::from_rgb(0x2B, 0x46, 0x67),
            field_bg: Color32::from_rgb(0x1B, 0x2B, 0x45),
            field_ink: Color32::from_rgb(0xE6, 0xED, 0xF7),
            text_bg: Color32::from_rgb(0x1B, 0x2B, 0x45),
            text_ink: Color32::from_rgb(0xE6, 0xED, 0xF7),
            disabled: Color32::from_rgba_unmultiplied(0xC7, 0xD3, 0xE3, 235),
            selection: Color32::from_rgb(0xD9, 0x77, 0x06),
            search_hit: Color32::from_rgb(0xFF, 0xF4, 0xB9),
            search_active: Color32::from_rgb(0xFF, 0xD6, 0x78),
            link: Color32::from_rgb(0x8D, 0xC6, 0xFF),
            soft_bg: Color32::from_rgb(0x24, 0x3B, 0x59),
        },
        Theme::Parchment => Palette {
            canvas: Color32::from_rgb(0xF3, 0xE8, 0xD1),
            chrome: Color32::from_rgb(0xFA, 0xF1, 0xDE),
            card: Color32::from_rgb(0xFF, 0xF9, 0xEC),
            page: Color32::WHITE,
            ink: Color32::from_rgb(0x40, 0x36, 0x2B),
            muted: Color32::from_rgb(0x6B, 0x5B, 0x49),
            faint: Color32::from_rgb(0x80, 0x6E, 0x59),
            line: Color32::from_rgb(0xD9, 0xC8, 0xA9),
            line_soft: Color32::from_rgb(0xE6, 0xD7, 0xBC),
            accent: Color32::from_rgb(0x7A, 0x4E, 0x2D),
            accent_hover: Color32::from_rgb(0x63, 0x3D, 0x20),
            accent_soft: Color32::from_rgb(0xEA, 0xD7, 0xBC),
            accent_ink: Color32::from_rgb(0x4B, 0x2E, 0x18),
            on_accent: Color32::WHITE,
            warn: Color32::from_rgb(0x8A, 0x4B, 0x0F),
            warn_soft: Color32::from_rgb(0xF4, 0xDF, 0xC3),
            danger: Color32::from_rgb(0x8F, 0x2D, 0x23),
            hover: Color32::from_rgb(0xED, 0xE0, 0xC9),
            pressed: Color32::from_rgb(0xE3, 0xD2, 0xB7),
            field_bg: Color32::from_rgb(0xFF, 0xF9, 0xEC),
            field_ink: Color32::from_rgb(0x40, 0x36, 0x2B),
            text_bg: Color32::from_rgb(0xFF, 0xF9, 0xEC),
            text_ink: Color32::from_rgb(0x40, 0x36, 0x2B),
            disabled: Color32::from_rgba_unmultiplied(0x5A, 0x4A, 0x3A, 230),
            selection: Color32::from_rgb(0xD9, 0x77, 0x06),
            search_hit: Color32::from_rgb(0xFF, 0xF4, 0xB9),
            search_active: Color32::from_rgb(0xFF, 0xD6, 0x78),
            link: Color32::from_rgb(0x7A, 0x4E, 0x2D),
            soft_bg: Color32::from_rgb(0xED, 0xE0, 0xC9),
        },
        Theme::Sage => Palette {
            canvas: Color32::from_rgb(0xE5, 0xEC, 0xE0),
            chrome: Color32::from_rgb(0xF0, 0xF5, 0xEC),
            card: Color32::from_rgb(0xFA, 0xFC, 0xF7),
            page: Color32::WHITE,
            ink: Color32::from_rgb(0x28, 0x38, 0x2C),
            muted: Color32::from_rgb(0x53, 0x64, 0x54),
            faint: Color32::from_rgb(0x6B, 0x7B, 0x6A),
            line: Color32::from_rgb(0xC9, 0xD6, 0xC5),
            line_soft: Color32::from_rgb(0xD8, 0xE2, 0xD4),
            accent: Color32::from_rgb(0x3E, 0x6B, 0x4F),
            accent_hover: Color32::from_rgb(0x2F, 0x54, 0x3D),
            accent_soft: Color32::from_rgb(0xD8, 0xE8, 0xDB),
            accent_ink: Color32::from_rgb(0x24, 0x42, 0x2F),
            on_accent: Color32::WHITE,
            warn: Color32::from_rgb(0x8A, 0x5A, 0x12),
            warn_soft: Color32::from_rgb(0xF1, 0xE4, 0xC9),
            danger: Color32::from_rgb(0x8C, 0x2F, 0x2A),
            hover: Color32::from_rgb(0xDC, 0xE8, 0xD7),
            pressed: Color32::from_rgb(0xCF, 0xDD, 0xCA),
            field_bg: Color32::from_rgb(0xFA, 0xFC, 0xF7),
            field_ink: Color32::from_rgb(0x28, 0x38, 0x2C),
            text_bg: Color32::from_rgb(0xFA, 0xFC, 0xF7),
            text_ink: Color32::from_rgb(0x28, 0x38, 0x2C),
            disabled: Color32::from_rgba_unmultiplied(0x42, 0x54, 0x43, 230),
            selection: Color32::from_rgb(0xD9, 0x77, 0x06),
            search_hit: Color32::from_rgb(0xFF, 0xF4, 0xB9),
            search_active: Color32::from_rgb(0xFF, 0xD6, 0x78),
            link: Color32::from_rgb(0x3E, 0x6B, 0x4F),
            soft_bg: Color32::from_rgb(0xDC, 0xE8, 0xD7),
        },
        Theme::WarmGraphite => Palette {
            canvas: Color32::from_rgb(0x29, 0x26, 0x24),
            chrome: Color32::from_rgb(0x30, 0x2C, 0x29),
            card: Color32::from_rgb(0x38, 0x33, 0x30),
            page: Color32::WHITE,
            ink: Color32::from_rgb(0xE8, 0xE0, 0xD5),
            muted: Color32::from_rgb(0xC4, 0xB9, 0xAD),
            faint: Color32::from_rgb(0xA9, 0x9D, 0x90),
            line: Color32::from_rgb(0x4A, 0x42, 0x3D),
            line_soft: Color32::from_rgb(0x3D, 0x37, 0x33),
            accent: Color32::from_rgb(0xE0, 0xA3, 0x6B),
            accent_hover: Color32::from_rgb(0xF0, 0xB7, 0x7E),
            accent_soft: Color32::from_rgb(0x4A, 0x38, 0x28),
            accent_ink: Color32::from_rgb(0xFF, 0xD7, 0xAE),
            on_accent: Color32::from_rgb(0x21, 0x1A, 0x12),
            warn: Color32::from_rgb(0xFF, 0xB8, 0x6B),
            warn_soft: Color32::from_rgb(0x4A, 0x35, 0x20),
            danger: Color32::from_rgb(0xFF, 0x8F, 0x84),
            hover: Color32::from_rgb(0x3D, 0x38, 0x34),
            pressed: Color32::from_rgb(0x49, 0x42, 0x3D),
            field_bg: Color32::from_rgb(0x33, 0x2F, 0x2C),
            field_ink: Color32::from_rgb(0xE8, 0xE0, 0xD5),
            text_bg: Color32::from_rgb(0x33, 0x2F, 0x2C),
            text_ink: Color32::from_rgb(0xE8, 0xE0, 0xD5),
            disabled: Color32::from_rgba_unmultiplied(0xD8, 0xCC, 0xC0, 235),
            selection: Color32::from_rgb(0xD9, 0x77, 0x06),
            search_hit: Color32::from_rgb(0xFF, 0xF4, 0xB9),
            search_active: Color32::from_rgb(0xFF, 0xD6, 0x78),
            link: Color32::from_rgb(0xE0, 0xA3, 0x6B),
            soft_bg: Color32::from_rgb(0x3D, 0x38, 0x34),
        },
        Theme::AmberTerminal => Palette {
            canvas: Color32::from_rgb(0x21, 0x1A, 0x12),
            chrome: Color32::from_rgb(0x2A, 0x21, 0x17),
            card: Color32::from_rgb(0x33, 0x28, 0x1B),
            page: Color32::WHITE,
            ink: Color32::from_rgb(0xE7, 0xBD, 0x78),
            muted: Color32::from_rgb(0xC9, 0xA6, 0x6B),
            faint: Color32::from_rgb(0xA9, 0x8A, 0x58),
            line: Color32::from_rgb(0x4A, 0x3A, 0x25),
            line_soft: Color32::from_rgb(0x3C, 0x2F, 0x1E),
            accent: Color32::from_rgb(0xE7, 0xBD, 0x78),
            accent_hover: Color32::from_rgb(0xF3, 0xCE, 0x8E),
            accent_soft: Color32::from_rgb(0x4A, 0x38, 0x20),
            accent_ink: Color32::from_rgb(0xFF, 0xE0, 0xA8),
            on_accent: Color32::from_rgb(0x21, 0x1A, 0x12),
            warn: Color32::from_rgb(0xFF, 0xC4, 0x6B),
            warn_soft: Color32::from_rgb(0x3F, 0x2E, 0x18),
            danger: Color32::from_rgb(0xFF, 0x8A, 0x80),
            hover: Color32::from_rgb(0x3A, 0x2D, 0x1D),
            pressed: Color32::from_rgb(0x46, 0x37, 0x22),
            field_bg: Color32::from_rgb(0x2A, 0x21, 0x17),
            field_ink: Color32::from_rgb(0xE7, 0xBD, 0x78),
            text_bg: Color32::from_rgb(0x2A, 0x21, 0x17),
            text_ink: Color32::from_rgb(0xE7, 0xBD, 0x78),
            disabled: Color32::from_rgba_unmultiplied(0xD9, 0xB7, 0x7D, 235),
            selection: Color32::from_rgb(0xD9, 0x77, 0x06),
            search_hit: Color32::from_rgb(0xFF, 0xF4, 0xB9),
            search_active: Color32::from_rgb(0xFF, 0xD6, 0x78),
            link: Color32::from_rgb(0xE7, 0xBD, 0x78),
            soft_bg: Color32::from_rgb(0x3A, 0x2D, 0x1D),
        },
    }
}

thread_local! {
    static ACTIVE_THEME: std::cell::Cell<Theme> = const { std::cell::Cell::new(Theme::Light) };
}

pub fn activate(theme: Theme) {
    ACTIVE_THEME.with(|active| active.set(theme));
}

pub fn current_theme() -> Theme {
    ACTIVE_THEME.with(std::cell::Cell::get)
}

fn palette() -> Palette {
    palette_for(current_theme())
}

pub fn chrome() -> Color32 {
    palette().chrome
}
pub fn card() -> Color32 {
    palette().card
}
pub fn page() -> Color32 {
    palette().page
}
pub fn ink() -> Color32 {
    palette().ink
}
pub fn muted() -> Color32 {
    palette().muted
}
pub fn faint() -> Color32 {
    palette().faint
}
pub fn line() -> Color32 {
    palette().line
}
pub fn accent() -> Color32 {
    palette().accent
}
pub fn accent_hover() -> Color32 {
    palette().accent_hover
}
pub fn accent_soft() -> Color32 {
    palette().accent_soft
}
pub fn accent_ink() -> Color32 {
    palette().accent_ink
}
pub fn warn() -> Color32 {
    palette().warn
}
pub fn warn_soft() -> Color32 {
    palette().warn_soft
}
pub fn danger() -> Color32 {
    palette().danger
}
pub fn hover() -> Color32 {
    palette().hover
}
pub fn pressed() -> Color32 {
    palette().pressed
}
pub fn text_bg() -> Color32 {
    palette().text_bg
}
pub fn text_ink() -> Color32 {
    palette().text_ink
}
pub fn disabled() -> Color32 {
    palette().disabled
}
pub fn selection() -> Color32 {
    palette().selection
}
pub fn search_hit() -> Color32 {
    palette().search_hit
}
pub fn search_active() -> Color32 {
    palette().search_active
}
pub fn link() -> Color32 {
    palette().link
}
pub fn soft_bg() -> Color32 {
    palette().soft_bg
}

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

/// Apply the selected palette to widgets and install it for this window.
/// Called once when the window is created and again when the theme changes.
pub fn install(ctx: &egui::Context, theme: Theme) {
    activate(theme);
    let p = palette_for(theme);
    let mut style = (*ctx.style()).clone();
    let dark = !matches!(theme, Theme::Light | Theme::Parchment | Theme::Sage);
    let mut visuals = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    visuals.dark_mode = dark;

    visuals.override_text_color = Some(p.field_ink);
    visuals.panel_fill = p.canvas;
    visuals.window_fill = p.card;
    visuals.window_stroke = Stroke::new(1.0_f32, p.line);
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
    visuals.extreme_bg_color = p.field_bg;
    visuals.faint_bg_color = p.soft_bg;
    visuals.code_bg_color = p.soft_bg;
    visuals.hyperlink_color = link();
    visuals.warn_fg_color = p.warn;
    visuals.error_fg_color = p.danger;
    visuals.selection.bg_fill = p.selection;
    visuals.selection.stroke = Stroke::new(1.0_f32, p.accent);
    visuals.text_cursor.stroke = Stroke::new(1.6_f32, p.ink);
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);
    visuals.striped = false;
    visuals.indent_has_left_vline = false;
    visuals.window_highlight_topmost = false;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = p.card;
    widgets.noninteractive.weak_bg_fill = p.card;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.line);
    widgets.noninteractive.rounding = Rounding::same(6.0);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.ink);

    widgets.inactive.bg_fill = p.soft_bg;
    widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
    widgets.inactive.bg_stroke = Stroke::new(1.0_f32, p.line_soft);
    widgets.inactive.rounding = Rounding::same(6.0);
    widgets.inactive.fg_stroke = Stroke::new(1.0_f32, p.ink);

    widgets.hovered.bg_fill = p.hover;
    widgets.hovered.weak_bg_fill = p.hover;
    widgets.hovered.bg_stroke = Stroke::new(1.0_f32, p.line);
    widgets.hovered.rounding = Rounding::same(6.0);
    widgets.hovered.fg_stroke = Stroke::new(1.0_f32, p.ink);

    widgets.active.bg_fill = p.accent_soft;
    widgets.active.weak_bg_fill = p.pressed;
    widgets.active.bg_stroke = Stroke::new(1.0_f32, p.accent);
    widgets.active.rounding = Rounding::same(6.0);
    widgets.active.fg_stroke = Stroke::new(1.0_f32, p.ink);

    widgets.open.bg_fill = p.accent_soft;
    widgets.open.weak_bg_fill = p.accent_soft;
    widgets.open.bg_stroke = Stroke::new(1.0_f32, p.line);
    widgets.open.rounding = Rounding::same(6.0);
    widgets.open.fg_stroke = Stroke::new(1.0_f32, p.ink);

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
    ctx.data_mut(|data| data.insert_temp(theme_id(), theme));
}

fn theme_id() -> egui::Id {
    egui::Id::new("notoffice.theme-installed")
}

/// Activate the window-local palette immediately. Widget styles are rebuilt
/// only when this window's theme differs from the installed one.
pub fn apply(ctx: &egui::Context, theme: Theme) {
    activate(theme);
    let installed = ctx.data(|data| data.get_temp::<Theme>(theme_id()));
    if installed != Some(theme) {
        install(ctx, theme);
    }
}

// ---------------------------------------------------------------------------
// Small building blocks
// ---------------------------------------------------------------------------

pub fn hairline(ui: &mut Ui) {
    let width = 1.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 18.0), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, palette().line);
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
        ui.painter().rect_filled(rect, rounding, palette().accent);
    } else {
        ui.painter()
            .rect_stroke(rect, rounding, Stroke::new(1.2_f32, palette().accent));
    }
    let paper = egui::Rect::from_center_size(rect.center(), Vec2::new(size * 0.44, size * 0.58));
    let paper_color = if filled {
        Color32::WHITE
    } else {
        palette().accent
    };
    ui.painter()
        .rect_filled(paper, Rounding::same(size * 0.06), paper_color);
    let ink = if filled {
        palette().accent
    } else {
        Color32::WHITE
    };
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
            .color(palette().ink),
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
                disc(painter, point(knob, y), 1.45 * scale, palette().chrome);
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
            painter.rect_filled(rect, rounding, palette().accent_soft);
        } else if response.is_pointer_button_down_on() && enabled {
            painter.rect_filled(rect, rounding, pressed());
        } else if hovered {
            painter.rect_filled(rect, rounding, hover());
        }

        let color = if !enabled {
            disabled()
        } else if self.selected {
            palette().accent
        } else {
            palette().ink
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
            .layout_no_wrap(self.text.to_owned(), font, palette().on_accent);
        let size = Vec2::new(galley.size().x + 24.0, 27.0);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let enabled = ui.is_enabled();
        let fill = if !enabled {
            palette().disabled
        } else if response.is_pointer_button_down_on() || response.hovered() {
            accent_hover()
        } else {
            palette().accent
        };
        ui.painter().rect_filled(rect, Rounding::same(6.0), fill);
        let origin = rect.center() - galley.size() * 0.5;
        ui.painter().galley(origin, galley, palette().on_accent);
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
        .fill(chrome())
        .inner_margin(Margin::symmetric(10.0, 0.0))
}

pub fn strip_frame() -> egui::Frame {
    egui::Frame::none().fill(chrome()).inner_margin(Margin {
        left: 12.0,
        right: 12.0,
        top: 4.0,
        bottom: 2.0,
    })
}

pub fn status_frame() -> egui::Frame {
    egui::Frame::none()
        .fill(soft_bg())
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
        Stroke::new(1.0_f32, palette().line),
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
        Stroke::new(1.0_f32, palette().line),
    );
}

#[cfg(test)]
mod theme_tests {
    use super::*;

    fn luminance(color: Color32) -> f32 {
        let convert = |value: u8| {
            let value = value as f32 / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * convert(color.r()) + 0.7152 * convert(color.g()) + 0.0722 * convert(color.b())
    }

    fn contrast(a: Color32, b: Color32) -> f32 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[test]
    fn every_theme_keeps_ordinary_text_and_fields_above_4_5() {
        for theme in Theme::ALL {
            let p = palette_for(theme);
            for (name, fg, bg) in [
                ("canvas", p.ink, p.canvas),
                ("chrome", p.ink, p.chrome),
                ("card", p.ink, p.card),
                ("field", p.field_ink, p.field_bg),
            ] {
                let ratio = contrast(fg, bg);
                assert!(
                    ratio >= 4.5,
                    "{theme:?} {name} contrast {ratio:.2} is below 4.5"
                );
            }
        }
    }

    #[test]
    fn theme_labels_and_slugs_are_stable() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_slug(theme.slug()), theme);
            assert!(!theme.label_ru().is_empty());
            assert!(!theme.label_en().is_empty());
        }
        assert_eq!(Theme::from_slug("broken"), Theme::Light);
        assert_eq!(Theme::from_slug(""), Theme::Light);
        assert_eq!(Theme::default(), Theme::Light);
    }
}
