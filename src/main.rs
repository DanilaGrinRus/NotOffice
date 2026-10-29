#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod docx_view;
mod legacy_layout;
mod pdf_view;
mod ui_shell;
mod ui_theme;
use docx_view::*;
use eframe::egui;
use liteoffice::clean_copy;
use liteoffice::clipboard;
use liteoffice::doc::LegacyDocPackage;
use liteoffice::docx::{DocumentModel, DocxPackage};
use liteoffice::file_ops::{self, DocumentKind};
use liteoffice::legacy_sidecar;
use liteoffice::preferences;
use liteoffice::print;
use liteoffice::replace;
use liteoffice::search::{self, Hit as SearchHit, Target as SearchTarget};
use liteoffice::spreadsheet_ui::{
    column_name as spreadsheet_column_name, SearchView as SpreadsheetSearchView,
    SpreadsheetDocument,
};
use liteoffice::theme::Theme;
use liteoffice::workbook_edit::{CellFormat, CellKey};
use liteoffice::xls::XlsPackage;
use liteoffice::xlsx::{SpreadsheetModel, XlsxPackage};
use liteoffice::{tfm, tr};
use std::path::{Path, PathBuf};

fn spawn_liteoffice_window(document: Option<&Path>) -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| {
        tfm!(
            "не удалось найти исполняемый файл NotOffice: {error}",
            "could not locate the NotOffice executable: {error}"
        )
    })?;
    liteoffice::startup::new_window_command(executable.into_os_string(), document)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            tfm!(
                "не удалось открыть независимое окно NotOffice: {error}",
                "could not start an independent NotOffice window: {error}"
            )
        })
}

/// Ctrl+N keeps its original meaning: a new window that is already a blank text
/// document, not the start screen.
fn spawn_blank_text_window() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|error| {
        tfm!(
            "не удалось найти исполняемый файл NotOffice: {error}",
            "could not locate the NotOffice executable: {error}"
        )
    })?;
    liteoffice::startup::new_text_window_command(executable.into_os_string())
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            tfm!(
                "не удалось открыть независимое окно NotOffice: {error}",
                "could not start an independent NotOffice window: {error}"
            )
        })
}

/// Command-line flag that opens a window with an empty text document.
const BLANK_TEXT_FLAG: &str = "--blank-text";

const PARAGRAPH_GAP: f32 = 10.0;
const FRAGMENT_HEIGHT: f32 = 24.0;
const FRAGMENT_MIN_WIDTH: f32 = 36.0;

// View-only zoom. Never alters DOCX font sizes or printer page settings.
// The limits are asserted by tests; the shell itself only calls sanitize_zoom.
#[allow(dead_code)]
const MIN_VIEW_ZOOM: f32 = preferences::MIN_VIEW_ZOOM;
#[allow(dead_code)]
const MAX_VIEW_ZOOM: f32 = preferences::MAX_VIEW_ZOOM;
const DEFAULT_VIEW_ZOOM: f32 = preferences::DEFAULT_VIEW_ZOOM;

fn sanitize_view_zoom(zoom: f32) -> f32 {
    preferences::sanitize_zoom(zoom)
}

/// Recovery notices are compared by prefix, and the text follows the selected
/// interface language. Accept both variants so the state machine stays
/// language-independent.
fn recovery_notice_pending(text: &str) -> bool {
    text.starts_with("Ожидание восстановления") || text.starts_with("Recovery pending")
}

fn recovery_notice_failed(text: &str) -> bool {
    text.starts_with("ОШИБКА записи") || text.starts_with("Recovery write FAILED")
}

// egui 0.28 turns Ctrl+Wheel into InputState::zoom_delta(). Applying that
// factor to the existing document zoom keeps the motion smooth while leaving
// egui's pixels-per-point (the whole application UI scale) unchanged.
fn ctrl_wheel_document_zoom(
    current_zoom: f32,
    ctrl_held: bool,
    document_hovered: bool,
    text_input_or_dialog_active: bool,
    zoom_factor: f32,
) -> Option<f32> {
    if !ctrl_held
        || !document_hovered
        || text_input_or_dialog_active
        || !zoom_factor.is_finite()
        || zoom_factor <= 0.0
        || (zoom_factor - 1.0).abs() < f32::EPSILON
    {
        return None;
    }
    Some(sanitize_view_zoom(current_zoom * zoom_factor))
}

fn ctrl_s_save_requested(
    command_held: bool,
    s_pressed: bool,
    has_docx: bool,
    text_field_focused: bool,
) -> bool {
    command_held && s_pressed && has_docx && !text_field_focused
}

fn decode_document_image(
    ctx: &egui::Context,
    bytes: &[u8],
    name: String,
) -> Option<egui::TextureHandle> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let (w, h) = reader.into_dimensions().ok()?;
    if w == 0 || h == 0 || w > 16384 || h > 16384 || u64::from(w) * u64::from(h) > 32_000_000 {
        return None;
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    let rgba = decoded.to_rgba8();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    );
    Some(ctx.load_texture(name, color_image, egui::TextureOptions::LINEAR))
}

fn decode_doc_gallery(
    ctx: &egui::Context,
    package: &LegacyDocPackage,
) -> Vec<Option<egui::TextureHandle>> {
    package
        .images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            decode_document_image(ctx, &image.bytes, format!("doc-raster-{index}"))
        })
        .collect()
}

// DOC 97-2003 does not expose picture positions through our current raster
// recovery path. This manual anchor is a VIEW preference only; it does not
// modify the original DOC or claim to reconstruct Word layout.
fn draw_legacy_picture(
    ui: &mut egui::Ui,
    texture: &egui::TextureHandle,
    width_pct: f32,
    _zoom: f32,
) {
    let intrinsic = texture.size_vec2();
    if intrinsic.x <= 0.0 || intrinsic.y <= 0.0 {
        return;
    }
    let width = (ui.available_width().max(100.0) * (width_pct / 100.0)).max(40.0);
    let scale = (width / intrinsic.x).min(2.0);
    ui.image((texture.id(), intrinsic * scale));
}

struct LegacyViewOptions<'a> {
    zoom: f32,
    page_settings: print::PageSettings,
    query: &'a str,
    current_hit: Option<&'a SearchHit>,
    scroll_to_hit: bool,
}

fn render_legacy_doc(
    ui: &mut egui::Ui,
    package: &LegacyDocPackage,
    textures: &[Option<egui::TextureHandle>],
    image_anchors: &mut Vec<Option<usize>>,
    image_widths: &mut Vec<f32>,
    options: &LegacyViewOptions<'_>,
) {
    let zoom = options.zoom;
    let page_settings = options.page_settings;
    if image_anchors.len() != textures.len() {
        image_anchors.resize(textures.len(), None);
    }
    if image_widths.len() != textures.len() {
        image_widths.resize(textures.len(), 85.0);
    }
    let page_settings = page_settings.sanitized();
    let (paper_width_mm, _) = page_settings.page_size_mm();
    let paper_width_px = paper_width_mm * 96.0 / 25.4 * zoom;
    let margin_px = page_settings.margin_mm * 96.0 / 25.4 * zoom;
    egui::Frame::none()
        .fill(egui::Color32::WHITE)
        .inner_margin(margin_px)
        .show(ui, |ui| {
            ui.set_min_width(paper_width_px - 2.0 * margin_px);
            ui.set_max_width(paper_width_px - 2.0 * margin_px);
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.label(egui::RichText::new(tr!("Просмотр DOC — размещение изображений (экспериментально)", "DOC preview — image placement (experimental)"))
                    .heading().color(egui::Color32::BLACK));
                ui.label(egui::RichText::new(
                    "Word 97–2003: text is recovered in CLX order. Raster-to-shape mapping is NOT recovered. \
                     Word shape CP anchors are read when available, but images are NOT reliably matched to shapes. \
                     All proposed placements are unverified; manual positions can be saved in a sidecar. \
                     Original Word layout is not reproduced. Page separators below are height estimates, not print-exact breaks."
                ).color(egui::Color32::DARK_GRAY));
                if !textures.is_empty() {
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(tr!("Все изображения в галерею", "All images to gallery")).clicked() {
                            image_anchors.iter_mut().for_each(|anchor| *anchor = None);
                        }
                        if !package.shape_anchors.is_empty() {
                            ui.label(tfm!("Найдено привязок фигур Word: {}; подтверждённых растровых связей: {}", "Word shape anchors found: {}; verified raster links: {}",
                                package.shape_anchors.len(), package.verified_shape_images.len()));
                            if !package.verified_shape_images.is_empty()
                                && ui.button(tfm!("Разместить подтверждённые изображения OfficeArt: {}", "Place {} verified OfficeArt images", package.verified_shape_images.len()))
                                    .on_hover_text(tr!("Сопоставлены только точные байты изображений по ID фигуры и хранилищу BLIP", "Only exact image bytes matched through shape ID and BLIP store"))
                                    .clicked() {
                                    for link in &package.verified_shape_images {
                                        if let Some(target) = image_anchors.get_mut(link.image_index) {
                                            *target = Some(link.paragraph_index.min(package.paragraphs.len().saturating_sub(1)));
                                        }
                                    }
                            }
                            if package.shape_anchors.len() == textures.len() {
                                if ui.button(tr!("Подсказать по CP фигур Word (порядок изображений не проверен)", "Suggest from Word shape CPs (unverified image order)")).clicked() {
                                    for (i, anchor) in image_anchors.iter_mut().enumerate() {
                                        *anchor = Some(package.shape_anchors[i].paragraph_index
                                            .min(package.paragraphs.len().saturating_sub(1)));
                                    }
                                }
                            } else {
                                ui.label(tr!("Количество фигур и изображений различается; автоматическое сопоставление отключено", "Shape/image counts differ; automatic pairing disabled"));
                            }
                        }
                        if ui.button(tr!("Подсказать равномерные позиции (приблизительно)", "Suggest evenly spaced positions (approximate)")).clicked() {
                            for (i, anchor) in image_anchors.iter_mut().enumerate() {
                                if !package.paragraphs.is_empty() {
                                    *anchor = Some(((i + 1) * package.paragraphs.len()
                                        / (textures.len() + 1)).min(package.paragraphs.len() - 1));
                                }
                            }
                        }
                    });
                    egui::CollapsingHeader::new(tfm!(
                        "Восстановленные изображения ({}) — задайте позицию и ширину", "Recovered images ({}) — assign position and width", textures.len()
                    )).default_open(true).show(ui, |ui| {
                        for i in 0..textures.len() {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(tfm!("Изображение {}", "Image {}", i + 1));
                                egui::ComboBox::from_id_source(("doc-image-anchor", i))
                                    .selected_text(match image_anchors[i] {
                                        Some(index) => tfm!("После абзаца {}", "After paragraph {}", index + 1),
                                        None => tr!("Только галерея", "Gallery only").to_owned(),
                                    })
                                    .width(170.0)
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(&mut image_anchors[i], None, tr!("Только галерея", "Gallery only"));
                                        for paragraph_index in 0..package.paragraphs.len() {
                                            ui.selectable_value(
                                                &mut image_anchors[i], Some(paragraph_index),
                                                tfm!("После абзаца {}", "After paragraph {}", paragraph_index + 1)
                                            );
                                        }
                                    });
                                ui.label(tr!("Ширина, %:", "Width %:"));
                                ui.add(egui::DragValue::new(&mut image_widths[i])
                                    .speed(1.0).range(15.0..=100.0).suffix("%"));
                            });

                        }
                    });
                }
                ui.add_space(14.0);
                // The same page plan feeds both preview and Windows Print Preview.
                let raster_info = package.images.iter().map(|asset| {
                    match image::ImageReader::new(std::io::Cursor::new(&asset.bytes)).with_guessed_format()
                        .ok().and_then(|reader| reader.into_dimensions().ok()) {
                        Some((w,h)) => legacy_layout::RasterInfo { width_px:w, height_px:h, dpi_x:96.0 },
                        None => legacy_layout::RasterInfo {width_px:0,height_px:0,dpi_x:96.0},
                    }
                }).collect::<Vec<_>>();
                let pages = legacy_layout::paginate(&package.paragraphs, image_anchors, image_widths,
                    &raster_info, page_settings);
                let (_, paper_height_mm) = page_settings.page_size_mm();
                let page_inner_height = (paper_height_mm - 2.0 * page_settings.margin_mm).max(20.0)
                    * 96.0 / 25.4 * zoom;
                for (page_index, items) in pages.iter().enumerate() {
                    ui.add_space(18.0);
                    ui.label(egui::RichText::new(tfm!("Страница DOC {} из {} (приблизительная разметка)", "DOC page {} of {} (approximate layout)", page_index + 1, pages.len()))
                        .color(egui::Color32::DARK_GRAY));
                    egui::Frame::canvas(ui.style()).fill(egui::Color32::WHITE)
                        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::LIGHT_GRAY))
                        .inner_margin(12.0 * zoom)
                        .show(ui, |ui| {
                            ui.set_min_width((page_settings.printable_width_mm() * 96.0 / 25.4 * zoom - 24.0 * zoom).max(100.0));
                            ui.set_min_height((page_inner_height - 24.0 * zoom).max(100.0));
                            for item in items {
                                match item {
                                    legacy_layout::Item::Paragraph(paragraph_index) => {
                                        let paragraph = &package.paragraphs[*paragraph_index];
                                        let links: Vec<_> = package.links.iter()
                                            .filter(|link| link.paragraph_index == *paragraph_index).collect();
                                        let matched = !search::ranges(paragraph, options.query, 1).is_empty();
                                        let active = options.current_hit.is_some_and(|hit| hit.target == SearchTarget::DocParagraph(*paragraph_index));
                                        let response = egui::Frame::none().fill(if active {
                                            ui_theme::search_active()
                                        } else if matched {
                                            ui_theme::search_hit()
                                        } else { egui::Color32::TRANSPARENT }).show(ui, |ui| {
                                        if links.len() == 1 && links[0].label.trim() == paragraph.trim() {
                                            ui.add(egui::Hyperlink::from_label_and_url(
                                                egui::RichText::new(paragraph).size(15.0 * zoom),
                                                links[0].url.as_str(),
                                            ));
                                        } else {
                                            ui.add(egui::Label::new(egui::RichText::new(paragraph)
                                                .color(egui::Color32::BLACK).size(15.0 * zoom)).wrap());
                                            for link in links {
                                                ui.add(egui::Hyperlink::from_label_and_url(
                                                    egui::RichText::new(format!("↗ {}", link.label)).size(15.0 * zoom),
                                                    link.url.as_str(),
                                                ));
                                            }
                                        }
                                        });
                                        if active && options.scroll_to_hit { response.response.scroll_to_me(Some(egui::Align::Center)); }
                                        ui.add_space(4.0 * zoom);
                                    }
                                    legacy_layout::Item::Image(image_idx) => {
                                        if let Some(tex) = textures.get(*image_idx).and_then(Option::as_ref) {
                                            draw_legacy_picture(ui, tex, *image_widths.get(*image_idx).unwrap_or(&85.0), zoom);
                                            ui.add_space(8.0 * zoom);
                                        } else {
                                            ui.colored_label(egui::Color32::DARK_RED, tr!("Не удалось декодировать изображение для просмотра", "Image could not be decoded for preview"));
                                        }
                                    }
                                    legacy_layout::Item::UnplacedHeading => {
                                        ui.label(egui::RichText::new(tr!("Восстановленные изображения (без позиций; не оригинальная разметка Word)", "Recovered images (unplaced; not original Word layout)"))
                                            .color(egui::Color32::DARK_GRAY));
                                    }
                                }
                            }
                        });
                }
            });
        });
}

fn spreadsheet_print_blocks(
    book: &SpreadsheetDocument,
    settings: print::PageSettings,
) -> Vec<print::PrintBlock> {
    let Some(sheet) = book
        .sheets
        .get(book.selected_sheet)
        .or_else(|| book.sheets.first())
    else {
        return Vec::new();
    };
    let max_row = sheet
        .cells
        .iter()
        .map(|cell| cell.row + 1)
        .max()
        .unwrap_or(0)
        .min(10_000);
    let max_column = sheet
        .cells
        .iter()
        .map(|cell| cell.column + 1)
        .max()
        .unwrap_or(0)
        .min(32);
    if max_row == 0 || max_column == 0 {
        return vec![print::PrintBlock::Text(sheet.sheet_name.clone())];
    }
    let rows_per_page = if settings.landscape { 24 } else { 34 };
    let mut blocks = Vec::new();
    let mut start = 0usize;
    let mut page = 0usize;
    while start < max_row {
        let end = (start + rows_per_page).min(max_row);
        if page > 0 {
            blocks.push(print::PrintBlock::PageBreak);
        }
        blocks.push(print::PrintBlock::Text(format!(
            "{} — {}",
            sheet.sheet_name,
            page + 1
        )));
        for row in start..end {
            let mut values = Vec::with_capacity(max_column);
            for column in 0..max_column {
                let value = sheet
                    .cells
                    .iter()
                    .find(|cell| cell.row == row && cell.column == column)
                    .map(|cell| cell.value.as_str())
                    .unwrap_or("");
                values.push(value.to_owned());
            }
            blocks.push(print::PrintBlock::TableRow(values));
        }
        start = end;
        page += 1;
    }
    blocks
}

fn install_windows_fonts(ctx: &egui::Context) {
    let root = std::path::Path::new(r"C:\Windows\Fonts");
    let mut defs = egui::FontDefinitions::default();
    let candidates = [
        ("Segoe UI", "segoeui.ttf"),
        ("Arial", "arial.ttf"),
        ("Segoe UI Symbol", "seguisym.ttf"),
    ];
    let mut installed = Vec::new();
    for (name, filename) in candidates {
        if let Ok(bytes) = std::fs::read(root.join(filename)) {
            defs.font_data
                .insert(name.into(), egui::FontData::from_owned(bytes));
            installed.push(name.to_string());
        }
    }
    if !installed.is_empty() {
        if let Some(family) = defs.families.get_mut(&egui::FontFamily::Proportional) {
            for name in installed.iter().rev() {
                family.insert(0, name.clone());
            }
        }
    }
    // The compact shell uses a semibold face for headings and labels. Keep the
    // regular proportional stack as fallback so missing font files never leave
    // the custom family undefined (which would panic while laying out text).
    let fallback = defs
        .families
        .get(&egui::FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut semibold = Vec::new();
    if let Ok(bytes) = std::fs::read(root.join("seguisb.ttf")) {
        defs.font_data.insert(
            "Segoe UI Semibold".into(),
            egui::FontData::from_owned(bytes),
        );
        semibold.push("Segoe UI Semibold".to_string());
    }
    if semibold.is_empty() {
        semibold = fallback;
    } else {
        semibold.extend(fallback);
    }
    defs.families.insert(
        egui::FontFamily::Name(std::sync::Arc::from("semibold")),
        semibold,
    );
    ctx.set_fonts(defs);
}

/// Window/taskbar icon drawn in memory: no image file, no extra crate.
fn app_icon() -> egui::IconData {
    const SIZE: u32 = 64;
    fn inside_rounded(x: f32, y: f32, x0: f32, y0: f32, x1: f32, y1: f32, r: f32) -> bool {
        if x < x0 || x > x1 || y < y0 || y > y1 {
            return false;
        }
        let cx = x.clamp(x0 + r, x1 - r);
        let cy = y.clamp(y0 + r, y1 - r);
        (x - cx).powi(2) + (y - cy).powi(2) <= r * r
    }
    let accent = [0x17_u8, 0x6B, 0x5A];
    let mut rgba = vec![0_u8; (SIZE * SIZE * 4) as usize];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let mut color = [0_u8, 0, 0, 0];
            if inside_rounded(px, py, 4.0, 4.0, 60.0, 60.0, 14.0) {
                color = [accent[0], accent[1], accent[2], 255];
            }
            if inside_rounded(px, py, 19.0, 14.0, 45.0, 50.0, 3.0) {
                color = [255, 255, 255, 255];
            }
            for line_y in [23.0_f32, 30.5, 38.0] {
                if (25.0..=39.0).contains(&px) && (py - line_y).abs() <= 1.4 {
                    color = [accent[0], accent[1], accent[2], 255];
                }
            }
            let index = ((y * SIZE + x) * 4) as usize;
            rgba[index..index + 4].copy_from_slice(&color);
        }
    }
    egui::IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}

fn main() -> eframe::Result<()> {
    liteoffice::app_paths::migrate_legacy_data();
    let raw_args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let start_blank_text = raw_args.iter().any(|argument| argument == BLANK_TEXT_FLAG);
    let document_args = raw_args
        .into_iter()
        .filter(|argument| argument != BLANK_TEXT_FLAG)
        .collect::<Vec<_>>();
    let startup_document = liteoffice::startup::document_path_from_args(document_args);
    // A calm light workspace by default; the document should be the brightest
    // surface on screen, so the shell never follows a dark system theme.
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 800.0])
            .with_min_inner_size([680.0, 480.0])
            .with_icon(app_icon()),
        follow_system_theme: false,
        default_theme: eframe::Theme::Light,
        ..Default::default()
    };
    eframe::run_native(
        concat!("NotOffice v", env!("CARGO_PKG_VERSION")),
        native_options,
        Box::new(move |cc| {
            // Fonts and the visual style must exist before the first frame:
            // egui applies ctx.set_fonts() at the start of the next frame.
            liteoffice::i18n::load();
            install_windows_fonts(&cc.egui_ctx);
            ui_theme::install(&cc.egui_ctx, Theme::Light);
            ui_theme::mark_fonts_ready(&cc.egui_ctx);
            let mut app = if start_blank_text {
                NotOfficeApp::blank_text()
            } else {
                // No document argument: show the start screen with recent files
                // instead of filling the window with an empty monospace editor.
                NotOfficeApp::default()
            };
            app.fonts_initialized = true;
            match startup_document.clone() {
                Ok(Some(path)) => app.request_open_path(path),
                Ok(None) => {}
                Err(error) => {
                    app.status = tfm!(
                        "Не удалось открыть документ из командной строки: {error}",
                        "Cannot open command-line document: {error}"
                    )
                }
            }
            app.initialize_recovery(liteoffice::app_paths::data_dir().join("NotOffice-recovery"));
            Ok(Box::new(app))
        }),
    )
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingAction {
    Open(PathBuf),
    Close,
}

#[derive(Clone)]
struct CellEditDialog {
    row: usize,
    column: usize,
    text: String,
    focus: bool,
}

#[derive(Clone, PartialEq)]
struct WorkbookSnapshot {
    sheets: Vec<SpreadsheetModel>,
    formats: std::collections::BTreeMap<CellKey, CellFormat>,
}

struct NotOfficeApp {
    text: Option<liteoffice::text_document::TextDocument>,
    text_focus_pending: bool,
    text_input_id: Option<egui::Id>,
    pdf_print_open: bool,
    pdf_print_settings: liteoffice::pdf_print::PdfPrintSettings,
    pdf: Option<pdf_view::PdfView>,
    docx: Option<DocxPackage>,
    spreadsheet: Option<SpreadsheetDocument>,
    cell_edit_dialog: Option<CellEditDialog>,
    legacy_doc: Option<LegacyDocPackage>,
    legacy_doc_textures: Vec<Option<egui::TextureHandle>>,
    docx_textures: Vec<Option<egui::TextureHandle>>,
    doc_image_anchors: Vec<Option<usize>>,
    doc_image_widths: Vec<f32>,
    opened_path: Option<PathBuf>,
    saved_docx_model: Option<DocumentModel>,
    undo_stack: Vec<DocumentModel>,
    redo_stack: Vec<DocumentModel>,
    workbook_undo: Vec<WorkbookSnapshot>,
    workbook_redo: Vec<WorkbookSnapshot>,
    text_undo: Vec<String>,
    text_redo: Vec<String>,
    suppress_text_history: bool,
    search_query: String,
    current_match: usize,
    search_results: search::Results,
    search_cached_query: String,
    search_dirty: bool,
    search_scroll_requested: bool,
    search_focus_requested: bool,
    replace_open: bool,
    replacement_text: String,
    replacement_input_id: Option<egui::Id>,
    replacement_focus_requested: bool,
    pending_action: Option<PendingAction>,
    conflict_destination: Option<PathBuf>,
    close_approved: bool,
    save_prompt_open: bool,
    save_prompt_copy: bool,
    save_prompt_reason: String,
    legacy_edit_prompt: bool,
    recovery_root: Option<PathBuf>,
    recovery_session: Option<liteoffice::recovery::Session>,
    recovery_candidates: Vec<liteoffice::recovery::Session>,
    recovered_document: bool,
    recovery_snapshot_model: Option<DocumentModel>,
    recovery_snapshot_text: Option<(liteoffice::text_document::TextKind, String)>,
    recovery_snapshot_workbook: Option<(
        Vec<SpreadsheetModel>,
        std::collections::BTreeMap<CellKey, CellFormat>,
    )>,
    recovery_last_attempt: Option<std::time::Instant>,
    recovery_notice: String,
    recovery_discard_confirmation: bool,
    open_diagnostics: Vec<String>,
    recent_paths: Vec<PathBuf>,
    zoom: f32,
    status: String,
    selected_fragment_id: Option<usize>,
    docx_editor_generation: u64,
    docx_focus_pending: bool,
    docx_view_clip_size: Option<egui::Vec2>,
    select_all_docx: bool,
    select_all_text: bool,
    search_input_id: Option<egui::Id>,
    font_input_id: Option<egui::Id>,
    fonts_initialized: bool,
    print_receiver: Option<std::sync::mpsc::Receiver<String>>,
    print_settings: print::PageSettings,
    theme: Theme,
    document_view_rect: Option<egui::Rect>,
    /// Compact "Page setup" popover rendered by the redesigned shell.
    page_setup_open: bool,
    /// Find/replace strip visibility; the query itself stays across documents.
    search_active: bool,
}

fn recent_store_path() -> Option<PathBuf> {
    Some(liteoffice::app_paths::file("NotOffice-recent.txt"))
}

fn load_recent_paths() -> Vec<PathBuf> {
    recent_store_path()
        .map(|path| liteoffice::recent::load_paths(&path))
        .unwrap_or_default()
}

// Portable per-user appearance and print preferences. The original documents are never changed.
fn preferences_path() -> Option<PathBuf> {
    Some(liteoffice::app_paths::file("NotOffice-settings.txt"))
}

fn load_preferences() -> (print::PageSettings, f32, Theme) {
    preferences_path()
        .map(|path| preferences::load(&path))
        .unwrap_or((
            print::PageSettings::default(),
            DEFAULT_VIEW_ZOOM,
            Theme::Light,
        ))
}

fn save_preferences(
    before: (print::PageSettings, f32, Theme),
    settings: print::PageSettings,
    zoom: f32,
    theme: Theme,
) -> std::io::Result<()> {
    let Some(path) = preferences_path() else {
        return Ok(());
    };
    preferences::save_changes(&path, before, (settings, zoom, theme))
}

impl Default for NotOfficeApp {
    fn default() -> Self {
        let (saved_print_settings, saved_zoom, saved_theme) = load_preferences();
        Self {
            pdf: None,
            text: None,
            text_focus_pending: false,
            text_input_id: None,
            pdf_print_open: false,
            pdf_print_settings: Default::default(),
            docx: None,
            spreadsheet: None,
            cell_edit_dialog: None,
            legacy_doc: None,
            legacy_doc_textures: Vec::new(),
            docx_textures: Vec::new(),
            doc_image_anchors: Vec::new(),
            doc_image_widths: Vec::new(),
            opened_path: None,
            saved_docx_model: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            workbook_undo: Vec::new(),
            workbook_redo: Vec::new(),
            text_undo: Vec::new(),
            text_redo: Vec::new(),
            suppress_text_history: false,
            search_query: String::new(),
            current_match: 0,
            search_results: search::Results::default(),
            search_cached_query: String::new(),
            search_dirty: true,
            search_scroll_requested: false,
            search_focus_requested: false,
            replace_open: true,
            replacement_text: String::new(),
            replacement_input_id: None,
            replacement_focus_requested: false,
            pending_action: None,
            conflict_destination: None,
            close_approved: false,
            save_prompt_open: false,
            save_prompt_copy: false,
            save_prompt_reason: String::new(),
            legacy_edit_prompt: false,
            recovery_root: None,
            recovery_session: None,
            recovery_candidates: Vec::new(),
            recovered_document: false,
            recovery_snapshot_model: None,
            recovery_snapshot_text: None,
            recovery_snapshot_workbook: None,
            recovery_last_attempt: None,
            recovery_notice: String::new(),
            recovery_discard_confirmation: false,
            open_diagnostics: Vec::new(),
            recent_paths: load_recent_paths(),
            zoom: saved_zoom,
            status: String::new(),
            selected_fragment_id: None,
            docx_editor_generation: 0,
            docx_focus_pending: false,
            docx_view_clip_size: None,
            select_all_docx: false,
            select_all_text: false,
            search_input_id: None,
            font_input_id: None,
            fonts_initialized: false,
            print_receiver: None,
            print_settings: saved_print_settings,
            theme: saved_theme,
            document_view_rect: None,
            page_setup_open: false,
            search_active: true,
        }
    }
}

impl NotOfficeApp {
    fn blank_text() -> Self {
        Self {
            text: Some(Default::default()),
            text_focus_pending: true,
            ..Default::default()
        }
    }
    fn refresh_search(&mut self) {
        let query_changed = self.search_query != self.search_cached_query;
        if !self.search_dirty && !query_changed {
            return;
        }
        self.search_results = if let Some(document) = &self.text {
            search::text(&document.text, &self.search_query)
        } else if let Some(package) = &self.docx {
            search::docx(&package.model, &self.search_query)
        } else if let Some(package) = &self.legacy_doc {
            search::legacy(&package.paragraphs, &self.search_query)
        } else if let Some(book) = &self.spreadsheet {
            search::sheets(&book.sheets, &self.search_query)
        } else {
            search::Results::default()
        };
        if query_changed {
            self.current_match = 0;
        }
        self.current_match = self
            .current_match
            .min(self.search_results.hits.len().saturating_sub(1));
        self.search_cached_query = self.search_query.clone();
        self.search_dirty = false;
        if query_changed || self.search_scroll_requested {
            self.activate_search_hit();
        }
    }

    fn activate_search_hit(&mut self) {
        self.search_scroll_requested = !self.search_results.hits.is_empty();
        if let Some(SearchHit {
            target: SearchTarget::Cell { sheet, row, column },
            ..
        }) = self.search_results.hits.get(self.current_match)
        {
            if let Some(book) = self.spreadsheet.as_mut() {
                book.selected_sheet = *sheet;
                book.selected_cell = Some((*row, *column));
                book.preview_anchor = Some((*row, *column));
            }
        }
    }

    fn navigate_search(&mut self, backwards: bool) {
        self.refresh_search();
        self.current_match = search::next_index(
            self.current_match,
            self.search_results.hits.len(),
            backwards,
        );
        self.activate_search_hit();
    }

    fn search_context(&self) -> String {
        let Some(hit) = self.search_results.hits.get(self.current_match) else {
            return String::new();
        };
        let (label, text) = match hit.target {
            SearchTarget::Text => (
                tr!("Текст", "Text").to_string(),
                self.text
                    .as_ref()
                    .map(|document| document.text.clone())
                    .unwrap_or_default(),
            ),
            SearchTarget::DocxParagraph(index) => (
                format!("DOCX paragraph {}", index + 1),
                self.docx
                    .as_ref()
                    .and_then(|package| package.model.paragraphs.get(index))
                    .map(|paragraph| {
                        paragraph
                            .fragments
                            .iter()
                            .map(|fragment| fragment.text.as_str())
                            .collect::<String>()
                    })
                    .unwrap_or_default(),
            ),
            SearchTarget::DocParagraph(index) => (
                tfm!("Абзац DOC {}", "DOC paragraph {}", index + 1),
                self.legacy_doc
                    .as_ref()
                    .and_then(|package| package.paragraphs.get(index))
                    .cloned()
                    .unwrap_or_default(),
            ),
            SearchTarget::Cell { sheet, row, column } => {
                let sheet = self
                    .spreadsheet
                    .as_ref()
                    .and_then(|book| book.sheets.get(sheet));
                (
                    format!(
                        "{}!{}{}",
                        sheet
                            .map(|sheet| sheet.sheet_name.as_str())
                            .unwrap_or(tr!("Лист", "Sheet")),
                        spreadsheet_column_name(column),
                        row + 1
                    ),
                    sheet
                        .map(|sheet| sheet.cell_value(row, column).to_owned())
                        .unwrap_or_default(),
                )
            }
        };
        let Some(matched) = text.get(hit.bytes.clone()) else {
            return label;
        };
        let before = text[..hit.bytes.start]
            .chars()
            .rev()
            .take(24)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<String>();
        let after = text[hit.bytes.end..].chars().take(32).collect::<String>();
        format!(
            "{label}: …{before}[{}]{after}…",
            matched.chars().take(64).collect::<String>()
        )
    }

    fn show_search_bar(&mut self, ui: &mut egui::Ui) {
        self.replacement_input_id = None;
        // Open can finish in the chrome above. Remove stale Find focus before
        // that field can consume this document's first Text event.
        self.prepare_docx_focus(ui.ctx());
        if !self.search_visible() {
            self.search_input_id = None;
            return;
        }
        if self.pdf.is_some() {
            self.search_input_id = None;
            ui.label(
                egui::RichText::new(tr!(
                    "PDF: только растр, поиск по тексту недоступен",
                    "PDF: raster only, no text search"
                ))
                .size(11.0)
                .color(ui_theme::faint()),
            );
            return;
        }
        if !self.has_document() {
            self.search_input_id = None;
            return;
        }
        ui.label(
            egui::RichText::new(tr!("Найти", "Find"))
                .size(11.0)
                .color(ui_theme::faint()),
        );
        let id = egui::Id::new("liteoffice-global-search");
        let had_focus = ui.memory(|memory| memory.has_focus(id));
        if ui.is_enabled()
            && had_focus
            && ui.input(|input| {
                input.key_pressed(egui::Key::Tab)
                    && !input.modifiers.shift
                    && !input.modifiers.command
                    && !input.modifiers.alt
            })
        {
            // Render the replacement input directly after Find in focus order,
            // before navigation/Clear buttons; standard Tab picks it up.
            self.replace_open = true;
        }
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.search_query)
                .id(id)
                .desired_width(190.0)
                .hint_text(tr!("Поиск по документу", "Search this document")),
        );
        self.search_input_id = Some(response.id);
        if self.search_focus_requested && ui.is_enabled() {
            self.docx_focus_pending = false;
            response.request_focus();
            self.search_focus_requested = false;
        }
        if response.clicked() {
            self.docx_focus_pending = false;
        }
        self.refresh_search();
        if ui.is_enabled()
            && (response.has_focus() || response.lost_focus())
            && ui.input(|input| input.key_pressed(egui::Key::Enter))
        {
            self.navigate_search(ui.input(|input| input.modifiers.shift));
            response.request_focus();
        }
        if ui.is_enabled()
            && (had_focus || response.lost_focus())
            && ui.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.search_query.clear();
            self.refresh_search();
            response.surrender_focus();
            self.search_active = false;
            self.replace_open = false;
        }
        self.replace_open = true;
        self.show_replacement_input(ui);
        let count = self.search_results.hits.len();
        if ui
            .add_enabled(
                count > 0,
                ui_theme::IconButton::new(
                    ui_theme::Icon::ChevronLeft,
                    tr!(
                        "Предыдущее совпадение (Shift+Enter)",
                        "Previous match (Shift+Enter)"
                    ),
                )
                .small(),
            )
            .clicked()
        {
            self.navigate_search(true);
        }
        if ui
            .add_enabled(
                count > 0,
                ui_theme::IconButton::new(
                    ui_theme::Icon::ChevronRight,
                    tr!("Следующее совпадение (Enter)", "Next match (Enter)"),
                )
                .small(),
            )
            .clicked()
        {
            self.navigate_search(false);
        }
        ui.label(
            egui::RichText::new(tfm!(
                "{} из {count}{} совпадений",
                "{} / {count}{} matches",
                if count == 0 {
                    0
                } else {
                    self.current_match + 1
                },
                if self.search_results.limited {
                    tr!("+ (лимит)", "+ (limit)")
                } else {
                    ""
                }
            ))
            .size(10.5)
            .color(ui_theme::muted()),
        );
        if ui
            .add(
                egui::Button::new(
                    egui::RichText::new(tr!("Очистить замену", "Clear replacement"))
                        .size(11.0)
                        .color(ui_theme::muted()),
                )
                .frame(false),
            )
            .on_hover_text(tr!(
                "Очистить только поле «Заменить на»; поисковый запрос не меняется",
                "Clear only the replacement field; the search query is unchanged"
            ))
            .clicked()
        {
            self.replacement_text.clear();
            self.replacement_focus_requested = true;
        }
        if ui
            .add(
                ui_theme::IconButton::new(
                    ui_theme::Icon::Close,
                    tr!("Закрыть поиск", "Close find"),
                )
                .small(),
            )
            .clicked()
        {
            self.search_query.clear();
            self.refresh_search();
            self.search_active = false;
            self.replace_open = false;
        }
    }

    fn show_replacement_input(&mut self, ui: &mut egui::Ui) {
        ui.label(tr!("Заменить на", "Replace with"));
        let id = egui::Id::new("liteoffice-replacement-text");
        let had_focus = ui.memory(|memory| memory.has_focus(id));
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.replacement_text)
                .id(id)
                .desired_width(220.0),
        );
        self.replacement_input_id = Some(response.id);
        if self.replacement_focus_requested {
            response.request_focus();
            self.replacement_focus_requested = false;
        }
        if response.clicked() {
            self.docx_focus_pending = false;
        }
        let replace_enter = ui.is_enabled()
            && (response.has_focus() || response.lost_focus())
            && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if replace_enter && !self.decision_pending() && !self.search_results.hits.is_empty() {
            if self.docx.is_some() {
                self.replace_docx(false);
            } else if self.text.is_some() {
                self.replace_text(false);
            } else {
                self.replace_spreadsheet(false);
            }
            response.request_focus();
        }
        if ui.is_enabled()
            && (had_focus || response.lost_focus())
            && ui.input(|input| input.key_pressed(egui::Key::Escape))
        {
            self.search_active = false;
            self.replace_open = false;
            self.replacement_input_id = None;
            self.replacement_focus_requested = false;
            self.search_query.clear();
            self.refresh_search();
            response.surrender_focus();
        }
        if had_focus && ui.input(|input| input.key_pressed(egui::Key::Tab)) {
            // egui applies reverse focus order on the following frame.
            ui.ctx().request_repaint();
        }
    }

    fn show_replace_bar(&mut self, ui: &mut egui::Ui) {
        if !self.replace_open || !self.has_document() || self.pdf.is_some() {
            return;
        }
        ui.horizontal_wrapped(|ui| {
            let editable = !self.decision_pending()
                && (self.docx.is_some()
                    || self.text.is_some()
                    || self
                        .spreadsheet
                        .as_ref()
                        .is_some_and(|book| book.editor.is_some()));
            let can_replace = editable && !self.search_results.hits.is_empty();
            if ui
                .add_enabled(
                    can_replace,
                    egui::Button::new(egui::RichText::new(tr!("Заменить", "Replace")).size(11.0)),
                )
                .on_hover_text(tr!("Заменить выбранное совпадение; Enter в поле замены", "Replace selected Find occurrence; Enter in replacement field"))
                .clicked()
            {
                if self.docx.is_some() {
                    self.replace_docx(false);
                } else if self.text.is_some() {
                    self.replace_text(false);
                } else {
                    self.replace_spreadsheet(false);
                }
                self.replacement_focus_requested = true;
            }
            if ui
                .add_enabled(
                    can_replace && !self.search_results.limited,
                    egui::Button::new(egui::RichText::new(tr!("Заменить всё", "Replace all")).size(11.0)),
                )
                .on_hover_text(tr!("Одна транзакция отмены; при более 10 000 совпадений замена блокируется целиком", "One Undo transaction; blocked above 10,000 matches, never partial"))
                .clicked()
            {
                if self.docx.is_some() {
                    self.replace_docx(true);
                } else if self.text.is_some() {
                    self.replace_text(true);
                } else {
                    self.replace_spreadsheet(true);
                }
                self.replacement_focus_requested = true;
            }
            if !editable {
                ui.label(
                    egui::RichText::new(tr!(
                        "Замена доступна для DOCX и таблиц; этот формат только для чтения",
                        "Replacement is available for DOCX and workbooks; this format is read-only"
                    ))
                    .size(10.5)
                    .color(ui_theme::faint()),
                );
            } else if self.search_results.limited {
                ui.label(
                    egui::RichText::new(tr!("Уточните запрос (лимит 10 000 совпадений)", "Narrow the query (10,000 match limit)"))
                        .size(10.5)
                        .color(ui_theme::warn()),
                );
            }
            // "Done" is intentionally omitted: the Find row already has a close
            // button, and keeping the action row to two aligned buttons avoids
            // the long hint/Done tail wrapping under itself in narrow windows.
        });
    }

    fn replace_text(&mut self, all: bool) {
        if self.decision_pending() {
            return;
        }
        self.refresh_search();
        if all && self.search_results.limited {
            self.status = tr!(
                "Замена всё заблокирована: уточните запрос (лимит 10 000 совпадений)",
                "Replace all blocked: narrow the query (10,000 match limit)"
            )
            .into();
            return;
        }
        let hits = if all {
            self.search_results.hits.clone()
        } else {
            self.search_results
                .hits
                .get(self.current_match)
                .cloned()
                .into_iter()
                .collect()
        };
        let mut ranges = hits
            .into_iter()
            .filter_map(|hit| match hit.target {
                SearchTarget::Text => Some(hit.bytes),
                _ => None,
            })
            .collect::<Vec<_>>();
        if ranges.is_empty() {
            return;
        }
        ranges.sort_by_key(|range| std::cmp::Reverse(range.start));
        let Some(document) = self.text.as_ref() else {
            return;
        };
        let before = document.text.clone();
        let mut updated = before.clone();
        let mut replaced = 0usize;
        for range in ranges {
            if range.end > updated.len() {
                continue;
            }
            let mut candidate = updated.clone();
            candidate.replace_range(range, &self.replacement_text);
            if candidate != updated {
                updated = candidate;
                replaced += 1;
            }
        }
        if replaced == 0 {
            return;
        }
        if let Some(document) = self.text.as_mut() {
            document.text = updated;
        }
        self.record_text_edit(Some(before));
        self.suppress_text_history = true;
        self.search_dirty = true;
        self.current_match = 0;
        self.refresh_search();
        self.status = tfm!(
            "Заменено в тексте: {replaced}; доступна отмена",
            "Replaced in text: {replaced}; Undo available"
        );
    }

    fn replace_spreadsheet(&mut self, all: bool) {
        if self.decision_pending() {
            return;
        }
        self.refresh_search();
        let editable = self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.editor.is_some());
        if !editable {
            self.status = tr!(
                "Замена недоступна: книга не редактируется",
                "Replacement unavailable: the workbook is read-only"
            )
            .into();
            return;
        }
        if all && self.search_results.limited {
            self.status = tr!(
                "Замена всё заблокирована: уточните запрос (лимит 10 000 совпадений)",
                "Replace all blocked: narrow the query (10,000 match limit)"
            )
            .into();
            return;
        }
        let hits = if all {
            self.search_results.hits.clone()
        } else {
            self.search_results
                .hits
                .get(self.current_match)
                .cloned()
                .into_iter()
                .collect()
        };
        let mut per_cell: std::collections::BTreeMap<
            (usize, usize, usize),
            Vec<std::ops::Range<usize>>,
        > = std::collections::BTreeMap::new();
        for hit in hits {
            let SearchTarget::Cell { sheet, row, column } = hit.target else {
                continue;
            };
            per_cell
                .entry((sheet, row, column))
                .or_default()
                .push(hit.bytes);
        }
        if per_cell.is_empty() {
            return;
        }
        let before = self.workbook_snapshot();
        let mut replaced = 0usize;
        if let Some(book) = self.spreadsheet.as_mut() {
            for ((sheet, row, column), mut ranges) in per_cell {
                let Some(model) = book.sheets.get_mut(sheet) else {
                    continue;
                };
                let Some(cell) = model
                    .cells
                    .iter_mut()
                    .find(|cell| cell.row == row && cell.column == column)
                else {
                    continue;
                };
                ranges.sort_by_key(|range| std::cmp::Reverse(range.start));
                for range in ranges {
                    if range.end > cell.value.len() {
                        continue;
                    }
                    let mut value = cell.value.clone();
                    value.replace_range(range, &self.replacement_text);
                    if value != cell.value {
                        cell.value = value;
                        replaced += 1;
                    }
                }
            }
        }
        if replaced == 0 {
            self.status = tr!(
                "Совпадений для замены не осталось; книга не изменена",
                "No matches left to replace; workbook unchanged"
            )
            .into();
            return;
        }
        self.record_workbook_edit(before);
        self.search_dirty = true;
        self.current_match = 0;
        self.refresh_search();
        self.status = tfm!(
            "Заменено в книге: {replaced}; доступна отмена",
            "Replaced in workbook: {replaced}; Undo available"
        );
    }

    fn replace_docx(&mut self, all: bool) {
        if self.decision_pending() {
            return;
        }
        self.refresh_search();
        let Some(package) = self.docx.as_ref() else {
            self.status = tr!(
                "Замена сейчас доступна только для DOCX; документ не изменён",
                "Replacement is currently available for DOCX only; document unchanged"
            )
            .into();
            return;
        };
        let selection = if all {
            replace::Selection::All
        } else {
            replace::Selection::Current(self.current_match)
        };
        match replace::docx(
            &package.model,
            &self.search_query,
            &self.replacement_text,
            selection,
        ) {
            Ok(edit) => {
                let before = std::mem::replace(&mut self.docx.as_mut().unwrap().model, edit.model);
                let changed = self.docx.as_ref().unwrap().model != before;
                self.record_docx_edit(before);
                self.refresh_search();
                self.current_match = edit
                    .resume_from
                    .and_then(|(paragraph, offset)| {
                        self.search_results
                            .hits
                            .iter()
                            .position(|hit| match hit.target {
                                SearchTarget::DocxParagraph(index) => {
                                    index > paragraph
                                        || (index == paragraph && hit.bytes.start >= offset)
                                }
                                _ => false,
                            })
                    })
                    .unwrap_or(0);
                self.activate_search_hit();
                self.status = if changed {
                    tfm!(
                        "Заменено: {}; несохранённые правки DOCX — доступна отмена",
                        "Replaced {} occurrence(s); unsaved DOCX edits — Undo available",
                        edit.count
                    )
                } else {
                    tfm!(
                        "Совпадений: {}; документ не изменён",
                        "{} occurrence(s) matched; document unchanged",
                        edit.count
                    )
                };
            }
            Err(error) => {
                self.status = tfm!(
                    "Замена отменена: {error}; правки сохранены",
                    "Replacement cancelled: {error}; edits kept"
                )
            }
        }
    }

    fn show_cell_edit_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.cell_edit_dialog.take() else {
            return;
        };
        let mut apply = false;
        let mut cancel = false;
        let mut open = true;
        let id = egui::Id::new("workbook-cell-edit-dialog");
        let read_only = self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.editor.is_none());
        egui::Window::new(tr!("Изменить данные", "Edit data"))
            .id(id)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(tfm!(
                        "Ячейка {}{}",
                        "Cell {}{}",
                        spreadsheet_column_name(dialog.column),
                        dialog.row + 1
                    ))
                    .strong()
                    .size(14.0),
                );
                if read_only {
                    ui.label(tr!(
                        "Изменение создаст редактируемую копию книги; исходный файл не изменится.",
                        "Editing will create an editable workbook copy; the original file is not changed."
                    ));
                }
                let text_id = id.with("text");
                let enter_before = ui.memory(|memory| memory.has_focus(text_id))
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    });
                let response = ui.add(
                    egui::TextEdit::singleline(&mut dialog.text)
                        .id_source(text_id)
                        .desired_width(320.0)
                        .char_limit(32767),
                );
                if dialog.focus {
                    response.request_focus();
                    dialog.focus = false;
                }
                let enter_after = response.lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if enter_before || enter_after {
                    apply = true;
                }
                if ui.input_mut(|input| {
                    input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                }) {
                    cancel = true;
                }
                ui.horizontal(|ui| {
                    if ui.button(tr!("Сохранить", "Save")).clicked() {
                        apply = true;
                    }
                    if ui.button(tr!("Отмена", "Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        if apply {
            self.apply_cell_edit_dialog(dialog);
        } else if !cancel && open {
            self.cell_edit_dialog = Some(dialog);
        }
    }

    fn apply_cell_edit_dialog(&mut self, dialog: CellEditDialog) {
        if self.decision_pending() {
            return;
        }
        let before = self.workbook_snapshot();
        let read_only = self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.editor.is_none());
        if read_only {
            if self.opened_path.is_none() {
                self.status = tr!(
                    "Нельзя изменить данные: у книги нет пути для редактируемой копии",
                    "Cannot edit data: workbook has no path for an editable copy"
                )
                .into();
                return;
            }
            self.edit_workbook_copy();
        }
        let Some(book) = self.spreadsheet.as_mut() else {
            return;
        };
        book.selected_cell = Some((dialog.row, dialog.column));
        book.set_cell_text(dialog.row, dialog.column, dialog.text);
        book.focus_pending = false;
        self.record_workbook_edit(before);
        self.search_dirty = true;
        self.status = tfm!(
            "Ячейка {}{} изменена; Ctrl+Z отменяет",
            "Cell {}{} changed; Ctrl+Z undoes",
            spreadsheet_column_name(dialog.column),
            dialog.row + 1
        );
    }

    fn handle_search_shortcuts(&mut self, ctx: &egui::Context) {
        if self.decision_pending() || !self.has_document() || self.pdf.is_some() {
            return;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            self.docx_focus_pending = false;
            self.search_active = true;
            self.search_focus_requested = true;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::H)) {
            self.docx_focus_pending = false;
            self.search_active = true;
            self.replace_open = true;
            self.search_focus_requested = true;
        }
        let backwards = ctx.input(|input| input.modifiers.shift);
        let modifiers = if backwards {
            egui::Modifiers::SHIFT
        } else {
            egui::Modifiers::NONE
        };
        if ctx.input_mut(|input| input.consume_key(modifiers, egui::Key::F3)) {
            self.navigate_search(backwards);
        }
    }

    fn handle_table_clipboard_shortcuts(&mut self, ctx: &egui::Context) {
        if self.decision_pending() || ctx.wants_keyboard_input() {
            return;
        }
        let has_row_or_column = self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.selection.row.is_some() || book.selection.column.is_some());
        if !has_row_or_column {
            return;
        }
        if ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::CTRL, egui::Key::C)
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::C)
        }) {
            if let Some(text) = self
                .spreadsheet
                .as_mut()
                .and_then(SpreadsheetDocument::copy_selected_row_or_column)
            {
                ctx.copy_text(text);
                self.status = tr!(
                    "Строка/столбец скопированы; выберите строку или столбец и нажмите Ctrl+V",
                    "Row/column copied; select a row or column and press Ctrl+V"
                )
                .into();
            }
            return;
        }
        if ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::CTRL, egui::Key::V)
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::V)
        }) {
            if !self
                .spreadsheet
                .as_ref()
                .is_some_and(|book| book.editor.is_some())
            {
                return;
            }
            let before = self.workbook_snapshot();
            if self
                .spreadsheet
                .as_mut()
                .is_some_and(SpreadsheetDocument::paste_selected_row_or_column)
            {
                self.record_workbook_edit(before);
                self.search_dirty = true;
                self.status = tr!(
                    "Строка/столбец вставлены; Ctrl+Z отменяет",
                    "Row/column pasted; Ctrl+Z undoes"
                )
                .into();
            }
        }
    }

    fn handle_spreadsheet_undo_shortcut(&mut self, ctx: &egui::Context) {
        if self.decision_pending() {
            return;
        }
        let (editable, selection_active) = self
            .spreadsheet
            .as_ref()
            .map(|book| {
                (
                    book.editor.is_some(),
                    book.selection.row.is_some() || book.selection.column.is_some(),
                )
            })
            .unwrap_or((false, false));
        if !editable || !self.can_undo() || (!selection_active && ctx.wants_keyboard_input()) {
            return;
        }
        let shift = ctx.input(|input| input.modifiers.shift);
        if shift {
            return;
        }
        if ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::CTRL, egui::Key::Z)
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)
        }) {
            // The grid header selection can leave no text field focused. Handle
            // this before toolbar/search code so Ctrl+Z always reaches the app
            // history after a row/column Delete.
            self.undo_workbook();
        }
    }

    fn handle_spreadsheet_delete_shortcut(&mut self, ctx: &egui::Context) {
        if self.decision_pending() {
            return;
        }
        let has_selection = self.spreadsheet.as_ref().is_some_and(|book| {
            book.editor.is_some()
                && (book.selection.row.is_some() || book.selection.column.is_some())
        });
        if !has_selection || ctx.wants_keyboard_input() {
            return;
        }
        if !ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
            return;
        }
        let before = self.workbook_snapshot();
        let changed = self.spreadsheet.as_mut().is_some_and(|book| {
            if has_selection {
                book.delete_selected_row_or_column()
            } else {
                book.clear_active_cell()
            }
        });
        if changed {
            // This handler runs before the grid is drawn, so the snapshot is
            // guaranteed to be the state before Delete. The old in-grid delete
            // path mutated the workbook after the app snapshot and could leave
            // Ctrl+Z with no matching history entry.
            self.record_workbook_edit(before);
            self.search_dirty = true;
            self.status = tr!(
                "Строка/столбец очищены; Ctrl+Z вернёт ячейки",
                "Row/column cleared; Ctrl+Z restores the cells"
            )
            .into();
        }
    }

    fn handle_edit_shortcuts(&mut self, ctx: &egui::Context) {
        let editable_workbook = self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.editor.is_some());
        let editable_text = self.text.is_some();
        if self.decision_pending()
            || (self.docx.is_none() && !editable_workbook && !editable_text)
            || self.search_focus_requested
        {
            return;
        }
        let focused = ctx.memory(|memory| memory.focused());
        let toolbar_focused = focused.is_some_and(|id| {
            self.search_input_id == Some(id)
                || self.replacement_input_id == Some(id)
                || self.font_input_id == Some(id)
        });
        if toolbar_focused {
            // Ctrl+Z in the Replace field must undo the last document/table/text
            // replacement. Keeping it local to egui's field editor was the
            // reported bug, so route this explicit shortcut to the app history.
            if self.replacement_input_id == focused
                && self.can_undo()
                && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z))
            {
                self.undo();
            }
            return;
        }
        let redo_modifiers = egui::Modifiers {
            shift: true,
            ..egui::Modifiers::COMMAND
        };
        if ctx.input_mut(|input| {
            input.consume_key(redo_modifiers, egui::Key::Z)
                || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
        }) {
            self.redo();
        } else if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z)) {
            // Consume before the fragment editor: one app transaction, not a
            // second independent TextEdit undo of a stale per-run buffer.
            self.undo();
        }
    }

    fn handle_docx_selection_shortcuts(&mut self, ctx: &egui::Context) {
        if self.decision_pending()
            || self.docx.is_none()
            || self.search_focus_requested
            || self.toolbar_text_field_is_focused(ctx)
        {
            return;
        }
        if ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::A)) {
            self.select_all_docx = true;
            self.status = tr!(
                "Выделен весь текст DOCX (включая таблицы); Delete удаляет выделенный текст",
                "All DOCX text selected (including tables); Delete removes selected text"
            )
            .into();
        }
        let modifiers = ctx.input(|input| input.modifiers);
        if self.select_all_docx
            && !modifiers.alt
            && ctx.input_mut(|input| {
                input.consume_key(modifiers, egui::Key::Delete)
                    || input.consume_key(modifiers, egui::Key::Backspace)
            })
        {
            let package = self.docx.as_mut().unwrap();
            let before = package.model.clone();
            for fragment in package
                .model
                .paragraphs
                .iter_mut()
                .flat_map(|p| &mut p.fragments)
            {
                fragment.text.clear();
            }
            self.record_docx_edit(before);
            self.select_all_docx = false;
            self.selected_fragment_id = None;
            self.status = tr!(
                "Выделенный текст DOCX удалён; изображения и таблицы сохранены — доступна отмена",
                "Selected DOCX text deleted; images/tables kept — Undo available"
            )
            .into();
        }
    }

    fn prepare_docx_focus(&self, ctx: &egui::Context) {
        if !self.docx_focus_pending || self.decision_pending() || !ctx.input(|input| input.focused)
        {
            return;
        }
        if let Some(fragment) = self.docx.as_ref().and_then(|package| {
            package
                .model
                .paragraphs
                .iter()
                .flat_map(|paragraph| &paragraph.fragments)
                .next()
        }) {
            ctx.memory_mut(|memory| {
                memory.request_focus(docx_editor_id(self.docx_editor_generation, fragment.id))
            });
        }
    }

    fn initialize_recovery(&mut self, root: PathBuf) {
        match liteoffice::recovery::discover(&root) {
            Ok((sessions, warnings)) => {
                self.recovery_candidates = sessions;
                self.recovery_notice = warnings.join("; ");
            }
            Err(error) => {
                self.recovery_notice = tfm!(
                    "Не удалось найти файлы восстановления; файлы сохранены: {error}",
                    "Recovery discovery failed; files kept: {error}"
                )
            }
        }
        self.recovery_root = Some(root);
    }

    fn sync_recovery(&mut self, force: bool) {
        if self.close_approved || self.recovery_root.is_none() {
            return;
        }
        if self.text.is_some() {
            self.sync_text_recovery(force);
            return;
        }
        if self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.editor.is_some())
        {
            self.sync_workbook_recovery(force);
            return;
        }
        let Some(package) = self.docx.as_ref() else {
            return;
        };
        if !self.is_dirty() && self.recovery_session.is_none() {
            return;
        }
        if self.recovery_snapshot_model.as_ref() == Some(&package.model) {
            if recovery_notice_pending(&self.recovery_notice)
                || recovery_notice_failed(&self.recovery_notice)
            {
                self.recovery_notice = tr!(
                    "Резервная копия соответствует текущим правкам; оригинал не изменён",
                    "Recovery copy matches current edits; original unchanged"
                )
                .into();
            }
            return;
        }
        let interval = std::time::Duration::from_secs(2);
        if !force
            && self
                .recovery_last_attempt
                .is_some_and(|last| last.elapsed() < interval)
        {
            if !recovery_notice_failed(&self.recovery_notice) {
                self.recovery_notice = tr!(
                    "Ожидание восстановления: последние правки ещё не записаны",
                    "Recovery pending: latest edits not checkpointed yet"
                )
                .into();
            }
            return;
        }
        self.recovery_last_attempt = Some(std::time::Instant::now());
        let result = (|| {
            if self.recovery_session.is_none() {
                self.recovery_session = Some(liteoffice::recovery::Session::create(
                    self.recovery_root.as_ref().unwrap(),
                    self.opened_path
                        .as_deref()
                        .unwrap_or_else(|| package.source_path()),
                )?);
            }
            self.recovery_session.as_mut().unwrap().checkpoint(package)
        })();
        match result {
            Ok(()) => {
                self.recovery_snapshot_model = Some(package.model.clone());
                self.recovery_notice = tr!("Резервная копия создана; оригинал не изменён", "Recovery copy created; original unchanged").into();
            }
            Err(error) => self.recovery_notice = tfm!("ОШИБКА записи восстановления; правки только в памяти, предыдущая точка сохранена: {error}", "Recovery write FAILED; current edits are only in memory, earlier checkpoint kept: {error}"),
        }
    }

    fn clear_current_recovery(&mut self) -> bool {
        if let Some(session) = self.recovery_session.as_ref() {
            if let Err(error) = session.discard() {
                self.recovery_notice = tfm!(
                    "Не удалось очистить восстановление; файлы сохранены: {error}",
                    "Recovery cleanup failed; files kept: {error}"
                );
                return false;
            }
        }
        self.recovery_session = None;
        self.recovery_snapshot_model = None;
        self.recovery_snapshot_text = None;
        self.recovery_snapshot_workbook = None;
        self.recovery_last_attempt = None;
        self.recovery_notice.clear();
        if let Some(root) = self.recovery_root.as_deref() {
            let _ = liteoffice::recovery::prune_empty_sessions(root);
        }
        true
    }

    fn restore_first_recovery(&mut self) -> bool {
        let Some(session) = self.recovery_candidates.first() else {
            return false;
        };
        if session.is_text() {
            return self.restore_first_text_recovery();
        }
        if session.is_workbook() {
            return self.restore_first_workbook_recovery();
        }
        match session.restore() {
            Ok((package, fallback)) => {
                // Startup choices are modal; no unsaved active document may be
                // silently replaced. Other candidates remain on disk, unlocked
                // for another independent window or the next launch.
                if self.is_dirty() {
                    return false;
                }
                self.opened_path = Some(PathBuf::from(&session.origin));
                self.text = None;
                self.text_input_id = None;
                self.text_focus_pending = false;
                self.pdf_print_open = false;
                self.saved_docx_model = Some(package.model.clone());
                self.recovery_snapshot_model = Some(package.model.clone());
                self.recovery_snapshot_text = None;
                self.docx = Some(package);
                self.pdf = None;
                self.spreadsheet = None;
                self.legacy_doc = None;
                self.docx_textures.clear();
                self.legacy_doc_textures.clear();
                self.doc_image_anchors.clear();
                self.doc_image_widths.clear();
                self.undo_stack.clear();
                self.workbook_undo.clear();
                self.workbook_redo.clear();
                self.redo_stack.clear();
                self.selected_fragment_id = None;
                self.docx_editor_generation = self.docx_editor_generation.wrapping_add(1);
                self.docx_focus_pending = true;
                self.docx_view_clip_size = None;
                self.select_all_docx = false;
                self.replace_open = false;
                self.replacement_input_id = None;
                self.current_match = 0;
                self.search_results = search::Results::default();
                self.search_dirty = true;
                self.search_scroll_requested = !self.search_query.trim().is_empty();
                self.recovery_session = Some(self.recovery_candidates.remove(0));
                self.recovery_candidates.clear();
                self.recovered_document = true;
                self.recovery_discard_confirmation = false;
                self.recovery_notice = if fallback {
                    tr!("Восстановлена ПРЕДЫДУЩАЯ корректная резервная копия (новая повреждена). Требуется «Сохранить как»; оригинал не изменён", "Recovered PREVIOUS valid recovery copy (newest was damaged). Save As required; original unchanged")
                } else {
                    tr!("Восстановлен несохранённый DOCX. Требуется «Сохранить как»; оригинал не изменён", "Recovered unsaved DOCX. Save As required; original unchanged")
                }.into();
                self.status = self.recovery_notice.clone();
                true
            }
            Err(error) => {
                self.recovery_notice = tfm!(
                    "Восстановление не удалось; файлы сохранены: {error}",
                    "Recovery failed; files kept: {error}"
                );
                false
            }
        }
    }

    fn discard_first_recovery(&mut self) -> bool {
        let Some(session) = self.recovery_candidates.first() else {
            return false;
        };
        if let Err(error) = session.discard() {
            self.recovery_notice = tfm!(
                "Не удалось удалить восстановление: {error}",
                "Recovery discard failed: {error}"
            );
            return false;
        }
        self.recovery_candidates.remove(0);
        if let Some(root) = self.recovery_root.as_deref() {
            let _ = liteoffice::recovery::prune_empty_sessions(root);
        }
        self.recovery_discard_confirmation = false;
        self.recovery_notice = tr!(
            "Выбранное восстановление удалено по вашему решению; оригинал не изменён",
            "Selected recovery discarded by your explicit choice; original unchanged"
        )
        .into();
        true
    }

    fn sync_workbook_recovery(&mut self, force: bool) {
        let Some(book) = self.spreadsheet.as_ref() else {
            return;
        };
        if book.editor.is_none() {
            return;
        }
        let sheets = book.sheets.clone();
        let formats = book.formats.clone();
        if !self.is_dirty() && self.recovery_session.is_none() {
            return;
        }
        if self.recovery_snapshot_workbook.as_ref().is_some_and(
            |(snapshot_sheets, snapshot_formats)| {
                *snapshot_sheets == sheets && *snapshot_formats == formats
            },
        ) {
            if recovery_notice_pending(&self.recovery_notice)
                || recovery_notice_failed(&self.recovery_notice)
            {
                self.recovery_notice = tr!(
                    "Резервная копия соответствует текущим правкам; оригинал не изменён",
                    "Recovery copy matches current edits; original unchanged"
                )
                .into();
            }
            return;
        }
        if !force
            && self
                .recovery_last_attempt
                .is_some_and(|last| last.elapsed() < std::time::Duration::from_secs(2))
        {
            if !recovery_notice_failed(&self.recovery_notice) {
                self.recovery_notice = tr!(
                    "Ожидание восстановления: правки книги ещё не записаны",
                    "Recovery pending: workbook edits not checkpointed yet"
                )
                .into();
            }
            return;
        }
        let Some(origin) = self.opened_path.clone() else {
            self.recovery_notice = tr!(
                "Восстановление книги недоступно: у документа нет пути",
                "Workbook recovery unavailable: the document has no path"
            )
            .into();
            return;
        };
        self.recovery_last_attempt = Some(std::time::Instant::now());
        let result = (|| {
            if self.recovery_session.is_none() {
                self.recovery_session = Some(liteoffice::recovery::Session::create_workbook(
                    self.recovery_root.as_ref().unwrap(),
                    &origin,
                )?);
            }
            self.recovery_session
                .as_mut()
                .unwrap()
                .checkpoint_workbook(&sheets, &formats)
        })();
        match result {
            Ok(()) => {
                self.recovery_snapshot_workbook = Some((sheets, formats));
                self.recovery_notice = tr!(
                    "Резервная копия книги создана; оригинал не изменён",
                    "Workbook recovery copy created; original unchanged"
                )
                .into();
            }
            Err(error) => {
                self.recovery_notice = tfm!(
                    "ОШИБКА записи восстановления книги; правки только в памяти, предыдущая точка сохранена: {error}",
                    "Workbook recovery write FAILED; edits are only in memory, earlier checkpoint kept: {error}"
                )
            }
        }
    }

    fn restore_first_workbook_recovery(&mut self) -> bool {
        if self.is_dirty() {
            return false;
        }
        let session = &self.recovery_candidates[0];
        let origin = PathBuf::from(&session.origin);
        let (sheets, formats, fallback) = match session.restore_workbook() {
            Ok(value) => value,
            Err(error) => {
                self.recovery_notice = tfm!(
                    "Восстановление книги не удалось; файлы сохранены: {error}",
                    "Workbook recovery failed; files kept: {error}"
                );
                return false;
            }
        };
        let mut book = SpreadsheetDocument::from_sheets(sheets.clone());
        book.formats = formats;
        book.selected_sheet = 0;
        book.selected_cell = None;
        book.preview_anchor = None;
        book.input_id = None;
        book.focus_pending = false;
        book.editor = Some(liteoffice::workbook_edit::WorkbookEditor::new(
            origin.clone(),
            &sheets,
        ));
        self.opened_path = origin.is_absolute().then_some(origin);
        self.spreadsheet = Some(book);
        self.text = None;
        self.docx = None;
        self.pdf = None;
        self.legacy_doc = None;
        self.saved_docx_model = None;
        self.recovery_snapshot_model = None;
        self.recovery_snapshot_text = None;
        self.recovery_snapshot_workbook =
            Some((sheets, self.spreadsheet.as_ref().unwrap().formats.clone()));
        self.docx_textures.clear();
        self.legacy_doc_textures.clear();
        self.doc_image_anchors.clear();
        self.doc_image_widths.clear();
        self.undo_stack.clear();
        self.workbook_undo.clear();
        self.workbook_redo.clear();
        self.text_undo.clear();
        self.text_redo.clear();
        self.redo_stack.clear();
        self.selected_fragment_id = None;
        self.docx_editor_generation = self.docx_editor_generation.wrapping_add(1);
        self.docx_focus_pending = false;
        self.docx_view_clip_size = None;
        self.text_focus_pending = false;
        self.text_input_id = None;
        self.search_results = search::Results::default();
        self.search_dirty = true;
        self.current_match = 0;
        self.search_focus_requested = false;
        self.replace_open = false;
        self.replacement_input_id = None;
        self.pdf_print_open = false;
        self.pending_action = None;
        self.conflict_destination = None;
        self.close_approved = false;
        self.recovery_session = Some(self.recovery_candidates.remove(0));
        self.recovery_candidates.clear();
        self.recovery_last_attempt = None;
        self.recovered_document = true;
        self.recovery_discard_confirmation = false;
        self.recovery_notice = if fallback {
            tr!(
                "Восстановлена ПРЕДЫДУЩАЯ корректная резервная копия книги (новая повреждена). Требуется «Сохранить как XLSX»; оригинал не изменён",
                "Recovered PREVIOUS valid workbook checkpoint (newest damaged). Save As XLSX required; original unchanged"
            )
        } else {
            tr!(
                "Восстановлена несохранённая книга. Требуется «Сохранить как XLSX»; оригинал не изменён",
                "Recovered unsaved workbook. Save As XLSX required; original unchanged"
            )
        }
        .into();
        self.status = self.recovery_notice.clone();
        true
    }

    fn sync_text_recovery(&mut self, force: bool) {
        let Some(document) = self.text.as_ref() else {
            return;
        };
        if !self.is_dirty() && self.recovery_session.is_none() {
            return;
        }
        if self
            .recovery_snapshot_text
            .as_ref()
            .is_some_and(|(kind, text)| *kind == document.kind && *text == document.text)
        {
            if recovery_notice_pending(&self.recovery_notice)
                || recovery_notice_failed(&self.recovery_notice)
            {
                self.recovery_notice = tr!(
                    "Резервная копия соответствует текущим правкам; оригинал не изменён",
                    "Recovery copy matches current edits; original unchanged"
                )
                .into();
            }
            return;
        }
        if !force
            && self
                .recovery_last_attempt
                .is_some_and(|last| last.elapsed() < std::time::Duration::from_secs(2))
        {
            if !recovery_notice_failed(&self.recovery_notice) {
                self.recovery_notice = tr!(
                    "Ожидание восстановления: последний текст ещё не записан",
                    "Recovery pending: latest text not checkpointed yet"
                )
                .into();
            }
            return;
        }
        self.recovery_last_attempt = Some(std::time::Instant::now());
        let result = (|| {
            if self.recovery_session.is_none() {
                self.recovery_session = Some(liteoffice::recovery::Session::create_text(
                    self.recovery_root.as_ref().unwrap(),
                    document.path(),
                )?);
            }
            self.recovery_session
                .as_mut()
                .unwrap()
                .checkpoint_text(document)
        })();
        match result {
            Ok(())=>{self.recovery_snapshot_text=Some((document.kind,document.text.clone()));self.recovery_notice=tr!("Резервная копия создана; оригинал не изменён", "Recovery copy created; original unchanged").into();}
            Err(error)=>self.recovery_notice=tfm!("ОШИБКА записи восстановления; текст остаётся в окне, предыдущая точка сохранена: {error}", "Recovery write FAILED; latest text remains in this window, previous checkpoint kept: {error}"),
        }
    }

    fn restore_first_text_recovery(&mut self) -> bool {
        if self.is_dirty() {
            return false;
        }
        let session = &self.recovery_candidates[0];
        let origin = PathBuf::from(&session.origin);
        let (document, fallback) = match session.restore_text() {
            Ok(value) => value,
            Err(error) => {
                self.recovery_notice = tfm!(
                    "Восстановление текста не удалось; файлы сохранены: {error}",
                    "Text recovery failed; files kept: {error}"
                );
                return false;
            }
        };
        self.opened_path = origin.is_absolute().then_some(origin);
        self.recovery_snapshot_text = Some((document.kind, document.text.clone()));
        self.text = Some(document);
        self.docx = None;
        self.pdf = None;
        self.spreadsheet = None;
        self.legacy_doc = None;
        self.saved_docx_model = None;
        self.recovery_snapshot_model = None;
        self.docx_textures.clear();
        self.legacy_doc_textures.clear();
        self.doc_image_anchors.clear();
        self.doc_image_widths.clear();
        self.undo_stack.clear();
        self.workbook_undo.clear();
        self.workbook_redo.clear();
        self.text_undo.clear();
        self.text_redo.clear();
        self.redo_stack.clear();
        self.selected_fragment_id = None;
        self.docx_editor_generation = self.docx_editor_generation.wrapping_add(1);
        self.docx_focus_pending = false;
        self.docx_view_clip_size = None;
        self.text_focus_pending = true;
        self.text_input_id = None;
        self.search_results = search::Results::default();
        self.search_dirty = true;
        self.current_match = 0;
        self.search_focus_requested = false;
        self.replace_open = false;
        self.replacement_input_id = None;
        self.pdf_print_open = false;
        self.pending_action = None;
        self.conflict_destination = None;
        self.close_approved = false;
        self.recovery_session = Some(self.recovery_candidates.remove(0));
        self.recovery_candidates.clear();
        self.recovery_last_attempt = None;
        self.recovered_document = true;
        self.recovery_discard_confirmation = false;
        self.recovery_notice=if fallback {tr!("Восстановлена ПРЕДЫДУЩАЯ корректная текстовая резервная копия (новая повреждена). Требуется «Сохранить как»; оригинал не изменён", "Recovered PREVIOUS valid text recovery copy (newest damaged). Save As copy required; original unchanged")}
            else {tr!("Восстановлен несохранённый текст. Требуется «Сохранить как»; оригинал не изменён", "Recovered unsaved text. Save As copy required; original unchanged")}.into();
        self.status = self.recovery_notice.clone();
        true
    }

    fn open_new_blank_window(&mut self) {
        match spawn_blank_text_window() {
            Ok(()) => {
                self.status = tr!(
                    "Открыто новое независимое окно NotOffice",
                    "Opened a new independent NotOffice window"
                )
                .into()
            }
            Err(error) => {
                self.status = tfm!(
                    "Не удалось открыть новое окно: {error}",
                    "New window failed: {error}"
                )
            }
        }
    }

    fn open_path_in_new_window(&mut self, path: PathBuf) {
        match spawn_liteoffice_window(Some(&path)) {
            Ok(()) => {
                self.status = tfm!(
                    "Файл {} открыт в новом независимом окне",
                    "Opened {} in a new independent window",
                    path.display()
                )
            }
            Err(error) => {
                self.status = tfm!(
                    "Не удалось открыть в новом окне: {error}",
                    "Open in new window failed: {error}"
                )
            }
        }
    }

    fn open_document_in_new_window(&mut self) {
        let path = rfd::FileDialog::new()
            .add_filter(
                tr!("Документ", "Document"),
                &[
                    "docx", "doc", "xlsx", "xls", "xlsb", "ods", "csv", "tsv", "pdf", "txt", "md",
                    "rtf",
                ],
            )
            .pick_file();
        match path {
            Some(path) => match spawn_liteoffice_window(Some(&path)) {
                Ok(()) => {
                    self.status = tfm!(
                        "Файл {} открыт в новом независимом окне",
                        "Opened {} in a new independent window",
                        path.display()
                    )
                }
                Err(error) => {
                    self.status = tfm!(
                        "Не удалось открыть в новом окне: {error}",
                        "Open in new window failed: {error}"
                    )
                }
            },
            None => {
                self.status = tr!(
                    "Открытие в новом окне отменено: файл не выбран",
                    "Open in new window cancelled: no file was selected"
                )
                .into()
            }
        }
    }

    fn window_title(&self) -> String {
        let document_name = self
            .opened_path
            .as_deref()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                if self.text.is_some() {
                    "Untitled.txt".into()
                } else {
                    tr!("Новый документ", "New document").into()
                }
            });
        let dirty = if self.is_dirty() { "* " } else { "" };
        format!(
            "{dirty}{document_name} — NotOffice v{}",
            env!("CARGO_PKG_VERSION")
        )
    }

    fn toolbar_text_field_is_focused(&self, ctx: &egui::Context) -> bool {
        let focused = ctx.memory(|memory| memory.focused());
        focused.is_some_and(|id| {
            self.search_input_id == Some(id)
                || self.replacement_input_id == Some(id)
                || self.font_input_id == Some(id)
        })
    }

    fn apply_ctrl_wheel_document_zoom(&mut self, ctx: &egui::Context) {
        let pointer_over_document = ctx.input(|input| {
            input
                .pointer
                .hover_pos()
                .zip(self.document_view_rect)
                .is_some_and(|(pointer, rect)| rect.contains(pointer))
        });
        let (ctrl_held, zoom_factor) =
            ctx.input(|input| (input.modifiers.command, input.zoom_delta()));
        let text_input_or_dialog_active = ctx.wants_keyboard_input() || self.decision_pending();
        let Some(next_zoom) = ctrl_wheel_document_zoom(
            self.zoom,
            ctrl_held,
            self.has_document() && pointer_over_document,
            text_input_or_dialog_active,
            zoom_factor,
        ) else {
            return;
        };

        if (next_zoom - self.zoom).abs() >= f32::EPSILON {
            self.zoom = next_zoom;
            self.status = tfm!(
                "Масштаб: {}%",
                "Document zoom: {}%",
                (self.zoom * 100.0).round() as u32
            );
        }
    }

    fn open_document(&mut self) {
        let path = rfd::FileDialog::new()
            .add_filter(
                tr!("Документ", "Document"),
                &[
                    "docx", "doc", "xlsx", "xls", "xlsb", "ods", "csv", "tsv", "pdf", "txt", "md",
                    "rtf",
                ],
            )
            .pick_file();
        match path {
            Some(path) => self.request_open_path(path),
            None => {
                self.open_diagnostics.clear();
                self.record_open_stage(
                    tr!(
                        "Открытие отменено: файл не выбран",
                        "Open cancelled: no file was selected"
                    )
                    .into(),
                );
            }
        };
    }

    fn has_document(&self) -> bool {
        self.text.is_some()
            || self.docx.is_some()
            || self.spreadsheet.is_some()
            || self.legacy_doc.is_some()
            || self.pdf.is_some()
    }

    fn is_dirty(&self) -> bool {
        self.text.as_ref().is_some_and(|d| d.is_dirty())
            || self.recovered_document
            || self
                .spreadsheet
                .as_ref()
                .is_some_and(SpreadsheetDocument::is_dirty)
            || self
                .docx
                .as_ref()
                .is_some_and(DocxPackage::requires_save_as)
            || self
                .docx
                .as_ref()
                .zip(self.saved_docx_model.as_ref())
                .is_some_and(|(package, saved)| package.model != *saved)
    }

    fn remember_recent(&mut self, path: PathBuf) {
        liteoffice::recent::remember_path(&mut self.recent_paths, path);
        if let Some(store_path) = recent_store_path() {
            let _ = liteoffice::recent::save_paths(&store_path, &self.recent_paths);
        }
    }

    fn record_open_stage(&mut self, message: String) {
        const OPEN_DIAGNOSTIC_LIMIT: usize = 8;
        self.status = message.clone();
        self.open_diagnostics.push(message);
        if self.open_diagnostics.len() > OPEN_DIAGNOSTIC_LIMIT {
            let excess = self.open_diagnostics.len() - OPEN_DIAGNOSTIC_LIMIT;
            self.open_diagnostics.drain(..excess);
        }
    }

    fn open_failed(&mut self, stage: &str, error: impl std::fmt::Display) {
        self.record_open_stage(tfm!(
            "Не удалось открыть на этапе «{stage}»: {error}",
            "Open failed during {stage}: {error}"
        ));
    }

    fn finish_open(&mut self, path: &Path, description: &str) {
        if !matches!(
            file_ops::document_kind(path),
            Ok(DocumentKind::Txt | DocumentKind::Markdown | DocumentKind::Rtf)
        ) {
            self.text = None;
        }
        self.text_focus_pending = self.text.is_some();
        self.text_input_id = None;
        self.pdf_print_open = false;
        self.pdf_print_settings = liteoffice::pdf_print::PdfPrintSettings {
            first: 1,
            last: self.pdf.as_ref().map(|v| v.page_count()).unwrap_or(1),
            percent: None,
        };
        if file_ops::document_kind(path) != Ok(DocumentKind::Pdf) {
            self.pdf = None;
        }
        // Reached only after a successful replacement-open (including explicit
        // Don't save / Reload). Failed opens leave the previous recovery intact.
        if !self.clear_current_recovery() {
            // Keep the old files for later discovery, but never checkpoint a
            // different document into that old document's session.
            self.recovery_session = None;
            self.recovery_snapshot_model = None;
            self.recovery_snapshot_text = None;
            self.recovery_snapshot_workbook = None;
            self.recovery_last_attempt = None;
        }
        self.recovered_document = false;
        // These fields only belong to the currently displayed document.  They
        // are reset after, never before, a new package has loaded successfully.
        self.selected_fragment_id = None;
        self.docx_editor_generation = self.docx_editor_generation.wrapping_add(1);
        self.docx_focus_pending = self.docx.is_some();
        self.docx_view_clip_size = None;
        self.select_all_docx = false;
        self.replace_open = false;
        self.replacement_input_id = None;
        self.current_match = 0;
        self.search_results = search::Results::default();
        self.search_dirty = true;
        self.search_scroll_requested = !self.search_query.trim().is_empty();
        self.undo_stack.clear();
        self.workbook_undo.clear();
        self.workbook_redo.clear();
        self.text_undo.clear();
        self.text_redo.clear();
        self.redo_stack.clear();
        self.pending_action = None;
        self.conflict_destination = None;
        self.close_approved = false;
        self.remember_recent(path.to_path_buf());
        self.record_open_stage(tfm!(
            "Открыто: {description}; активная модель заменена, интерфейс обновлён для {}",
            "Open complete: {description}; active model replaced and UI updated for {}",
            path.display()
        ));
    }

    fn request_open_path(&mut self, path: PathBuf) {
        if self.decision_pending() {
            return;
        }
        self.open_diagnostics.clear();
        self.record_open_stage(tfm!(
            "Открытие: выбран файл {}",
            "Open: file selected: {}",
            path.display()
        ));
        if self.is_dirty() {
            self.pending_action = Some(PendingAction::Open(path));
            self.record_open_stage(
                tr!("Открытие: обнаружены несохранённые правки DOCX; ждём выбора перед загрузкой нового файла", "Open: unsaved DOCX edits detected; waiting for your choice before loading the new file")
                    .into(),
            );
        } else {
            self.record_open_stage(
                tr!(
                    "Открытие: несохранённых правок DOCX нет; проверяем тип документа",
                    "Open: no unsaved DOCX edits; checking document type"
                )
                .into(),
            );
            self.open_path(path);
        }
    }

    fn decision_pending(&self) -> bool {
        self.pdf_print_open
            || self.pending_action.is_some()
            || self.conflict_destination.is_some()
            || !self.recovery_candidates.is_empty()
    }

    fn request_close(&mut self) -> bool {
        if self.close_approved || !self.is_dirty() {
            if self.text.is_some() {
                let _ = self.clear_current_recovery();
            }
            return true;
        }
        self.pending_action = Some(PendingAction::Close);
        self.status = tr!(
            "Несохранённые правки: сохраните, откажитесь или отмените закрытие",
            "Unsaved document edits: save, discard, or cancel closing"
        )
        .into();
        false
    }

    fn cancel_pending_action(&mut self) {
        self.pending_action = None;
        self.conflict_destination = None;
        self.close_approved = false;
        self.status = tr!(
            "Отменено: документ и несохранённые правки сохранены",
            "Cancelled: document and unsaved edits were kept"
        )
        .into();
    }

    fn resume_pending_action(&mut self, ctx: &egui::Context) {
        match self.pending_action.take() {
            Some(PendingAction::Open(path)) => self.open_path(path),
            Some(PendingAction::Close) => {
                if !self.clear_current_recovery() {
                    self.pending_action = Some(PendingAction::Close);
                    return;
                }
                self.close_approved = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            None => {}
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        let kind = match file_ops::document_kind(&path) {
            Ok(kind) => kind,
            Err(error) => {
                self.open_failed(tr!("проверка типа документа", "document-type check"), error);
                return;
            }
        };
        match kind {
            DocumentKind::Txt | DocumentKind::Markdown | DocumentKind::Rtf => {
                match liteoffice::text_document::TextDocument::open(&path) {
                    Ok(document) => {
                        self.text = Some(document);
                        self.opened_path = Some(path.clone());
                        self.docx = None;
                        self.pdf = None;
                        self.spreadsheet = None;
                        self.legacy_doc = None;
                        self.docx_textures.clear();
                        self.legacy_doc_textures.clear();
                        self.saved_docx_model = None;
                        self.finish_open(
                            &path,
                            tr!(
                                "текст загружен; Markdown — исходник, RTF — только текст",
                                "text loaded; Markdown is literal source, RTF is text-only"
                            ),
                        );
                    }
                    Err(error) => self.open_failed(tr!("загрузка текста", "text loading"), error),
                }
            }
            DocumentKind::Pdf => match pdf_view::PdfView::open(&path) {
                Ok(view) => {
                    self.pdf = Some(view);
                    self.opened_path = Some(path.clone());
                    self.docx = None;
                    self.spreadsheet = None;
                    self.legacy_doc = None;
                    self.docx_textures.clear();
                    self.legacy_doc_textures.clear();
                    self.saved_docx_model = None;
                    self.finish_open(
                        &path,
                        tr!(
                            "PDF загружен только для чтения; действия документа не выполняются",
                            "PDF loaded read-only; document actions are not executed"
                        ),
                    );
                }
                Err(error) => self.open_failed(tr!("загрузка PDF", "PDF loading"), error),
            },
            DocumentKind::Docx => {
                self.record_open_stage(
                    tr!(
                        "Открытие: загрузка пакета DOCX",
                        "Open: loading DOCX package"
                    )
                    .into(),
                );
                match DocxPackage::open(&path) {
                    Ok(package) => {
                        self.record_open_stage(
                            tr!(
                                "Открытие: DOCX загружен; заменяем активную модель",
                                "Open: DOCX loaded; replacing active model"
                            )
                            .into(),
                        );
                        self.opened_path = Some(path.clone());
                        self.docx = Some(package);
                        self.docx_textures.clear();
                        self.spreadsheet = None;
                        self.legacy_doc = None;
                        self.legacy_doc_textures.clear();
                        self.doc_image_anchors.clear();
                        self.doc_image_widths.clear();
                        self.saved_docx_model =
                            self.docx.as_ref().map(|package| package.model.clone());
                        self.finish_open(&path, tr!("DOCX загружен", "DOCX loaded"));
                    }
                    Err(error) => self.open_failed(tr!("загрузка DOCX", "DOCX loading"), error),
                }
            }
            DocumentKind::Xlsx => {
                self.record_open_stage(
                    tr!(
                        "Открытие: загрузка пакета XLSX",
                        "Open: loading XLSX package"
                    )
                    .into(),
                );
                match XlsxPackage::open(&path) {
                    Ok(package) => {
                        self.record_open_stage(
                            tr!(
                                "Открытие: XLSX загружен; заменяем активную модель",
                                "Open: XLSX loaded; replacing active model"
                            )
                            .into(),
                        );
                        self.opened_path = Some(path.clone());
                        let mut book = SpreadsheetDocument::from_sheets(package.sheets);
                        book.formats = package.formats;
                        self.spreadsheet = Some(book);
                        self.docx = None;
                        self.legacy_doc = None;
                        self.legacy_doc_textures.clear();
                        self.doc_image_anchors.clear();
                        self.doc_image_widths.clear();
                        self.saved_docx_model = None;
                        self.finish_open(
                            &path,
                            tr!(
                                "XLSX: загружен просмотр первого листа",
                                "XLSX first-worksheet preview loaded"
                            ),
                        );
                    }
                    Err(error) => self.open_failed(tr!("загрузка XLSX", "XLSX loading"), error),
                }
            }
            DocumentKind::Xls | DocumentKind::Xlsb | DocumentKind::Ods => {
                self.record_open_stage(
                    tr!("Открытие: загрузка таблицы", "Open: loading spreadsheet").into(),
                );
                match XlsPackage::open(&path) {
                    Ok(package) => {
                        self.record_open_stage(
                            tr!(
                                "Открытие: таблица загружена; заменяем активную модель",
                                "Open: spreadsheet loaded; replacing active model"
                            )
                            .into(),
                        );
                        self.opened_path = Some(path.clone());
                        self.spreadsheet = Some(SpreadsheetDocument::from_sheets(package.sheets));
                        self.docx = None;
                        self.legacy_doc = None;
                        self.legacy_doc_textures.clear();
                        self.doc_image_anchors.clear();
                        self.doc_image_widths.clear();
                        self.saved_docx_model = None;
                        self.finish_open(
                            &path,
                            tr!(
                                "Таблица: загружен просмотр первого листа",
                                "Spreadsheet first-worksheet preview loaded"
                            ),
                        );
                    }
                    Err(error) => {
                        self.open_failed(tr!("загрузка таблицы", "spreadsheet loading"), error)
                    }
                }
            }
            DocumentKind::Csv => match liteoffice::csv_import::open(&path) {
                Ok(model) => {
                    self.opened_path = Some(path.clone());
                    self.spreadsheet = Some(SpreadsheetDocument::from_sheets(vec![model]));
                    self.docx = None;
                    self.legacy_doc = None;
                    self.legacy_doc_textures.clear();
                    self.doc_image_anchors.clear();
                    self.doc_image_widths.clear();
                    self.saved_docx_model = None;
                    self.finish_open(
                        &path,
                        tr!(
                            "CSV/TSV: загружен просмотр таблицы",
                            "CSV/TSV table preview loaded"
                        ),
                    );
                }
                Err(error) => self.open_failed(tr!("загрузка CSV/TSV", "CSV/TSV loading"), error),
            },
            DocumentKind::Doc => {
                self.record_open_stage(
                    tr!(
                        "Открытие: загрузка старого пакета DOC",
                        "Open: loading legacy DOC package"
                    )
                    .into(),
                );
                match LegacyDocPackage::open(&path) {
                    Ok(package) => {
                        let paragraph_count = package.paragraphs.len();
                        self.record_open_stage(tfm!(
                        "Открытие: DOC загружен ({paragraph_count} восстановленных абзацев); заменяем активную модель", "Open: DOC loaded ({paragraph_count} recovered text paragraphs); replacing active model"
                    ));
                        self.opened_path = Some(path.clone());
                        let image_count = package.images.len();
                        self.legacy_doc_textures.clear();
                        self.doc_image_anchors = vec![None; image_count];
                        self.doc_image_widths = vec![85.0; image_count];
                        match legacy_sidecar::load(&path, paragraph_count, image_count) {
                            Ok(Some((anchors, widths))) => {
                                self.doc_image_anchors = anchors;
                                self.doc_image_widths = widths;
                                self.record_open_stage(
                                    tr!("Открытие: сохранённая разметка изображений DOC восстановлена", "Open: saved DOC image layout restored").into(),
                                );
                            }
                            Ok(None) => {}
                            Err(reason) => self.record_open_stage(tfm!(
                                "Предупреждение: сохранённая разметка изображений DOC не загружена: {reason}", "Open warning: saved DOC image layout was not loaded: {reason}"
                            )),
                        }
                        self.legacy_doc = Some(package);
                        self.docx = None;
                        self.spreadsheet = None;
                        self.saved_docx_model = None;
                        self.finish_open(&path, tr!("старый DOC загружен", "legacy DOC loaded"));
                    }
                    Err(error) => {
                        self.open_failed(tr!("загрузка старого DOC", "legacy DOC loading"), error)
                    }
                }
            }
        }
    }

    fn plain_text(&self) -> Option<String> {
        if let Some(document) = &self.text {
            return Some(document.text.clone());
        }
        if let Some(package) = &self.docx {
            return Some(
                package
                    .model
                    .paragraphs
                    .iter()
                    .map(|paragraph| {
                        paragraph
                            .fragments
                            .iter()
                            .map(|fragment| fragment.text.as_str())
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        if let Some(sheet) = self
            .spreadsheet
            .as_ref()
            .and_then(SpreadsheetDocument::selected_sheet)
        {
            return Some(
                sheet
                    .cells
                    .iter()
                    .map(|cell| {
                        format!(
                            "{}{}\t{}",
                            spreadsheet_column_name(cell.column),
                            cell.row + 1,
                            cell.value
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
        self.legacy_doc
            .as_ref()
            .map(|package| package.paragraphs.join("\n"))
    }

    fn record_docx_edit(&mut self, before: DocumentModel) {
        let Some(package) = self.docx.as_ref() else {
            return;
        };
        if package.model != before {
            self.search_dirty = true;
            self.undo_stack.push(before);
            if self.undo_stack.len() > 100 {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
        }
    }

    fn change_font_size(&mut self, half_points: u16) {
        let Some(package) = self.docx.as_mut() else {
            return;
        };
        if !(12..=144).contains(&half_points) {
            return;
        }
        let before = package.model.clone();
        let changed = if self.select_all_docx {
            package.model.set_all_run_font_sizes(half_points)
        } else if let Some(fragment_id) = self.selected_fragment_id {
            let needs_change = package
                .model
                .fragment(fragment_id)
                .is_some_and(|f| f.font_size_half_points != Some(half_points));
            needs_change && package.model.set_run_font_size(fragment_id, half_points)
        } else {
            false
        };
        if changed {
            self.undo_stack.push(before);
            if self.undo_stack.len() > 100 {
                self.undo_stack.remove(0);
            }
            self.redo_stack.clear();
        }
    }

    fn workbook_snapshot(&self) -> Option<WorkbookSnapshot> {
        self.spreadsheet.as_ref().map(|book| WorkbookSnapshot {
            sheets: book.sheets.clone(),
            formats: book.formats.clone(),
        })
    }

    fn record_workbook_edit(&mut self, before: Option<WorkbookSnapshot>) {
        let Some(before) = before else {
            return;
        };
        let Some(after) = self.workbook_snapshot() else {
            return;
        };
        if before == after {
            return;
        }
        if self.workbook_undo.len() >= 100 {
            self.workbook_undo.remove(0);
        }
        self.workbook_undo.push(before);
        self.workbook_redo.clear();
    }

    fn apply_workbook_snapshot(&mut self, snapshot: WorkbookSnapshot) {
        let Some(book) = self.spreadsheet.as_mut() else {
            return;
        };
        book.sheets = snapshot.sheets;
        book.formats = snapshot.formats;
        if book.selected_cell.is_some_and(|(row, column)| {
            book.selected_sheet()
                .is_none_or(|sheet| row >= sheet.row_count || column >= sheet.column_count)
        }) {
            book.selected_cell = None;
        }
        self.search_dirty = true;
    }

    fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty() || !self.workbook_undo.is_empty() || !self.text_undo.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty() || !self.workbook_redo.is_empty() || !self.text_redo.is_empty()
    }

    fn undo(&mut self) {
        if self.docx.is_some() {
            let Some(previous) = self.undo_stack.pop() else {
                return;
            };
            if let Some(package) = self.docx.as_mut() {
                self.redo_stack.push(package.model.clone());
                package.model = previous;
                self.search_dirty = true;
                self.status = tr!("Последняя правка DOCX отменена", "Undid last DOCX edit").into();
            }
        } else if self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.editor.is_some())
        {
            self.undo_workbook();
        } else if self.text.is_some() {
            self.undo_text();
        }
    }

    fn undo_workbook(&mut self) {
        let Some(previous) = self.workbook_undo.pop() else {
            return;
        };
        if let Some(current) = self.workbook_snapshot() {
            self.workbook_redo.push(current);
        }
        self.apply_workbook_snapshot(previous);
        self.status = tr!(
            "Последняя правка таблицы отменена",
            "Undid last workbook edit"
        )
        .into();
    }

    fn redo(&mut self) {
        if self.docx.is_some() {
            let Some(next) = self.redo_stack.pop() else {
                return;
            };
            if let Some(package) = self.docx.as_mut() {
                self.undo_stack.push(package.model.clone());
                package.model = next;
                self.search_dirty = true;
                self.status = tr!("Правка DOCX повторена", "Redid DOCX edit").into();
            }
        } else if self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.editor.is_some())
        {
            self.redo_workbook();
        } else if self.text.is_some() {
            self.redo_text();
        }
    }

    fn redo_workbook(&mut self) {
        let Some(next) = self.workbook_redo.pop() else {
            return;
        };
        if let Some(current) = self.workbook_snapshot() {
            self.workbook_undo.push(current);
        }
        self.apply_workbook_snapshot(next);
        self.status = tr!("Правка таблицы повторена", "Redid workbook edit").into();
    }

    fn record_text_edit(&mut self, before: Option<String>) {
        if self.suppress_text_history {
            self.suppress_text_history = false;
            return;
        }
        let (Some(before), Some(document)) = (before, self.text.as_ref()) else {
            return;
        };
        if before == document.text {
            return;
        }
        if self.text_undo.len() >= 100 {
            self.text_undo.remove(0);
        }
        self.text_undo.push(before);
        self.text_redo.clear();
    }

    fn undo_text(&mut self) {
        let Some(previous) = self.text_undo.pop() else {
            return;
        };
        self.suppress_text_history = true;
        if let Some(document) = self.text.as_mut() {
            self.text_redo.push(document.text.clone());
            document.text = previous;
        }
        self.search_dirty = true;
        self.status = tr!("Последняя правка текста отменена", "Undid last text edit").into();
    }

    fn redo_text(&mut self) {
        let Some(next) = self.text_redo.pop() else {
            return;
        };
        self.suppress_text_history = true;
        if let Some(document) = self.text.as_mut() {
            self.text_undo.push(document.text.clone());
            document.text = next;
        }
        self.search_dirty = true;
        self.status = tr!("Правка текста повторена", "Redid text edit").into();
    }

    fn csv_text(&self) -> Option<String> {
        let sheet = self
            .spreadsheet
            .as_ref()
            .and_then(SpreadsheetDocument::selected_sheet)?;
        const MAX_EXPORT_COLUMNS: usize = 256;
        const MAX_EXPORT_ROWS: usize = 10_000;
        let column_count = sheet.column_count.min(MAX_EXPORT_COLUMNS);
        let row_count = sheet.row_count.min(MAX_EXPORT_ROWS);
        let mut lines = Vec::with_capacity(row_count);
        for row in 0..row_count {
            let values = (0..column_count)
                .map(|column| {
                    let value = sheet.cell_value(row, column);
                    if value.contains([',', '"', '\n', '\r']) {
                        format!("\"{}\"", value.replace('"', "\"\""))
                    } else {
                        value.to_owned()
                    }
                })
                .collect::<Vec<_>>();
            lines.push(values.join(","));
        }
        Some(lines.join("\n"))
    }

    #[cfg(windows)]
    fn export_pdf(&mut self) {
        if self.print_receiver.is_some() {
            self.status = tr!(
                "Дождитесь завершения текущей печати",
                "Wait for the current print job to finish"
            )
            .into();
            return;
        }
        let settings = self.print_settings.sanitized();
        let blocks = if let Some(package) = &self.docx {
            docx_print_blocks(&package.model, &package.images, settings)
        } else if let Some(book) = &self.spreadsheet {
            spreadsheet_print_blocks(book, settings)
        } else {
            self.status = tr!(
                "Экспорт PDF доступен для DOCX и таблиц",
                "PDF export is available for DOCX and workbooks"
            )
            .into();
            return;
        };
        if blocks.is_empty() {
            self.status = tr!("Нечего экспортировать в PDF", "Nothing to export to PDF").into();
            return;
        }
        let suggested = self
            .opened_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|stem| format!("{}.pdf", stem.to_string_lossy()))
            .unwrap_or_else(|| "NotOffice.pdf".into());
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(suggested)
            .save_file()
        else {
            return;
        };
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
        {
            self.status = tr!(
                "Выберите файл с расширением .pdf",
                "Choose a .pdf destination"
            )
            .into();
            return;
        }
        let manifest = match print::spool_manifest(&blocks, settings) {
            Ok(manifest) => manifest,
            Err(error) => {
                self.status = tfm!(
                    "Не удалось подготовить PDF: {error}",
                    "PDF preparation failed: {error}"
                );
                return;
            }
        };
        let (sender, receiver) = std::sync::mpsc::channel();
        self.print_receiver = Some(receiver);
        self.status = tr!(
            "Экспорт PDF: печать во временный файл…",
            "Exporting PDF: printing to a file…"
        )
        .into();
        std::thread::spawn(move || {
            let result = print::run_windows_print_helper(&manifest, Some(&path));
            let message = match result {
                Ok(print::PrintOutcome::Printed) => {
                    tfm!("PDF сохранён: {}", "PDF saved: {}", path.display())
                }
                Ok(print::PrintOutcome::Cancelled) => {
                    tr!("Экспорт PDF отменён", "PDF export cancelled").to_string()
                }
                Err(error) => {
                    tfm!(
                        "Не удалось создать PDF: {error}",
                        "PDF export failed: {error}"
                    )
                }
            };
            let _ = sender.send(message);
        });
    }

    #[cfg(not(windows))]
    fn export_pdf(&mut self) {
        self.status = tr!(
            "Экспорт PDF доступен только в Windows-сборке",
            "PDF export is available in the Windows build only"
        )
        .into();
    }

    fn export_html(&mut self) {
        let title = self
            .opened_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "NotOffice".into());
        let html = if let Some(package) = &self.docx {
            liteoffice::html_export::document(&title, &package.model)
        } else if let Some(book) = &self.spreadsheet {
            let Some(sheet) = book.sheets.get(book.selected_sheet) else {
                self.status = tr!(
                    "В книге нет листа для экспорта",
                    "The workbook has no sheet to export"
                )
                .into();
                return;
            };
            liteoffice::html_export::spreadsheet(&sheet.sheet_name, sheet)
        } else {
            self.status = tr!(
                "Экспорт HTML доступен для DOCX и таблиц",
                "HTML export is available for DOCX and workbooks"
            )
            .into();
            return;
        };
        let suggested = format!("{title}.html");
        let Some(path) = rfd::FileDialog::new()
            .add_filter("HTML", &["html", "htm"])
            .set_file_name(suggested)
            .save_file()
        else {
            return;
        };
        if !path.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm")
        }) {
            self.status = tr!("Выберите файл .html", "Choose an .html destination").into();
            return;
        }
        match std::fs::write(&path, html.as_bytes()) {
            Ok(()) => {
                self.status = tfm!("HTML сохранён: {}", "HTML saved: {}", path.display());
            }
            Err(error) => {
                self.status = tfm!(
                    "Не удалось сохранить HTML: {error}",
                    "HTML export failed: {error}"
                );
            }
        }
    }

    fn export_csv(&mut self) {
        let Some(text) = self.csv_text() else {
            return;
        };
        let suggested = self
            .opened_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .and_then(|stem| stem.to_str())
            .map(|stem| format!("{stem}.csv"))
            .unwrap_or_else(|| "worksheet.csv".into());
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(suggested)
            .add_filter("CSV", &["csv"])
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, text) {
            Ok(()) => self.status = tfm!("Экспортировано: {}", "Exported {}", path.display()),
            Err(error) => {
                self.status = tfm!(
                    "Не удалось экспортировать CSV: {error}",
                    "CSV export failed: {error}"
                )
            }
        }
    }

    fn print_document(&mut self) {
        if self.pdf.is_some() {
            if self.print_receiver.is_none() {
                self.pdf_print_open = true;
            }
            return;
        }
        if self.print_receiver.is_some() {
            self.status = tr!(
                "Диалог печати уже открыт",
                "A print dialog is already active"
            )
            .into();
            return;
        }
        let mut blocks = Vec::<print::PrintBlock>::new();
        if let Some(doc) = &self.legacy_doc {
            let rasters = doc
                .images
                .iter()
                .map(|asset| {
                    match image::ImageReader::new(std::io::Cursor::new(&asset.bytes))
                        .with_guessed_format()
                        .ok()
                        .and_then(|reader| reader.into_dimensions().ok())
                    {
                        Some((w, h)) => legacy_layout::RasterInfo {
                            width_px: w,
                            height_px: h,
                            dpi_x: 96.0,
                        },
                        None => legacy_layout::RasterInfo {
                            width_px: 0,
                            height_px: 0,
                            dpi_x: 96.0,
                        },
                    }
                })
                .collect::<Vec<_>>();
            let pages = legacy_layout::paginate(
                &doc.paragraphs,
                &self.doc_image_anchors,
                &self.doc_image_widths,
                &rasters,
                self.print_settings,
            );
            for (page_index, page) in pages.iter().enumerate() {
                if page_index > 0 {
                    blocks.push(print::PrintBlock::PageBreak);
                }
                if page.is_empty() {
                    blocks.push(print::PrintBlock::Text(String::new()));
                    continue;
                }
                for item in page {
                    match item {
                        legacy_layout::Item::Paragraph(i) => {
                            blocks.push(print::PrintBlock::Text(doc.paragraphs[*i].clone()))
                        }
                        legacy_layout::Item::Image(i) => {
                            if let Some(asset) = doc.images.get(*i) {
                                blocks.push(print::PrintBlock::Image {
                                    bytes: asset.bytes.clone(),
                                    width_percent: *self.doc_image_widths.get(*i).unwrap_or(&85.0),
                                });
                            }
                        }
                        legacy_layout::Item::UnplacedHeading => {
                            blocks.push(print::PrintBlock::Text(
                                tr!("Восстановленные изображения (без позиций, не оригинальная разметка Word)", "Recovered images (unplaced, not original Word layout)").into(),
                            ))
                        }
                    }
                }
            }
        } else if let Some(package) = &self.docx {
            blocks = docx_print_blocks(&package.model, &package.images, self.print_settings);
        } else if let Some(text) = self.plain_text() {
            if text.is_empty() {
                blocks.push(print::PrintBlock::Text(String::new()));
            } else {
                blocks.extend(
                    text.lines()
                        .map(|line| print::PrintBlock::Text(line.into())),
                );
            }
        }
        if blocks.is_empty() {
            self.status = tr!("Нечего печатать", "Nothing to print").into();
            return;
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        self.print_receiver = Some(receiver);
        self.status =
            tr!("Открываем предпросмотр печати — приблизительная разметка; оригинальная разметка Word не воспроизводится", "Opening print preview — approximate layout; original Word layout is not reproduced")
                .into();
        let page_settings = self.print_settings.sanitized();
        std::thread::spawn(move || {
            let result = print::print_blocks(blocks, page_settings);
            let report = match result {
                Ok(print::PrintOutcome::Printed) => tr!(
                    "Задание отправлено на выбранный принтер",
                    "Print job sent to the selected printer"
                )
                .to_owned(),
                Ok(print::PrintOutcome::Cancelled) => tr!(
                    "Печать отменена пользователем",
                    "Printing cancelled by the user"
                )
                .to_owned(),
                Err(error) => tfm!("Не удалось напечатать: {error}", "Print failed: {error}"),
            };
            let _ = sender.send(report);
        });
    }

    fn clear_recent_files(&mut self) {
        if self.recent_paths.is_empty() {
            return;
        }
        let answer = rfd::MessageDialog::new()
            .set_title(tr!(
                "Очистить список недавних файлов?",
                "Clear recent files?"
            ))
            .set_description(tr!(
                "Список будет очищен. Сами документы на диске не удаляются.",
                "The list will be cleared. Documents on disk will not be deleted."
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if answer != rfd::MessageDialogResult::Yes {
            return;
        }
        self.recent_paths.clear();
        let result = recent_store_path()
            .map(|path| liteoffice::recent::save_paths(&path, &self.recent_paths))
            .unwrap_or(Ok(()));
        self.status = match result {
            Ok(()) => tr!("Список недавних файлов очищен", "Recent files cleared").into(),
            Err(error) => tfm!(
                "Список очищен в памяти, но не сохранён: {error}",
                "Recent list cleared in memory, but not saved: {error}"
            ),
        };
    }

    fn delete_recovery_folders(&mut self) {
        let root = self
            .recovery_root
            .clone()
            .unwrap_or_else(|| liteoffice::app_paths::data_dir().join("NotOffice-recovery"));
        let answer = rfd::MessageDialog::new()
            .set_title(tr!(
                "Удалить папки восстановления?",
                "Delete recovery folders?"
            ))
            .set_description(tr!(
                "Будут удалены все закрытые папки восстановления, включая несохранённые копии. Открытые документы и их несохранённые правки не изменятся. Активные окна не затрагиваются. Продолжить?",
                "All closed recovery folders will be deleted, including unsaved copies. Open documents and their unsaved edits will not change. Active windows are not affected. Continue?"
            ))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if answer != rfd::MessageDialogResult::Yes {
            return;
        }
        // Release this window's claimed sessions so its own folders can be
        // removed by the explicit cleanup too.
        self.recovery_session = None;
        self.recovery_candidates.clear();
        self.recovery_snapshot_model = None;
        self.recovery_snapshot_text = None;
        self.recovery_snapshot_workbook = None;
        self.recovery_last_attempt = None;
        match liteoffice::recovery::clear_closed_sessions(&root) {
            Ok(report) => {
                self.status = tfm!(
                    "Папки восстановления удалены: {}; активных окон пропущено: {}{}",
                    "Recovery folders deleted: {}; live windows skipped: {}{}",
                    report.removed,
                    report.skipped_live,
                    if report.failed.is_empty() {
                        String::new()
                    } else {
                        format!("; {}", report.failed.join("; "))
                    }
                );
            }
            Err(error) => {
                self.status = tfm!(
                    "Не удалось удалить папки восстановления: {error}",
                    "Could not delete recovery folders: {error}"
                );
            }
        }
    }

    fn open_recovery_folder(&mut self) {
        let root = self
            .recovery_root
            .clone()
            .unwrap_or_else(|| liteoffice::app_paths::data_dir().join("NotOffice-recovery"));
        if let Err(error) = std::fs::create_dir_all(&root) {
            self.status = tfm!(
                "Не удалось открыть папку восстановления: {error}",
                "Could not open the recovery folder: {error}"
            );
            return;
        }
        #[cfg(windows)]
        let result = std::process::Command::new("explorer").arg(&root).spawn();
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg(&root).spawn();
        #[cfg(all(not(windows), not(target_os = "macos")))]
        let result = std::process::Command::new("xdg-open").arg(&root).spawn();
        self.status = match result {
            Ok(_) => tfm!(
                "Открыта папка восстановления: {}",
                "Opened recovery folder: {}",
                root.display()
            ),
            Err(error) => tfm!(
                "Не удалось открыть папку восстановления: {error}",
                "Could not open the recovery folder: {error}"
            ),
        };
    }

    fn paste_range_text(
        &mut self,
        text: &str,
    ) -> Result<(usize, usize), liteoffice::range_paste::RangePasteError> {
        if !self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.editor.is_some())
        {
            return Err(liteoffice::range_paste::RangePasteError::ReadOnly);
        }
        let before = self.workbook_snapshot();
        let result = self
            .spreadsheet
            .as_mut()
            .expect("spreadsheet checked above")
            .paste_grid(text);
        if result.is_ok() {
            self.record_workbook_edit(before);
            self.search_dirty = true;
        }
        result
    }

    fn paste_range_from_clipboard(&mut self) {
        if !self
            .spreadsheet
            .as_ref()
            .is_some_and(|book| book.editor.is_some())
        {
            self.status = tr!(
                "Диапазон можно вставить только в редактируемую XLSX-копию",
                "A range can be pasted only into an editable XLSX copy"
            )
            .into();
            return;
        }
        let text = match clipboard::read_text() {
            Ok(text) => text,
            Err(error) => {
                self.status = tfm!("Буфер обмена: {error}", "Clipboard: {error}");
                return;
            }
        };
        match self.paste_range_text(&text) {
            Ok((rows, columns)) => {
                self.status = tfm!(
                    "Вставлен диапазон {} × {} (только значения; формулы не выполняются)",
                    "Pasted {} × {} range (values only; formulas are not executed)",
                    rows,
                    columns
                );
            }
            Err(error) => {
                self.status = tfm!(
                    "Диапазон не вставлен: {error}",
                    "Range was not pasted: {error}"
                );
            }
        }
    }

    fn set_theme(&mut self, theme: Theme) {
        if self.theme == theme {
            return;
        }
        let before = (self.print_settings, self.zoom, self.theme);
        self.theme = theme;
        if let Err(error) = save_preferences(before, self.print_settings, self.zoom, self.theme) {
            self.status = tfm!(
                "Тема применена, но настройка не сохранена: {error}",
                "Theme applied, but the preference was not saved: {error}"
            );
        }
    }

    fn export_text(&mut self) {
        let Some(text) = self.plain_text() else {
            return;
        };
        let suggested = self
            .opened_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .and_then(|stem| stem.to_str())
            .map(|stem| format!("{stem}.txt"))
            .unwrap_or_else(|| "document.txt".into());
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(suggested)
            .add_filter(tr!("Обычный текст", "Plain text"), &["txt"])
            .save_file()
        else {
            return;
        };
        match std::fs::write(&path, text) {
            Ok(()) => self.status = tfm!("Экспортировано: {}", "Exported {}", path.display()),
            Err(error) => {
                self.status = tfm!(
                    "Не удалось экспортировать текст: {error}",
                    "Text export failed: {error}"
                )
            }
        }
    }

    fn create_clean_copy(&mut self) {
        let Some(source) = self.opened_path.clone() else {
            self.status = tr!("Сначала откройте документ", "Open a document first").into();
            return;
        };
        let docx_source = self.docx.is_some();
        let legacy_source = self.legacy_doc.is_some();
        let workbook_source = self.spreadsheet.is_some();
        if !docx_source && !legacy_source && !workbook_source {
            self.status = tr!(
                "Копия без активного содержимого доступна для DOC/DOCX и XLS/XLSX",
                "Clean copy is available for DOC/DOCX and XLS/XLSX"
            )
            .into();
            return;
        }
        let extension = if workbook_source { "xlsx" } else { "docx" };
        let stem = source
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("document");
        let warning = tr!(
            "Будет создана отдельная копия. Переносятся только текст, таблицы, значения ячеек и растровые изображения. Формулы, макросы, VBA, ActiveX/OLE, вложения, внешние подключения, поля и часть оформления будут потеряны. Кеш значений формул может быть устаревшим. Это не антивирус и не гарантия отсутствия всех угроз. Оригинал не изменится.",
            "A separate copy will be created. Only text, tables, cell values and raster images are transferred. Formulas, macros, VBA, ActiveX/OLE, attachments, external connections, fields and some formatting will be lost. Cached formula values may be stale. This is not an antivirus and does not guarantee the absence of every threat. The original will not change."
        );
        let answer = rfd::MessageDialog::new()
            .set_title(tr!(
                "Создать копию без активного содержимого?",
                "Create a passive-only copy?"
            ))
            .set_description(warning)
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if answer != rfd::MessageDialogResult::Yes {
            self.status = tr!("Создание копии отменено", "Clean copy cancelled").into();
            return;
        }
        let mut dialog = rfd::FileDialog::new()
            .set_file_name(format!("{stem}-clean.{extension}"))
            .add_filter(
                if workbook_source {
                    tr!("Книга Excel", "Excel workbook")
                } else {
                    tr!("Документ Word", "Word document")
                },
                &[extension],
            );
        if let Some(parent) = source.parent() {
            dialog = dialog.set_directory(parent);
        }
        let Some(destination) = dialog.save_file() else {
            self.status = tr!("Создание копии отменено", "Clean copy cancelled").into();
            return;
        };
        let result = if workbook_source {
            self.spreadsheet.as_ref().map_or_else(
                || {
                    Err(liteoffice::docx::DocxError::UnsafeSave(
                        "no workbook".into(),
                    ))
                },
                |book| {
                    clean_copy::clean_workbook(&source, &book.sheets, &book.formats, &destination)
                },
            )
        } else if legacy_source {
            let Some(legacy) = self.legacy_doc.as_ref() else {
                return;
            };
            match liteoffice::docx::DocxPackage::from_legacy(
                &source,
                legacy,
                &self.doc_image_anchors,
                &self.doc_image_widths,
            ) {
                Ok(package) => clean_copy::clean_docx(&source, &package, &destination),
                Err(error) => Err(error),
            }
        } else {
            let Some(package) = self.docx.as_ref() else {
                return;
            };
            clean_copy::clean_docx(&source, package, &destination)
        };
        match result {
            Ok(report) => {
                self.status = tfm!(
                    "Копия без активного содержимого создана: {}",
                    "Passive-only copy created: {}",
                    destination.display()
                );
                let _ = rfd::MessageDialog::new()
                    .set_title(tr!("Копия создана", "Copy created"))
                    .set_description(format!(
                        "{}\n\nОригинал не изменён.\n{}",
                        report.summary(),
                        tr!(
                            "Функция исключает перечисленные категории активного содержимого, но не является антивирусом и не гарантирует отсутствие любых возможных угроз.",
                            "This function excludes the listed categories of active content, but it is not an antivirus and does not guarantee the absence of every possible threat."
                        )
                    ))
                    .set_buttons(rfd::MessageButtons::Ok)
                    .show();
            }
            Err(error) => {
                self.status = tfm!("Копия не создана: {error}", "Clean copy failed: {error}")
            }
        }
    }

    fn save_legacy_layout(&mut self) {
        let (Some(path), Some(package)) = (&self.opened_path, &self.legacy_doc) else {
            return;
        };
        match legacy_sidecar::save(
            path,
            package.paragraphs.len(),
            package.images.len(),
            &self.doc_image_anchors,
            &self.doc_image_widths,
        ) {
            Ok(sidecar) => {
                self.status = tfm!(
                    "Разметка изображений сохранена: {} (оригинал DOC не изменён)",
                    "Image layout saved to {} (original DOC unchanged)",
                    sidecar.display()
                )
            }
            Err(reason) => {
                self.status = tfm!(
                    "Не удалось сохранить разметку изображений: {reason}",
                    "Saving image layout failed: {reason}"
                )
            }
        }
    }

    fn save_needs_explanation(&self) -> Option<(bool, String)> {
        if self.recovered_document {
            return Some((
                true,
                tr!(
                    "Восстановленный документ нельзя сохранить поверх исходного. Будет создана отдельная копия; оригинал не изменится.",
                    "A recovered document cannot overwrite its source. A separate copy will be created; the original stays unchanged."
                )
                .into(),
            ));
        }
        if let Some(document) = &self.text {
            if document.requires_save_as() {
                let reason = if document.path().is_none() {
                    tr!(
                        "Документ ещё не сохранён на диск. Сейчас будет предложено выбрать файл и формат.",
                        "This document has not been saved to disk yet. You will be asked to choose a file and format."
                    )
                } else {
                    tr!(
                        "Оригинальный RTF защищён от перезаписи. Будет создана отдельная копия; оригинал не изменится.",
                        "The original RTF is protected from overwriting. A separate copy will be created; the original stays unchanged."
                    )
                };
                let copy = document.path().is_some();
                return Some((copy, reason.into()));
            }
        }
        if let Some(book) = &self.spreadsheet {
            if book.editor.is_some()
                && book
                    .editor
                    .as_ref()
                    .and_then(|editor| editor.saved_path())
                    .is_none()
            {
                return Some((
                    false,
                    tr!(
                        "Книга ещё не сохранена в XLSX. Сейчас будет предложено выбрать файл; исходная книга не изменится.",
                        "This workbook has not been saved as XLSX yet. You will be asked to choose a file; the source workbook stays unchanged."
                    )
                    .into(),
                ));
            }
        }
        if self
            .docx
            .as_ref()
            .is_some_and(DocxPackage::requires_save_as)
        {
            return Some((
                true,
                tr!(
                    "Это преобразованная или восстановленная копия. Сохранить её поверх исходного файла нельзя; будет предложена отдельная копия DOCX. Оригинал не изменится.",
                    "This is a converted or recovered copy. It cannot overwrite the source; a separate DOCX copy will be offered. The original stays unchanged."
                )
                .into(),
            ));
        }
        None
    }

    fn request_save(&mut self) -> bool {
        if self.save_prompt_open || self.decision_pending() {
            return false;
        }
        if let Some((copy, reason)) = self.save_needs_explanation() {
            self.save_prompt_reason = reason;
            self.save_prompt_copy = copy;
            self.save_prompt_open = true;
            return false;
        }
        self.save_document()
    }

    fn request_legacy_edit(&mut self) {
        if self.legacy_doc.is_some() && !self.decision_pending() {
            self.legacy_edit_prompt = true;
        }
    }

    fn save_document(&mut self) -> bool {
        if let Some(document) = &self.text {
            if document.requires_save_as() || self.recovered_document {
                return self.save_document_as(self.recovered_document);
            }
            let path = document.path().unwrap().to_path_buf();
            return self.save_text_to(path);
        }
        if let Some(book) = self.spreadsheet.as_ref() {
            let path = book
                .editor
                .as_ref()
                .and_then(|e| e.saved_path())
                .map(Path::to_path_buf);
            return match path {
                Some(path) => {
                    let path = Self::with_default_extension(path, "xlsx");
                    self.save_workbook_to(path)
                }
                None => self.save_document_as(false),
            };
        }
        if self.recovered_document
            || self
                .docx
                .as_ref()
                .is_some_and(DocxPackage::requires_save_as)
        {
            return self.save_document_as(true);
        }
        let Some(path) = self
            .docx
            .as_ref()
            .map(|package| package.source_path().to_path_buf())
        else {
            return false;
        };
        self.save_document_to(path)
    }

    fn with_default_extension(path: PathBuf, extension: &str) -> PathBuf {
        if path.extension().is_none() {
            path.with_extension(extension)
        } else {
            path
        }
    }

    fn save_document_as(&mut self, copy: bool) -> bool {
        if let Some(document) = &self.text {
            let stem = self
                .opened_path
                .as_deref()
                .and_then(Path::file_stem)
                .unwrap_or_else(|| std::ffi::OsStr::new("document"))
                .to_string_lossy();
            let suffix = if copy
                || self.recovered_document
                || document.kind == liteoffice::text_document::TextKind::Rtf
            {
                "-copy"
            } else {
                ""
            };
            let suggested = format!("{stem}{suffix}.{}", document.kind.extension());
            let mut dialog = rfd::FileDialog::new().set_file_name(suggested).add_filter(
                tr!(
                    "Текст / Markdown / простой RTF",
                    "Text / Markdown / basic RTF"
                ),
                &["txt", "md", "rtf"],
            );
            if let Some(parent) = self.opened_path.as_deref().and_then(Path::parent) {
                dialog = dialog.set_directory(parent);
            }
            return match dialog.save_file() {
                Some(path) => {
                    let path = Self::with_default_extension(path, document.kind.extension());
                    self.save_text_to(path)
                }
                None => {
                    self.status = tr!(
                        "«Сохранить как» отменено: текст сохранён",
                        "Save As cancelled: text kept"
                    )
                    .into();
                    false
                }
            };
        }
        if self
            .spreadsheet
            .as_ref()
            .is_some_and(|b| b.editor.is_some())
        {
            let stem = self
                .opened_path
                .as_deref()
                .and_then(Path::file_stem)
                .unwrap_or_default()
                .to_string_lossy();
            let mut dialog = rfd::FileDialog::new()
                .set_file_name(format!("{stem}-copy.xlsx"))
                .add_filter(tr!("Книга Excel", "Excel workbook"), &["xlsx"]);
            if let Some(parent) = self.opened_path.as_deref().and_then(Path::parent) {
                dialog = dialog.set_directory(parent);
            }
            return match dialog.save_file() {
                Some(path) => self.save_workbook_to(path),
                None => {
                    self.status = tr!(
                        "«Сохранить как» отменено: правки книги сохранены",
                        "Save As cancelled: workbook edits kept"
                    )
                    .into();
                    false
                }
            };
        }
        if self.docx.is_none() {
            return false;
        }
        let suggested = self
            .opened_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("edited.docx");
        let suggested = if copy
            || self.recovered_document
            || self
                .docx
                .as_ref()
                .is_some_and(DocxPackage::requires_save_as)
        {
            format!(
                "{}-copy.docx",
                Path::new(suggested)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
            )
        } else {
            suggested.to_owned()
        };
        let mut dialog = rfd::FileDialog::new().set_file_name(suggested);
        if let Some(parent) = self.opened_path.as_deref().and_then(Path::parent) {
            dialog = dialog.set_directory(parent);
        }
        let Some(path) = dialog
            .add_filter(tr!("Документ Word", "Word document"), &["docx"])
            .save_file()
        else {
            self.status = tr!(
                "«Сохранить как» отменено: документ и несохранённые правки сохранены",
                "Save As cancelled: document and unsaved edits were kept"
            )
            .into();
            return false;
        };
        self.save_document_to(Self::with_default_extension(path, "docx"))
    }

    fn save_document_to(&mut self, path: PathBuf) -> bool {
        if self.text.is_some() {
            return self.save_text_to(path);
        }
        if self.spreadsheet.is_some() {
            return self.save_workbook_to(path);
        }
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"))
        {
            self.status = tr!(
                "Сохранение отклонено: выберите путь .docx; правки сохранены",
                "Save rejected: choose a .docx destination; edits kept"
            )
            .into();
            return false;
        }
        // A recovered session must never save into its own disposable storage.
        // Resolve the existing parent to catch relative paths/junction aliases.
        if let Some(root) = &self.recovery_root {
            if path
                .parent()
                .and_then(|parent| parent.canonicalize().ok())
                .zip(root.canonicalize().ok())
                .is_some_and(|(parent, root)| parent.starts_with(root))
            {
                self.status = tr!("Сохранение отклонено: выберите место вне NotOffice-recovery; правки сохранены", "Save rejected: choose a document location outside NotOffice-recovery; edits kept").into();
                return false;
            }
        }
        let Some(package) = self.docx.as_mut() else {
            return false;
        };
        let replaced_existing_file = path.exists();
        match package.save(&path) {
            Ok(()) => {
                if !self.clear_current_recovery() {
                    self.recovery_session = None;
                    self.recovery_snapshot_model = None;
                    self.recovery_snapshot_workbook = None;
                    self.recovery_last_attempt = None;
                }
                self.recovered_document = false;
                self.opened_path = Some(path.clone());
                self.saved_docx_model = self.docx.as_ref().map(|package| package.model.clone());
                self.docx_textures.clear();
                self.conflict_destination = None;
                self.remember_recent(path.clone());
                self.status = if replaced_existing_file {
                    tfm!(
                        "Сохранено: {} (предыдущий файл сохранён как отдельная резервная копия)",
                        "Saved {} (previous destination retained as a separate backup)",
                        path.display()
                    )
                } else {
                    tfm!(
                        "Надёжно сохранено: {}",
                        "Saved safely to {}",
                        path.display()
                    )
                };
                true
            }
            Err(liteoffice::docx::DocxError::SourceChanged) => {
                self.conflict_destination = Some(path.clone());
                self.status = tfm!("Конфликт сохранения: {} изменён на диске. Несохранённые правки сохранены; перезагрузите, сохраните копию или отмените.", "Save conflict: {} changed on disk. Your unsaved edits are kept; reload, save a copy, or cancel.", path.display());
                false
            }
            Err(error) => {
                self.status = tfm!(
                    "Не удалось сохранить {}; правки сохранены: {error}",
                    "Save failed for {}; unsaved edits were kept: {error}",
                    path.display()
                );
                false
            }
        }
    }

    fn edit_legacy_as_docx(&mut self) {
        if self.decision_pending() {
            return;
        }
        let (Some(path), Some(legacy)) = (self.opened_path.clone(), self.legacy_doc.as_ref())
        else {
            return;
        };
        match DocxPackage::from_legacy(
            &path,
            legacy,
            &self.doc_image_anchors,
            &self.doc_image_widths,
        ) {
            Ok(package) => {
                self.saved_docx_model = Some(package.model.clone());
                self.docx = Some(package);
                self.legacy_doc = None;
                self.docx_textures.clear();
                self.legacy_doc_textures.clear();
                self.finish_open(
                    &path,
                    tr!("DOC преобразован в редактируемое содержимое; требуется «Сохранить как DOCX»", "DOC converted to editable recovered content; Save As DOCX required"),
                );
            }
            Err(error) => {
                self.status = tfm!(
                    "Не удалось преобразовать DOC; исходный просмотр сохранён: {error}",
                    "DOC conversion failed; original preview kept: {error}"
                )
            }
        }
    }

    fn edit_workbook_copy(&mut self) {
        if self.decision_pending() {
            return;
        }
        let (Some(path), Some(book)) = (&self.opened_path, self.spreadsheet.as_mut()) else {
            return;
        };
        book.editor = Some(liteoffice::workbook_edit::WorkbookEditor::new(
            path.clone(),
            &book.sheets,
        ));
        book.editor.as_mut().unwrap().formats = book.formats.clone();
        book.selected_cell = Some((0, 0));
        book.focus_pending = true;
        self.status = tr!("Копия со значениями: текст и поддерживаемое форматирование; требуется «Сохранить как XLSX»", "Editable values copy: literal text and supported font formatting; Save As XLSX required").into();
    }

    fn save_workbook_to(&mut self, path: PathBuf) -> bool {
        let Some(book) = self.spreadsheet.as_mut() else {
            return false;
        };
        let Some(editor) = book.editor.as_mut() else {
            return false;
        };
        match editor.save(&path, &book.sheets) {
            Ok(()) => {
                if !self.clear_current_recovery() {
                    self.recovery_session = None;
                    self.recovery_snapshot_model = None;
                    self.recovery_snapshot_text = None;
                    self.recovery_snapshot_workbook = None;
                    self.recovery_last_attempt = None;
                }
                self.recovered_document = false;
                self.opened_path = Some(path.clone());
                self.conflict_destination = None;
                self.remember_recent(path.clone());
                self.status = tfm!(
                    "XLSX надёжно сохранён: {}; числа, логические значения и даты сохранены как типы",
                    "Saved XLSX safely to {}; numbers, booleans and dates kept as typed cells",
                    path.display()
                );
                true
            }
            Err(liteoffice::docx::DocxError::SourceChanged) => {
                self.conflict_destination = Some(path);
                self.status = tr!(
                    "Книга изменена на диске: запись заблокирована; правки сохранены",
                    "Workbook changed on disk: overwrite blocked; edits kept"
                )
                .into();
                false
            }
            Err(error) => {
                self.status = tfm!(
                    "Не удалось сохранить книгу; правки сохранены: {error}",
                    "Workbook save failed; edits kept: {error}"
                );
                false
            }
        }
    }

    fn save_text_to(&mut self, path: PathBuf) -> bool {
        if self.recovery_root.as_ref().is_some_and(|root| {
            path.parent()
                .and_then(|p| p.canonicalize().ok())
                .zip(root.canonicalize().ok())
                .is_some_and(|(parent, root)| parent.starts_with(root))
        }) {
            self.status = tr!(
                "Сохранение отклонено: выберите место вне NotOffice-recovery; текст сохранён",
                "Save rejected: choose a location outside NotOffice-recovery; text kept"
            )
            .into();
            return false;
        }
        let Some(document) = self.text.as_mut() else {
            return false;
        };
        match document.save(&path) {
            Ok(()) => {
                if !self.clear_current_recovery() {
                    self.recovery_session = None;
                    self.recovery_snapshot_model = None;
                    self.recovery_snapshot_text = None;
                    self.recovery_snapshot_workbook = None;
                    self.recovery_last_attempt = None;
                }
                self.recovered_document = false;
                self.opened_path = Some(path.clone());
                self.conflict_destination = None;
                self.remember_recent(path.clone());
                self.status = tfm!(
                    "Текст надёжно сохранён: {}",
                    "Saved text safely to {}",
                    path.display()
                );
                true
            }
            Err(liteoffice::docx::DocxError::SourceChanged) => {
                self.conflict_destination = Some(path);
                self.status = tr!(
                    "Текст изменён на диске: запись заблокирована; правки сохранены",
                    "Text changed on disk: overwrite blocked; edits kept"
                )
                .into();
                false
            }
            Err(error) => {
                self.status = tfm!(
                    "Не удалось сохранить текст; правки сохранены: {error}",
                    "Text save failed; edits kept: {error}"
                );
                false
            }
        }
    }

    fn start_pdf_print(&mut self) {
        let Some(view) = &self.pdf else {
            return;
        };
        if self.print_receiver.is_some() {
            return;
        }
        if let Err(error) = self.pdf_print_settings.validate(view.page_count()) {
            self.status = error;
            return;
        }
        let snapshot = view.snapshot();
        let settings = self.pdf_print_settings;
        let (sender, receiver) = std::sync::mpsc::channel();
        self.print_receiver = Some(receiver);
        self.pdf_print_open = false;
        self.status = tr!(
            "Подготовка растров страниц PDF; далее выбор системного принтера",
            "Preparing PDF page rasters; system printer selection will follow"
        )
        .into();
        std::thread::spawn(move || {
            let result = match liteoffice::pdf_print::print_pdf(snapshot, settings) {
                Ok(print::PrintOutcome::Printed) => {
                    tr!("Задание печати PDF отправлено", "PDF print job sent").into()
                }
                Ok(print::PrintOutcome::Cancelled) => {
                    tr!("Печать PDF отменена", "PDF printing cancelled").into()
                }
                Err(error) => tfm!(
                    "Не удалось напечатать PDF: {error}",
                    "PDF print failed: {error}"
                ),
            };
            let _ = sender.send(result);
        });
    }
}

impl eframe::App for NotOfficeApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.show(ctx);
    }
}

impl NotOfficeApp {
    // Also exercised by headless tests with the real toolbar and preview.
    fn show(&mut self, ctx: &egui::Context) {
        self.handle_spreadsheet_undo_shortcut(ctx);
        self.handle_search_shortcuts(ctx);
        self.prepare_docx_focus(ctx);
        self.handle_edit_shortcuts(ctx);
        self.handle_spreadsheet_delete_shortcut(ctx);
        self.handle_table_clipboard_shortcuts(ctx);
        if let Some(receiver) = self.print_receiver.as_ref() {
            match receiver.try_recv() {
                Ok(message) => {
                    self.status = message;
                    self.print_receiver = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.status = tr!(
                        "Процесс печати завершился неожиданно",
                        "Print worker ended unexpectedly"
                    )
                    .into();
                    self.print_receiver = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                }
            }
        }
        ui_theme::apply(ctx, self.theme);
        if !self.fonts_initialized {
            install_windows_fonts(ctx);
            ui_theme::install(ctx, self.theme);
            self.fonts_initialized = true;
        } else if !ui_theme::fonts_ready(ctx) {
            // A full frame has passed: egui applied the new definitions while
            // beginning this one, so the semibold family can be used safely.
            ui_theme::mark_fonts_ready(ctx);
        }
        if self.zoom == 0.0 {
            self.zoom = 1.0;
        }
        if !self.decision_pending()
            && self.has_document()
            && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::P))
        {
            self.print_document();
        }
        let save_pressed =
            ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::S));
        if !self.decision_pending()
            && ctrl_s_save_requested(
                ctx.input(|input| input.modifiers.command),
                save_pressed,
                self.text.is_some()
                    || self.docx.is_some()
                    || self
                        .spreadsheet
                        .as_ref()
                        .is_some_and(|b| b.editor.is_some()),
                self.toolbar_text_field_is_focused(ctx),
            )
        {
            if ctx.input(|input| input.modifiers.shift) {
                self.save_document_as(false);
            } else {
                self.request_save();
            }
        }
        if !self.decision_pending()
            && ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::O))
        {
            self.open_document();
        }
        let new_window_pressed =
            ctx.input(|input| input.modifiers.command && input.key_pressed(egui::Key::N));
        if !self.decision_pending()
            && new_window_pressed
            && !self.toolbar_text_field_is_focused(ctx)
        {
            self.open_new_blank_window();
        }
        if !self.decision_pending() {
            if let Some((path, open_in_new_window)) = ctx.input(|input| {
                input
                    .raw
                    .dropped_files
                    .iter()
                    .find_map(|file| file.path.clone())
                    .map(|path| (path, input.modifiers.shift))
            }) {
                if open_in_new_window {
                    match spawn_liteoffice_window(Some(&path)) {
                        Ok(()) => {
                            self.status = tfm!(
                                "Перетащенный файл {} открыт в новом независимом окне",
                                "Opened dropped file {} in a new independent window",
                                path.display()
                            )
                        }
                        Err(error) => {
                            self.status = tfm!(
                                "Не удалось открыть перетащенный файл в новом окне: {error}",
                                "Open dropped file in new window failed: {error}"
                            )
                        }
                    }
                } else {
                    self.request_open_path(path);
                }
            }
        }
        let previous_page_settings = self.print_settings;
        let previous_zoom = self.zoom;
        let previous_theme = self.theme;
        self.handle_docx_selection_shortcuts(ctx);
        self.apply_ctrl_wheel_document_zoom(ctx);
        self.shell_command_bar(ctx);
        self.shell_document_strip(ctx);
        if self
            .spreadsheet
            .as_mut()
            .is_some_and(|book| book.take_range_paste_request())
        {
            self.paste_range_from_clipboard();
        }
        if self.print_settings != previous_page_settings
            || (self.zoom - previous_zoom).abs() > 0.0001
        {
            if let Err(err) = save_preferences(
                (previous_page_settings, previous_zoom, previous_theme),
                self.print_settings,
                self.zoom,
                self.theme,
            ) {
                self.status = tfm!(
                    "Не удалось сохранить настройки просмотра: {err}",
                    "Could not save view preferences: {err}"
                );
            }
        }
        self.refresh_search();
        let current_match = self.search_results.hits.get(self.current_match).cloned();
        let search_query = self.search_query.clone();
        let before_edit = self.docx.as_ref().map(|package| package.model.clone());
        let before_text_edit = self.text.as_ref().map(|document| document.text.clone());
        if self.docx_focus_pending {
            // A fresh editable document starts at its first text field; an old
            // Find query must not scroll the newly focused caret out of view.
            self.search_scroll_requested = false;
        }
        // This plan is deliberately calculated after the toolbar. Opening a
        // second document is synchronous and may replace the model in that
        // toolbar interaction; using a plan made before it caused stale ranges
        // to index the new model and could crash the process.
        let page_settings = self.print_settings.sanitized();
        let points_per_mm = 72.0 / 25.4;
        let (page_width_mm, page_height_mm) = page_settings.page_size_mm();
        let page_width = page_width_mm * points_per_mm * self.zoom;
        let page_height = page_height_mm * points_per_mm * self.zoom;
        let margin_left = page_settings.margin_mm * points_per_mm * self.zoom;
        let margin_right = margin_left;
        let margin_top = margin_left;
        let margin_bottom = margin_left;
        let content_width = page_width - margin_left - margin_right;
        let content_height = page_height - margin_top - margin_bottom;
        let (flow_blocks, page_ranges) = self
            .docx
            .as_ref()
            .map(|package| {
                docx_preview_plan(&package.model, &package.images, page_settings, self.zoom)
            })
            .unwrap_or_default();

        self.shell_status_bar(ctx);

        let document_panel = egui::CentralPanel::default().show(ctx, |ui| {
            if self.decision_pending() {
                ui.disable();
            }
            if let Some(document) = self.text.as_mut() {
                egui::ScrollArea::vertical()
                    .id_source(("plain-text-scroll", self.docx_editor_generation))
                    .show(ui, |ui| {
                        ui.visuals_mut().extreme_bg_color = ui_theme::text_bg();
                        ui.visuals_mut().selection.bg_fill = ui_theme::selection();
                        ui.visuals_mut().text_cursor.stroke =
                            egui::Stroke::new(2.0_f32, ui_theme::text_ink());
                        let id = egui::Id::new(("plain-text", self.docx_editor_generation));
                        if self.text_focus_pending && ui.is_enabled() && ui.input(|i| i.focused) {
                            ui.memory_mut(|m| m.request_focus(id));
                            self.text_focus_pending = false;
                        }
                        if self.select_all_text {
                            let mut state = egui::text_edit::TextEditState::load(ui.ctx(), id)
                                .unwrap_or_default();
                            let end = document.text.chars().count();
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(0),
                                    egui::text::CCursor::new(end),
                                )));
                            state.store(ui.ctx(), id);
                            ui.memory_mut(|m| m.request_focus(id));
                            self.select_all_text = false;
                        }
                        let response = ui.add_sized(
                            ui.available_size().max(egui::vec2(100.0, 300.0)),
                            egui::TextEdit::multiline(&mut document.text)
                                .id(id)
                                .font(egui::FontId::monospace(14.0 * self.zoom))
                                .text_color(ui_theme::text_ink())
                                .char_limit(8 * 1024 * 1024),
                        );
                        self.text_input_id = Some(response.id);
                    });
            } else if let Some(pdf) = self.pdf.as_mut() {
                pdf.show(ui, self.zoom);
            } else if self.spreadsheet.is_some() {
                let before_edit = self.workbook_snapshot();
                let changed = {
                    let book = self.spreadsheet.as_mut().unwrap();
                    let formats = book.formats.clone();
                    let highlights = book.highlights.clone();
                    let view = SpreadsheetSearchView {
                        query: &search_query,
                        target: current_match.as_ref().and_then(|hit| match hit.target {
                            SearchTarget::Cell { sheet, row, column }
                                if sheet == book.selected_sheet =>
                            {
                                Some((row, column))
                            }
                            _ => None,
                        }),
                        anchor: book.preview_anchor,
                        scroll: self.search_scroll_requested,
                        formats: Some((&formats, book.selected_sheet)),
                        editable: book.editor.is_some(),
                        decimals: book.decimal_places,
                        freeze_top_row: book.freeze_top_row,
                        freeze_left_column: book.freeze_left_column,
                        highlights: Some(&highlights),
                    };
                    book.show_grid(ui, self.zoom, &view, self.docx_editor_generation)
                };
                if changed {
                    self.record_workbook_edit(before_edit);
                    self.search_dirty = true;
                }
                if let Some((row, column, text)) = self
                    .spreadsheet
                    .as_mut()
                    .and_then(SpreadsheetDocument::take_cell_edit_request)
                {
                    self.cell_edit_dialog = Some(CellEditDialog {
                        row,
                        column,
                        text,
                        focus: true,
                    });
                }
            } else {
                egui::ScrollArea::both()
                    .id_source(("document-scroll", self.docx_editor_generation))
                    .show(ui, |ui| {
                        let clip_size = ui.clip_rect().size();
                        let viewport_changed = self.docx_view_clip_size != Some(clip_size);
                        self.docx_view_clip_size = Some(clip_size);
                        // Center only if the entire page fits. Centering an
                        // oversized page puts its left edge at negative x, beyond
                        // the scroll area's non-negative offsets and hit targets.
                        let align = if page_width + 16.0 <= ui.available_width() {
                            egui::Align::Center
                        } else {
                            egui::Align::Min
                        };
                        ui.with_layout(egui::Layout::top_down(align), |ui| {
                            if let Some(package) = self.docx.as_mut() {
                                if self.docx_textures.len() != package.images.len() {
                                    self.docx_textures = package
                                        .images
                                        .iter()
                                        .enumerate()
                                        .map(|(index, image)| {
                                            decode_document_image(
                                                ui.ctx(),
                                                &image.bytes,
                                                format!("docx-raster-{index}"),
                                            )
                                        })
                                        .collect();
                                }
                                let page_count = page_ranges.len();
                                for (page_index, block_range) in page_ranges.iter().enumerate() {
                                    if page_index > 0 {
                                        ui.add_space(24.0 * self.zoom);
                                    }
                                    egui::Frame::none()
                                        .fill(ui_theme::page())
                                        .stroke(egui::Stroke::new(
                                            1.0_f32,
                                            egui::Color32::from_rgb(0xD8, 0xD1, 0xC3),
                                        ))
                                        .shadow(egui::epaint::Shadow {
                                            offset: egui::vec2(0.0, 5.0),
                                            blur: 18.0,
                                            spread: 0.0,
                                            color: egui::Color32::from_black_alpha(34),
                                        })
                                        .show(ui, |ui| {
                                            // Without an explicit maximum the frame
                                            // stretches to the whole scroll viewport
                                            // instead of keeping A4 geometry. Only the
                                            // width is clamped: a max height makes the
                                            // horizontal content row centre itself
                                            // vertically inside the page.
                                            ui.set_max_width(page_width);
                                            ui.set_min_size(egui::vec2(page_width, page_height));
                                            ui.add_space(margin_top);
                                            ui.horizontal(|ui| {
                                                ui.add_space(margin_left);
                                                ui.allocate_ui_with_layout(
                                                    egui::vec2(content_width, content_height),
                                                    egui::Layout::top_down(egui::Align::Min),
                                                    |ui| {
                                                        let mut paragraph_options =
                                                            ParagraphRenderOptions {
                                                                content_width,
                                                                zoom: self.zoom,
                                                                query: &search_query,
                                                                current_match: current_match
                                                                    .clone(),
                                                                scroll_to_match: self
                                                                    .search_scroll_requested,
                                                                selected_fragment_id: &mut self
                                                                    .selected_fragment_id,
                                                                select_all_docx: &mut self
                                                                    .select_all_docx,
                                                                editor_generation: self
                                                                    .docx_editor_generation,
                                                                focus_pending: &mut self
                                                                    .docx_focus_pending,
                                                                reveal_focused: viewport_changed,
                                                            };
                                                        for block_index in block_range.clone() {
                                                            match &flow_blocks[block_index] {
                                                                FlowBlock::Image {
                                                                    index,
                                                                    size_pt,
                                                                    ..
                                                                } => {
                                                                    render_docx_image(
                                                                        ui,
                                                                        self.docx_textures
                                                                            .get(*index)
                                                                            .and_then(
                                                                                Option::as_ref,
                                                                            ),
                                                                        *size_pt,
                                                                        self.zoom,
                                                                    );
                                                                }
                                                                FlowBlock::Paragraph(
                                                                    paragraph_index,
                                                                ) => {
                                                                    render_paragraph(
                                                                        ui,
                                                                        &mut package
                                                                            .model
                                                                            .paragraphs
                                                                            [*paragraph_index],
                                                                        *paragraph_index,
                                                                        &mut paragraph_options,
                                                                    );
                                                                }
                                                                FlowBlock::TableRow {
                                                                    row,
                                                                    grid_widths,
                                                                    column_count,
                                                                } => {
                                                                    render_table_row(
                                                                        ui,
                                                                        &mut package.model,
                                                                        row,
                                                                        grid_widths,
                                                                        *column_count,
                                                                        &mut paragraph_options,
                                                                    );
                                                                }
                                                            }
                                                            if block_index + 1 < block_range.end {
                                                                ui.add_space(
                                                                    PARAGRAPH_GAP * self.zoom,
                                                                );
                                                            }
                                                        }
                                                    },
                                                );
                                                ui.add_space(margin_right);
                                            });
                                            ui.add_space(margin_bottom);
                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.weak(tfm!(
                                                        "Страница {} из {}",
                                                        "Page {} of {}",
                                                        page_index + 1,
                                                        page_count
                                                    ));
                                                },
                                            );
                                        });
                                }
                            } else if let Some(package) = self.legacy_doc.as_ref() {
                                if self.legacy_doc_textures.is_empty() && !package.images.is_empty()
                                {
                                    self.legacy_doc_textures =
                                        decode_doc_gallery(ui.ctx(), package);
                                }
                                render_legacy_doc(
                                    ui,
                                    package,
                                    &self.legacy_doc_textures,
                                    &mut self.doc_image_anchors,
                                    &mut self.doc_image_widths,
                                    &LegacyViewOptions {
                                        zoom: self.zoom,
                                        page_settings: self.print_settings,
                                        query: &search_query,
                                        current_hit: current_match.as_ref(),
                                        scroll_to_hit: self.search_scroll_requested,
                                    },
                                );
                                let edit_click = ui
                                    .interact(
                                        ui.min_rect(),
                                        ui.id().with("legacy-doc-edit-double-click"),
                                        egui::Sense::click(),
                                    )
                                    .double_clicked();
                                if edit_click {
                                    self.request_legacy_edit();
                                }
                            } else {
                                self.shell_empty_state(ui);
                            }
                        });
                    });
            }
        });
        self.document_view_rect = Some(document_panel.response.rect);
        self.search_scroll_requested = false;
        if let Some(before_edit) = before_edit {
            self.record_docx_edit(before_edit);
        }
        self.record_text_edit(before_text_edit);
        self.sync_recovery(false);
        if self.is_dirty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        // Inspect dirty state after this frame's editor input. A final keypress
        // and the native close request can arrive in the same frame.
        if ctx.input(|input| input.viewport().close_requested()) && !self.request_close() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.pdf_print_open {
            let count = self.pdf.as_ref().map(|p| p.page_count()).unwrap_or(1);
            egui::Window::new(tr!("Печать PDF", "Print PDF")).collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).resizable(false).show(ctx,|ui| {
                ui.label(tfm!("Страниц: {count}. В следующем диалоге выбирается принтер. Ориентация страниц источника определяется автоматически.", "{count} pages. The next dialog selects the printer. Source page orientation is automatic."));
                ui.horizontal(|ui| {
                    ui.label(tr!("С", "From"));ui.add(egui::DragValue::new(&mut self.pdf_print_settings.first).range(1..=count));
                    ui.label(tr!("По", "To"));ui.add(egui::DragValue::new(&mut self.pdf_print_settings.last).range(1..=count));
                });
                let mut fit=self.pdf_print_settings.percent.is_none();
                if ui.checkbox(&mut fit,tr!("Вписать в область печати", "Fit to printable area")).changed() {self.pdf_print_settings.percent=if fit {None} else {Some(100)};}
                if let Some(percent)=self.pdf_print_settings.percent.as_mut() {
                    ui.horizontal(|ui| {ui.label(tr!("Масштаб, % (100 = реальный размер)", "Scale % (100 = actual size)"));ui.add(egui::DragValue::new(percent).range(10..=400));});
                    ui.weak(tr!("Слишком крупное содержимое может обрезаться принтером при реальном или заданном масштабе.", "Oversized content may be clipped by the printer at actual/custom size."));
                }
                let valid=self.pdf_print_settings.validate(count).is_ok();
                if !valid {ui.colored_label(egui::Color32::YELLOW,tr!("Укажите корректный диапазон страниц.", "Choose a valid first/last page range."));}
                ui.horizontal(|ui| {
                    if ui.add_enabled(valid,egui::Button::new(tr!("Выбрать принтер и печатать…", "Choose printer and print…"))).clicked() {self.start_pdf_print();}
                    if ui.button(tr!("Отмена", "Cancel")).clicked() {self.pdf_print_open=false;self.status=tr!("Печать PDF отменена", "PDF printing cancelled").into();}
                });
            });
        } else if !self.recovery_candidates.is_empty() {
            egui::Window::new(tr!("Восстановление после прерванного сеанса", "Recover unsaved documents after an interrupted session"))
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).resizable(true).show(ctx, |ui| {
                    ui.label(tfm!("Прерванных сеансов: {}. Активные окна исключены.", "{} interrupted session(s). Active windows are excluded.", self.recovery_candidates.len()));
                    let session = &self.recovery_candidates[0];
                    ui.label(tfm!("Оригинал: {}", "Original: {}", session.origin));
                    ui.label(tfm!("Восстановление: {}", "Recovery: {}", session.directory().display()));
                    ui.label(tr!("Оригинал не изменится. Для восстановления потребуется «Сохранить как». Остальные сеансы останутся доступны для другого окна или запуска.", "The original will not be changed. Restoring requires Save As. Other sessions stay available for another window or launch."));
                    if self.recovery_discard_confirmation {
                        ui.colored_label(egui::Color32::YELLOW, tr!("Удалить файлы восстановления этого сеанса безвозвратно?", "Permanently discard this session's recovery files?"));
                        if ui.button(tr!("Да — удалить выбранное восстановление", "Yes — discard selected recovery")).clicked() { self.discard_first_recovery(); }
                        if ui.button(tr!("Нет — сохранить восстановление", "No — keep recovery")).clicked() { self.recovery_discard_confirmation = false; }
                    } else {
                        if ui.button(tr!("Восстановить несохранённую копию", "Restore unsaved copy")).clicked() { self.restore_first_recovery(); }
                        if ui.button(tr!("Удалить выбранное восстановление…", "Discard selected recovery…")).clicked() { self.recovery_discard_confirmation = true; }
                        if ui.button(tr!("Позже — сохранить все файлы восстановления", "Later — keep all recovery files")).clicked() { self.recovery_candidates.clear(); }
                    }
                });
        } else if self.conflict_destination.is_some() {
            egui::Window::new(tr!("Документ изменён на диске", "Document changed on disk"))
                .collapsible(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(tr!(
                        "Файл изменён вне этого окна. Запись поверх него заблокирована.",
                        "The file changed outside this window. Saving over it was blocked."
                    ));
                    ui.label(tr!(
                        "Ваш несохранённый текст остаётся в этом окне.",
                        "Your unsaved text is still in this window."
                    ));
                    if let Some(path) = &self.conflict_destination {
                        ui.label(path.display().to_string());
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(tr!("Сохранить копию…", "Save a copy…")).clicked()
                            && self.save_document_as(true)
                        {
                            self.resume_pending_action(ctx);
                        }
                        if ui
                            .button(tr!(
                                "Перезагрузить с диска (отбросить мои правки)",
                                "Reload from disk (discard my edits)"
                            ))
                            .clicked()
                        {
                            if let Some(path) = self.conflict_destination.clone() {
                                self.open_path(path);
                            }
                        }
                        if ui
                            .button(tr!(
                                "Отмена — оставить мои правки",
                                "Cancel — keep my edits"
                            ))
                            .clicked()
                        {
                            self.cancel_pending_action();
                        }
                    });
                });
        } else if let Some(action) = self.pending_action.clone() {
            egui::Window::new(tr!(
                "Несохранённые правки документа",
                "Unsaved document edits"
            ))
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(tr!(
                    "В текущем документе есть несохранённые изменения.",
                    "The current document has unsaved changes."
                ));
                ui.label(match action {
                    PendingAction::Open(_) => tr!(
                        "Сохранить перед открытием другого документа?",
                        "Save before opening another document?"
                    ),
                    PendingAction::Close => tr!(
                        "Сохранить перед закрытием окна?",
                        "Save before closing this window?"
                    ),
                });
                ui.horizontal(|ui| {
                    if ui.button(tr!("Сохранить", "Save")).clicked() && self.save_document()
                    {
                        self.resume_pending_action(ctx);
                    }
                    if ui.button(tr!("Не сохранять", "Don't save")).clicked() {
                        self.resume_pending_action(ctx);
                    }
                    if ui.button(tr!("Отмена", "Cancel")).clicked() {
                        self.cancel_pending_action();
                    }
                });
            });
        }
        self.show_cell_edit_dialog(ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
    }
}

#[cfg(test)]
mod app_tests;

#[cfg(test)]
mod preferences_tests {
    use super::*;
    #[test]
    fn default_preferences_are_within_limits() {
        let (settings, zoom, _theme) = load_preferences();
        assert!((5.0..=40.0).contains(&settings.margin_mm));
        assert!((MIN_VIEW_ZOOM..=MAX_VIEW_ZOOM).contains(&zoom));
    }

    #[test]
    fn reading_zoom_supports_large_values_and_rejects_invalid_values() {
        assert_eq!(sanitize_view_zoom(3.0), 3.0);
        assert_eq!(sanitize_view_zoom(9.0), MAX_VIEW_ZOOM);
        assert_eq!(sanitize_view_zoom(0.1), MIN_VIEW_ZOOM);
        assert_eq!(sanitize_view_zoom(f32::NAN), DEFAULT_VIEW_ZOOM);
        assert_eq!(sanitize_view_zoom(f32::INFINITY), DEFAULT_VIEW_ZOOM);
    }
}
