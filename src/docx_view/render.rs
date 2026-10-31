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
                let measured = fragment
                    .text
                    .split('\n')
                    .map(|line| {
                        let galley = fonts.layout_job(egui::text::LayoutJob::simple_singleline(
                            line.to_owned(),
                            font.clone(),
                            egui::Color32::BLACK,
                        ));
                        galley.size().x.max(galley.mesh_bounds.max.x)
                    })
                    .fold(0.0_f32, f32::max)
                    + 18.0;
                let measured = measured.max(FRAGMENT_MIN_WIDTH).min(options.content_width);
                // Quantize the inline editor width so typing does not shift the
                // entire wrapped line on every glyph. The extra caret margin
                // also prevents the field from horizontally scrolling its text.
                ((measured / 16.0).ceil() * 16.0).min(options.content_width)
            });
            let line_count = fragment.text.split('\n').count().max(1);
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
            let text_output = egui::Frame::none()
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
                    ui.visuals_mut().selection.bg_fill = crate::ui_theme::selection();
                    let run_format = fragment.format;
                    let mut layouter = |ui: &egui::Ui, text: &str, _wrap_width: f32| {
                        let family = if run_format.bold {
                            crate::ui_theme::semibold_family()
                        } else {
                            font.family.clone()
                        };
                        let text_format = egui::TextFormat {
                            font_id: egui::FontId::new(font.size, family),
                            color: egui::Color32::BLACK,
                            italics: run_format.italic,
                            underline: if run_format.underline {
                                egui::Stroke::new(1.0_f32, egui::Color32::BLACK)
                            } else {
                                egui::Stroke::NONE
                            },
                            strikethrough: if run_format.strike {
                                egui::Stroke::new(1.0_f32, egui::Color32::BLACK)
                            } else {
                                egui::Stroke::NONE
                            },
                            ..Default::default()
                        };
                        let mut job = egui::text::LayoutJob::default();
                        job.append(text, 0.0, text_format);
                        ui.fonts(|fonts| fonts.layout_job(job))
                    };
                    let output = ui
                        .allocate_ui_with_layout(
                            egui::vec2(width, fragment_render_height(fragment, options.zoom)),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.set_width(width);
                                egui::TextEdit::multiline(&mut fragment.text)
                                    .id(id)
                                    .cursor_at_end(false)
                                    .desired_rows(line_count)
                                    .desired_width(width)
                                    .font(font.clone())
                                    .text_color(egui::Color32::BLACK)
                                    .layouter(&mut layouter)
                                    .frame(false)
                                    .show(ui)
                            },
                        )
                        .inner;
                    if output.response.double_clicked() {
                        if let Some(pointer) = output.response.interact_pointer_pos() {
                            let cursor = output.galley.cursor_from_pos(pointer - output.galley_pos);
                            let (start, end) =
                                liteoffice::text_document::TextDocument::unicode_word_range(
                                    &fragment.text,
                                    cursor.ccursor.index,
                                );
                            let mut state = output.state.clone();
                            state
                                .cursor
                                .set_char_range(Some(egui::text::CCursorRange::two(
                                    egui::text::CCursor::new(start),
                                    egui::text::CCursor::new(end),
                                )));
                            state.store(ui.ctx(), id);
                        }
                    }
                    output
                })
                .inner;
            let response = &text_output.response;
            if response.clicked() || response.gained_focus() {
                *options.selected_fragment_id = Some(fragment.id);
                if response.clicked() {
                    *options.select_all_docx = false;
                }
            }
            if response.changed() && fragment.text.ends_with('\n') {
                if let Some(prefix) = next_list_prefix(&fragment.text) {
                    fragment.text.push_str(&prefix);
                    if let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), id) {
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(
                                egui::text::CCursor::new(fragment.text.chars().count()),
                            )));
                        state.store(ui.ctx(), id);
                        ui.memory_mut(|memory| memory.request_focus(id));
                    }
                }
            }
            // A singleline TextEdit surrenders focus to Enter. In the DOCX
            // fragment editor Enter must not end the editing session: keep the
            // caret in the field so the next typed character still lands here.
            if response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
                ui.memory_mut(|memory| memory.request_focus(id));
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
fn next_list_prefix(text: &str) -> Option<String> {
    let body = text.strip_suffix('\n')?;
    let line = body.rsplit('\n').next().unwrap_or(body);
    let trimmed = line.trim_start();
    if trimmed == "• " || trimmed == "- " {
        return None;
    }
    if trimmed.starts_with("• ") || trimmed.starts_with("- ") {
        return Some("• ".to_owned());
    }
    let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && trimmed[digits..].starts_with(". ") {
        let current = trimmed[..digits].parse::<u32>().ok()?;
        let rest = &trimmed[digits + 2..];
        if rest.is_empty() {
            return None;
        }
        return Some(format!("{}. ", current.saturating_add(1)));
    }
    None
}

#[cfg(test)]
mod word_tests {
    use liteoffice::text_document::TextDocument;

    #[test]
    fn selects_cyrillic_and_latin_words_without_sentence_punctuation() {
        let text = "Привет, мир! Hello_world";
        assert_eq!(TextDocument::unicode_word_range(text, 2), (0, 6));
        assert_eq!(TextDocument::unicode_word_range(text, 8), (8, 11));
        assert_eq!(TextDocument::unicode_word_range(text, 13), (13, 24));
        assert_eq!(TextDocument::unicode_word_range(text, 21), (13, 24));
    }
}
