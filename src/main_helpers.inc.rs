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


#[derive(serde::Serialize, serde::Deserialize)]
struct IsolatedSpreadsheet {
    model: liteoffice::xlsx::SpreadsheetModel,
    sheets: Vec<liteoffice::xlsx::SpreadsheetModel>,
    formats: Vec<(
        liteoffice::workbook_edit::CellKey,
        liteoffice::workbook_edit::CellFormat,
    )>,
    formulas: Vec<(liteoffice::workbook_edit::CellKey, String)>,
}

impl IsolatedSpreadsheet {
    fn into_package(self) -> XlsxPackage {
        XlsxPackage {
            model: self.model,
            sheets: self.sheets,
            formats: self.formats.into_iter().collect(),
            formulas: self.formulas.into_iter().collect(),
        }
    }
}

#[cfg(test)]
fn parser_isolation_disabled() -> bool { true }
#[cfg(not(test))]
fn parser_isolation_disabled() -> bool { false }

fn xlsx_worker_mode() -> Option<i32> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().and_then(|value| value.to_str()) != Some("--xlsx-parse-worker") {
        return None;
    }
    if args.len() != 3 { // flag + input + output
        return Some(2);
    }
    let input = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    let result = XlsxPackage::open(&input);
    let write = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)?;
        match result {
            Ok(package) => {
                let isolated = IsolatedSpreadsheet {
                    model: package.model,
                    sheets: package.sheets,
                    formats: package.formats.into_iter().collect(),
                    formulas: package.formulas.into_iter().collect(),
                };
                serde_json::to_writer(&mut file, &isolated)
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            }
            Err(error) => {
                use std::io::Write as _;
                file.write_all(error.to_string().as_bytes())?;
                return Err(std::io::Error::other(error.to_string()));
            }
        }
        file.flush()?;
        Ok(())
    })();
    Some(if write.is_ok() { 0 } else { 1 })
}

fn open_xlsx_isolated(path: &Path) -> Result<XlsxPackage, String> {
    if parser_isolation_disabled() {
        return XlsxPackage::open(path).map_err(|error| error.to_string());
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let output = std::env::temp_dir().join(format!(
        "notoffice-xlsx-{}-{nonce}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    let mut command = std::process::Command::new(executable);
    command
        .arg("--xlsx-parse-worker")
        .arg(path)
        .arg(&output)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < std::time::Duration::from_secs(30) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(error) => {
                let _ = child.kill();
                return Err(format!("XLSX parser process failed: {error}"));
            }
        }
    };
    let bytes = std::fs::read(&output).unwrap_or_default();
    let _ = std::fs::remove_file(&output);
    match status {
        Some(status) if status.success() => {
            let isolated: IsolatedSpreadsheet = serde_json::from_slice(&bytes)
                .map_err(|error| format!("XLSX parser returned invalid data: {error}"))?;
            Ok(isolated.into_package())
        }
        Some(_) => Err(if bytes.is_empty() {
            "XLSX parser process failed".into()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        }),
        None => Err("XLSX parser timed out".into()),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct IsolatedDocx {
    model: liteoffice::docx::DocumentModel,
    images: Vec<liteoffice::docx::DocxImage>,
}


fn xls_worker_mode() -> Option<i32> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().and_then(|value| value.to_str()) != Some("--xls-parse-worker") {
        return None;
    }
    if args.len() != 3 {
        return Some(2);
    }
    let input = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    let result = XlsPackage::open(&input);
    let write = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)?;
        match result {
            Ok(package) => {
                let isolated = IsolatedSpreadsheet {
                    model: package.model,
                    sheets: package.sheets,
                    formats: Vec::new(),
                    formulas: Vec::new(),
                };
                serde_json::to_writer(&mut file, &isolated)
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            }
            Err(error) => {
                file.write_all(error.to_string().as_bytes())?;
                return Err(std::io::Error::other(error.to_string()));
            }
        }
        file.flush()?;
        Ok(())
    })();
    Some(if write.is_ok() { 0 } else { 1 })
}

fn csv_worker_mode() -> Option<i32> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().and_then(|value| value.to_str()) != Some("--csv-parse-worker") {
        return None;
    }
    if args.len() != 3 {
        return Some(2);
    }
    let input = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    let result = liteoffice::csv_import::open(&input);
    let write = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)?;
        match result {
            Ok(model) => {
                let isolated = IsolatedSpreadsheet {
                    model: model.clone(),
                    sheets: vec![model],
                    formats: Vec::new(),
                    formulas: Vec::new(),
                };
                serde_json::to_writer(&mut file, &isolated)
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            }
            Err(error) => {
                file.write_all(error.as_bytes())?;
                return Err(std::io::Error::other(error));
            }
        }
        file.flush()?;
        Ok(())
    })();
    Some(if write.is_ok() { 0 } else { 1 })
}

fn run_isolated_worker(path: &Path, flag: &str, label: &str) -> Result<Vec<u8>, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let output = std::env::temp_dir().join(format!(
        "notoffice-{}-{}-{nonce}.json",
        label,
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    let mut command = std::process::Command::new(executable);
    command
        .arg(flag)
        .arg(path)
        .arg(&output)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < std::time::Duration::from_secs(30) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(error) => {
                let _ = child.kill();
                return Err(format!("{label} parser process failed: {error}"));
            }
        }
    };
    let bytes = std::fs::read(&output).unwrap_or_default();
    let _ = std::fs::remove_file(&output);
    match status {
        Some(status) if status.success() => Ok(bytes),
        Some(_) => Err(if bytes.is_empty() {
            format!("{label} parser process failed")
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        }),
        None => Err(format!("{label} parser timed out")),
    }
}

fn open_xls_isolated(path: &Path) -> Result<XlsPackage, String> {
    if parser_isolation_disabled() {
        return XlsPackage::open(path).map_err(|error| error.to_string());
    }
    let bytes = run_isolated_worker(path, "--xls-parse-worker", "xls")?;
    let isolated: IsolatedSpreadsheet = serde_json::from_slice(&bytes)
        .map_err(|error| format!("XLS parser returned invalid data: {error}"))?;
    let package = isolated.into_package();
    Ok(XlsPackage {
        model: package.model,
        sheets: package.sheets,
    })
}

fn open_csv_isolated(path: &Path) -> Result<SpreadsheetModel, String> {
    if parser_isolation_disabled() {
        return liteoffice::csv_import::open(path);
    }
    let bytes = run_isolated_worker(path, "--csv-parse-worker", "csv")?;
    let isolated: IsolatedSpreadsheet = serde_json::from_slice(&bytes)
        .map_err(|error| format!("CSV parser returned invalid data: {error}"))?;
    Ok(isolated.model)
}


#[derive(serde::Serialize, serde::Deserialize)]
struct IsolatedLegacyDoc {
    package: liteoffice::doc::LegacyDocPackage,
}

fn doc_worker_mode() -> Option<i32> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().and_then(|value| value.to_str()) != Some("--doc-parse-worker") {
        return None;
    }
    if args.len() != 3 {
        return Some(2);
    }
    let input = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    let result = LegacyDocPackage::open(&input);
    let write = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)?;
        match result {
            Ok(package) => {
                let isolated = IsolatedLegacyDoc { package };
                serde_json::to_writer(&mut file, &isolated)
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            }
            Err(error) => {
                file.write_all(error.to_string().as_bytes())?;
                return Err(std::io::Error::other(error.to_string()));
            }
        }
        file.flush()?;
        Ok(())
    })();
    Some(if write.is_ok() { 0 } else { 1 })
}

fn open_doc_isolated(path: &Path) -> Result<LegacyDocPackage, String> {
    if parser_isolation_disabled() {
        return LegacyDocPackage::open(path).map_err(|error| error.to_string());
    }
    let bytes = run_isolated_worker(path, "--doc-parse-worker", "doc")?;
    let isolated: IsolatedLegacyDoc = serde_json::from_slice(&bytes)
        .map_err(|error| format!("DOC parser returned invalid data: {error}"))?;
    Ok(isolated.package)
}



#[derive(serde::Serialize, serde::Deserialize)]
struct IsolatedXlsxSave {
    origin: PathBuf,
    saved_path: Option<PathBuf>,
    sheets: Vec<liteoffice::xlsx::SpreadsheetModel>,
    formats: Vec<(
        liteoffice::workbook_edit::CellKey,
        liteoffice::workbook_edit::CellFormat,
    )>,
    formulas: Vec<(liteoffice::workbook_edit::CellKey, String)>,
}

fn xlsx_save_worker_mode() -> Option<i32> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().and_then(|value| value.to_str()) != Some("--xlsx-save-worker") {
        return None;
    }
    if args.len() != 4 {
        return Some(2);
    }
    let destination = PathBuf::from(&args[1]);
    let data_path = PathBuf::from(&args[2]);
    let result_path = PathBuf::from(&args[3]);
    let result = (|| -> Result<(), String> {
        let data: IsolatedXlsxSave = serde_json::from_slice(
            &std::fs::read(&data_path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let mut editor = liteoffice::workbook_edit::WorkbookEditor::new(
            data.origin.clone(),
            &data.sheets,
        );
        editor.formats = data.formats.into_iter().collect();
        editor.formulas = data.formulas.into_iter().collect();
        if let Some(saved_path) = data.saved_path.as_deref() {
            editor
                .mark_saved(saved_path, &data.sheets)
                .map_err(|error| error.to_string())?;
        }
        editor
            .save(&destination, &data.sheets)
            .map_err(|error| error.to_string())
    })();
    let message = match &result {
        Ok(()) => "ok".to_owned(),
        Err(error) => error.clone(),
    };
    let _ = std::fs::write(&result_path, message);
    Some(if result.is_ok() { 0 } else { 1 })
}

#[allow(clippy::too_many_arguments)]
fn save_xlsx_isolated(
    origin: &Path,
    saved_path: Option<&Path>,
    destination: &Path,
    sheets: &[SpreadsheetModel],
    formats: &std::collections::BTreeMap<
        liteoffice::workbook_edit::CellKey,
        liteoffice::workbook_edit::CellFormat,
    >,
    formulas: &std::collections::BTreeMap<liteoffice::workbook_edit::CellKey, String>,
) -> Result<(), String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let data_path = std::env::temp_dir().join(format!(
        "notoffice-xlsx-save-{}-{nonce}.json",
        std::process::id()
    ));
    let result_path = std::env::temp_dir().join(format!(
        "notoffice-xlsx-save-result-{}-{nonce}.txt",
        std::process::id()
    ));
    let data = IsolatedXlsxSave {
        origin: origin.to_path_buf(),
        saved_path: saved_path.map(Path::to_path_buf),
        sheets: sheets.to_vec(),
        formats: formats.iter().map(|(key, value)| (*key, *value)).collect(),
        formulas: formulas.iter().map(|(key, value)| (*key, value.clone())).collect(),
    };
    std::fs::write(
        &data_path,
        serde_json::to_vec(&data).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut command = std::process::Command::new(executable);
    command
        .arg("--xlsx-save-worker")
        .arg(destination)
        .arg(&data_path)
        .arg(&result_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < std::time::Duration::from_secs(60) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(error) => {
                let _ = child.kill();
                let _ = std::fs::remove_file(&data_path);
                let _ = std::fs::remove_file(&result_path);
                return Err(format!("XLSX save process failed: {error}"));
            }
        }
    };
    let message = std::fs::read_to_string(&result_path).unwrap_or_default();
    let _ = std::fs::remove_file(&data_path);
    let _ = std::fs::remove_file(&result_path);
    match status {
        Some(status) if status.success() => Ok(()),
        Some(_) => Err(if message.is_empty() {
            "XLSX save process failed".into()
        } else {
            message
        }),
        None => Err("XLSX save timed out".into()),
    }
}

fn docx_save_worker_mode() -> Option<i32> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().and_then(|value| value.to_str()) != Some("--docx-save-worker") {
        return None;
    }
    if args.len() != 5 {
        return Some(2);
    }
    let source = PathBuf::from(&args[1]);
    let destination = PathBuf::from(&args[2]);
    let model_path = PathBuf::from(&args[3]);
    let result_path = PathBuf::from(&args[4]);
    let result = (|| -> Result<(), String> {
        let model: DocumentModel = serde_json::from_slice(
            &std::fs::read(&model_path).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let mut package = DocxPackage::open(&source).map_err(|error| error.to_string())?;
        package.model = model;
        package
            .save_as(&destination)
            .map_err(|error| error.to_string())
    })();
    let message = match &result {
        Ok(()) => "ok".to_owned(),
        Err(error) => error.clone(),
    };
    let _ = std::fs::write(&result_path, message);
    Some(if result.is_ok() { 0 } else { 1 })
}

fn save_docx_isolated(source: &Path, destination: &Path, model: &DocumentModel) -> Result<(), String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let model_path = std::env::temp_dir().join(format!(
        "notoffice-docx-save-model-{}-{nonce}.json",
        std::process::id()
    ));
    let result_path = std::env::temp_dir().join(format!(
        "notoffice-docx-save-result-{}-{nonce}.txt",
        std::process::id()
    ));
    std::fs::write(
        &model_path,
        serde_json::to_vec(model).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut command = std::process::Command::new(executable);
    command
        .arg("--docx-save-worker")
        .arg(source)
        .arg(destination)
        .arg(&model_path)
        .arg(&result_path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < std::time::Duration::from_secs(60) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(error) => {
                let _ = child.kill();
                let _ = std::fs::remove_file(&model_path);
                let _ = std::fs::remove_file(&result_path);
                return Err(format!("DOCX save process failed: {error}"));
            }
        }
    };
    let message = std::fs::read_to_string(&result_path).unwrap_or_default();
    let _ = std::fs::remove_file(&model_path);
    let _ = std::fs::remove_file(&result_path);
    match status {
        Some(status) if status.success() => Ok(()),
        Some(_) => Err(if message.is_empty() {
            "DOCX save process failed".into()
        } else {
            message
        }),
        None => Err("DOCX save timed out".into()),
    }
}

fn docx_worker_mode() -> Option<i32> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().and_then(|value| value.to_str()) != Some("--docx-parse-worker") {
        return None;
    }
    if args.len() != 3 {
        return Some(2);
    }
    let input = PathBuf::from(&args[1]);
    let output = PathBuf::from(&args[2]);
    let result = DocxPackage::open(&input);
    let write = (|| -> std::io::Result<()> {
        use std::io::Write as _;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&output)?;
        match result {
            Ok(package) => {
                let isolated = IsolatedDocx {
                    model: package.model,
                    images: package.images,
                };
                serde_json::to_writer(&mut file, &isolated)
                    .map_err(|error| std::io::Error::other(error.to_string()))?;
            }
            Err(error) => {
                file.write_all(error.to_string().as_bytes())?;
                return Err(std::io::Error::other(error.to_string()));
            }
        }
        file.flush()?;
        Ok(())
    })();
    Some(if write.is_ok() { 0 } else { 1 })
}

fn open_docx_isolated(path: &Path) -> Result<DocxPackage, String> {
    if parser_isolation_disabled() {
        return DocxPackage::open(path).map_err(|error| error.to_string());
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let output = std::env::temp_dir().join(format!(
        "notoffice-docx-{}-{nonce}.json",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&output);
    let mut command = std::process::Command::new(executable);
    command
        .arg("--docx-parse-worker")
        .arg(path)
        .arg(&output)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < std::time::Duration::from_secs(30) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Err(error) => {
                let _ = child.kill();
                return Err(format!("DOCX parser process failed: {error}"));
            }
        }
    };
    let bytes = std::fs::read(&output).unwrap_or_default();
    let _ = std::fs::remove_file(&output);
    match status {
        Some(status) if status.success() => {
            let isolated: IsolatedDocx = serde_json::from_slice(&bytes)
                .map_err(|error| format!("DOCX parser returned invalid data: {error}"))?;
            DocxPackage::from_isolated_model(path.to_path_buf(), isolated.model, isolated.images)
                .map_err(|error| error.to_string())
        }
        Some(_) => Err(if bytes.is_empty() {
            "DOCX parser process failed".into()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        }),
        None => Err("DOCX parser timed out".into()),
    }
}

