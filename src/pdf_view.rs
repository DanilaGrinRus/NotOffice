//! Only bitmap textures are exposed to egui. PDF actions have no UI handlers.
use eframe::egui;
use liteoffice::pdf::PdfSession;
use liteoffice::{tfm, tr};
use std::collections::VecDeque;
use std::path::Path;

pub struct PdfView {
    session: PdfSession,
    selected: usize,
    pending: Option<usize>,
    cache: VecDeque<(usize, egui::TextureHandle)>,
    error: Option<String>,
}

impl PdfView {
    pub fn snapshot(&self) -> std::sync::Arc<Vec<u8>> {
        self.session.snapshot()
    }
    pub fn page_count(&self) -> usize {
        self.session.pages.len()
    }
    pub fn open(path: &Path) -> Result<Self, String> {
        Ok(Self {
            session: PdfSession::open(path)?,
            selected: 0,
            pending: None,
            cache: VecDeque::new(),
            error: None,
        })
    }

    pub fn show(&mut self, ui: &mut egui::Ui, zoom: f32) {
        ui.label(tr!("PDF — растровый просмотр и печать. Скрипты, ссылки, вложения и действия форм не выполняются. Поиск по тексту и редактирование недоступны.", "PDF — raster view and printing. Scripts, links, attachments and form actions are not run. Text search and editing are not available."));
        let previous = self.selected;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(self.selected > 0, egui::Button::new("◀ Previous page"))
                .clicked()
            {
                self.selected -= 1;
            }
            let mut page = self.selected + 1;
            if ui
                .add(
                    egui::DragValue::new(&mut page)
                        .range(1..=self.session.pages.len())
                        .prefix(tr!("Страница ", "Page ")),
                )
                .changed()
            {
                self.selected = page.saturating_sub(1).min(self.session.pages.len() - 1);
            }
            ui.label(tfm!("из {}", "of {}", self.session.pages.len()));
            if ui
                .add_enabled(
                    self.selected + 1 < self.session.pages.len(),
                    egui::Button::new(tr!("Следующая страница ▶", "Next page ▶")),
                )
                .clicked()
            {
                self.selected += 1;
            }
        });
        if previous != self.selected {
            self.error = None;
        }
        loop {
            let (index, result) = match self.session.responses.try_recv() {
                Ok(result) => result,
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.error = Some(
                        tr!(
                            "Процесс отрисовки PDF остановлен; откройте документ заново",
                            "PDF rendering worker stopped; reopen the document"
                        )
                        .into(),
                    );
                    break;
                }
            };
            self.pending = None;
            match result.and_then(|png| decode_png(ui.ctx(), index, &png)) {
                Ok(texture) => {
                    self.cache.retain(|(cached, _)| *cached != index);
                    self.cache.push_back((index, texture));
                    while self.cache.len() > 2 {
                        self.cache.pop_front();
                    }
                }
                Err(error) => {
                    if index == self.selected {
                        self.error = Some(error);
                    }
                }
            }
        }
        if let Some((_, texture)) = self.cache.iter().find(|(index, _)| *index == self.selected) {
            let size = self.session.pages[self.selected];
            egui::ScrollArea::both()
                .id_source(("pdf-raster", self.selected))
                .show(ui, |ui| {
                    egui::Frame::none()
                        .fill(egui::Color32::WHITE)
                        .show(ui, |ui| {
                            ui.image((texture.id(), egui::vec2(size[0] * zoom, size[1] * zoom)));
                        });
                });
        } else if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::RED, error);
            if ui
                .button(tr!("Повторить отрисовку страницы", "Retry rendering page"))
                .clicked()
            {
                self.error = None;
            }
        } else {
            ui.spinner();
            ui.label(tr!("Отрисовка страницы PDF…", "Rendering PDF page…"));
        }
        if self.pending.is_none()
            && self.error.is_none()
            && !self.cache.iter().any(|(index, _)| *index == self.selected)
        {
            match self.session.request_page(self.selected) {
                Ok(()) => self.pending = Some(self.selected),
                Err(error) => self.error = Some(error),
            }
        }
        if self.pending.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

#[cfg(all(test, windows))]
pub(crate) fn write_test_pdf(path: &Path) {
    let drawing = "0.2 0.5 0.8 rg 20 20 80 120 re f\n";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 160] /Contents 5 0 R >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 120 160] /Contents 5 0 R >>".to_owned(),
        format!(
            "<< /Length {} >>\nstream\n{drawing}endstream",
            drawing.len()
        ),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        bytes.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    std::fs::write(path, bytes).unwrap();
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn pdf_pages_paint_textures_at_all_zooms_with_bounded_cache_and_no_source_changes() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("read-only.pdf");
        write_test_pdf(&path);
        let original = std::fs::read(&path).unwrap();
        let mut view = PdfView::open(&path).unwrap();
        let ctx = egui::Context::default();
        for zoom in [0.5, 1.25, 4.0] {
            for page in [0, 1] {
                view.selected = page;
                let start = std::time::Instant::now();
                loop {
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1200.0, 800.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| view.show(ui, zoom));
                        },
                    );
                    if let Some((_, texture)) = view.cache.iter().find(|(index, _)| *index == page)
                    {
                        let id = texture.id();
                        assert!(ctx.tessellate(output.shapes, output.pixels_per_point).iter().any(|primitive| matches!(&primitive.primitive, egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == id && !mesh.vertices.is_empty())));
                        break;
                    }
                    assert!(view.error.is_none(), "{:?}", view.error);
                    assert!(start.elapsed() < std::time::Duration::from_secs(5));
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                assert!(view.cache.len() <= 2);
            }
        }
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
}

fn decode_png(
    ctx: &egui::Context,
    index: usize,
    bytes: &[u8],
) -> Result<egui::TextureHandle, String> {
    let mut reader =
        image::ImageReader::with_format(std::io::Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2000);
    limits.max_image_height = Some(2000);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|error| {
            tfm!(
                "Не удалось декодировать растр PDF: {error}",
                "PDF raster decoding failed: {error}"
            )
        })?
        .into_rgba8();
    Ok(ctx.load_texture(
        format!("pdf-page-{index}"),
        egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        ),
        egui::TextureOptions::LINEAR,
    ))
}
