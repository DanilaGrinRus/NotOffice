use super::layout::{
    estimated_table_row_height, fragment_point_size, fragment_render_height, table_column_widths,
};
use crate::{tr, FRAGMENT_MIN_WIDTH, PARAGRAPH_GAP};
use eframe::egui;
use liteoffice::docx::{DocumentModel, Paragraph, ParagraphAlignment, TableRow};
use liteoffice::search::{self, Hit as SearchHit, Target as SearchTarget};

pub(crate) fn render_docx_image(
    ui: &mut egui::Ui,
    texture: Option<&egui::TextureHandle>,
    size_pt: [f32; 2],
    zoom: f32,
) -> egui::Response {
    let size = egui::vec2(size_pt[0], size_pt[1]) * zoom;
    if let Some(texture) = texture {
        ui.image((texture.id(), size))
    } else {
        ui.add_sized(
            size,
            egui::Label::new(tr!(
                "Не удалось декодировать изображение для просмотра",
                "Image could not be decoded for preview"
            )),
        )
    }
}

pub(crate) struct ParagraphRenderOptions<'a> {
    pub(crate) content_width: f32,
    pub(crate) zoom: f32,
    pub(crate) query: &'a str,
    pub(crate) current_match: Option<SearchHit>,
    pub(crate) scroll_to_match: bool,
    pub(crate) selected_fragment_id: &'a mut Option<usize>,
    pub(crate) select_all_docx: &'a mut bool,
    pub(crate) editor_generation: u64,
    pub(crate) focus_pending: &'a mut bool,
    pub(crate) reveal_focused: bool,
}

pub(crate) fn docx_editor_id(generation: u64, fragment_id: usize) -> egui::Id {
    egui::Id::new(("liteoffice-docx-text", generation, fragment_id))
}

pub(crate) fn render_paragraph(
    ui: &mut egui::Ui,
    paragraph: &mut Paragraph,
    paragraph_index: usize,
    options: &mut ParagraphRenderOptions<'_>,
) {
    let text = if options.query.trim().is_empty() {
        String::new()
    } else {
        paragraph
            .fragments
            .iter()
            .map(|fragment| fragment.text.as_str())
            .collect::<String>()
    };
    let matches = search::ranges(&text, options.query, search::MAX_HITS);
    let render_fragments = |ui: &mut egui::Ui| {
        let mut offset = 0;
        for fragment in paragraph.fragments.iter_mut() {
            let range = offset..offset + fragment.text.len();
            offset = range.end;
            let font = egui::FontId::proportional(
                fragment_point_size(fragment) * (96.0 / 72.0) * options.zoom,
            );
            // Actual glyph advances/ink plus TextEdit's side margins and a
            // rounding/caret allowance, not the pagination width estimate.
            let width = ui.fonts(|fonts| {
                let galley = fonts.layout_job(egui::text::LayoutJob::simple_singleline(
                    fragment.text.clone(),
                    font.clone(),
                    egui::Color32::BLACK,
                ));
                (galley.size().x.max(galley.mesh_bounds.max.x) + 10.0)
                    .max(FRAGMENT_MIN_WIDTH)
                    .min(options.content_width)
            });
            let id = docx_editor_id(options.editor_generation, fragment.id);
            let focus_now =
                *options.focus_pending && ui.is_enabled() && ui.input(|input| input.focused);
            if focus_now {
                // Focus before processing Text events, not after drawing.
                ui.memory_mut(|memory| memory.request_focus(id));
                *options.focus_pending = false;
            }
            let matched = matches.iter().any(|hit| search::overlaps(&range, hit));
            let active = options.current_match.as_ref().is_some_and(|hit| {
                hit.target == SearchTarget::DocxParagraph(paragraph_index)
                    && search::overlaps(&range, &hit.bytes)
            });
            let response = egui::Frame::none()
                .fill(if *options.select_all_docx {
                    egui::Color32::from_rgb(255, 226, 170)
                } else if active {
                    egui::Color32::from_rgb(255, 214, 120)
                } else if matched {
                    egui::Color32::from_rgb(255, 244, 185)
                } else {
                    egui::Color32::TRANSPARENT
                })
                .show(ui, |ui| {
                    ui.add_sized(
                        [width, fragment_render_height(fragment, options.zoom)],
                        egui::TextEdit::singleline(&mut fragment.text)
                            .id(id)
                            .cursor_at_end(false)
                            .font(font)
                            .text_color(egui::Color32::BLACK)
                            .frame(false),
                    )
                })
                .inner;
            if response.clicked() || response.gained_focus() {
                *options.selected_fragment_id = Some(fragment.id);
                if response.clicked() {
                    *options.select_all_docx = false;
                }
            }
            if active && options.scroll_to_match {
                ui.scroll_to_rect(response.rect, Some(egui::Align::Center));
            }
            if focus_now || (options.reveal_focused && response.has_focus()) {
                // Minimal scrolling, only when needed: at high zoom the top
                // margin may otherwise place the first caret below a short
                // viewport. Do not force Align::Min in both axes.
                response.scroll_to_me(None);
            }
        }
    };
    match paragraph.alignment {
        ParagraphAlignment::Center => ui.horizontal_centered(render_fragments),
        ParagraphAlignment::Left => ui.horizontal_wrapped(render_fragments),
    };
}

pub(crate) fn render_table_row(
    ui: &mut egui::Ui,
    model: &mut DocumentModel,
    row: &TableRow,
    grid_widths: &[u32],
    column_count: usize,
    options: &mut ParagraphRenderOptions<'_>,
) {
    let column_widths = table_column_widths(
        options.content_width,
        grid_widths,
        column_count,
        ui.spacing().item_spacing.x,
    );
    let row_height = estimated_table_row_height(row, model, &column_widths, options.zoom);
    let page_content_width = options.content_width;
    ui.horizontal(|ui| {
        for (cell_index, cell_width) in column_widths.iter().enumerate() {
            let cell = row.cells.get(cell_index).map(Vec::as_slice).unwrap_or(&[]);
            ui.allocate_ui_with_layout(
                egui::vec2(*cell_width, row_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::Frame::none()
                        .fill(egui::Color32::from_gray(250))
                        .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(190)))
                        .inner_margin(4.0 * options.zoom)
                        .show(ui, |ui| {
                            ui.set_min_width(*cell_width - 8.0 * options.zoom);
                            for (index, paragraph_index) in cell.iter().enumerate() {
                                options.content_width = *cell_width - 8.0 * options.zoom;
                                render_paragraph(
                                    ui,
                                    &mut model.paragraphs[*paragraph_index],
                                    *paragraph_index,
                                    options,
                                );
                                if index + 1 < cell.len() {
                                    ui.add_space(PARAGRAPH_GAP * options.zoom);
                                }
                            }
                        });
                },
            );
        }
    });
    options.content_width = page_content_width;
}
