//! Read-only spreadsheet view state and egui rendering.
use crate::xlsx::SpreadsheetModel;
use crate::{tfm, tr};
use eframe::egui;

#[derive(Default)]
pub struct SpreadsheetDocument {
    pub sheets: Vec<SpreadsheetModel>,
    pub selected_sheet: usize,
    pub selected_cell: Option<(usize, usize)>,
    /// Bounded preview window moved by Find, independent of mouse selection.
    pub preview_anchor: Option<(usize, usize)>,
    pub editor: Option<crate::workbook_edit::WorkbookEditor>,
    pub formats:
        std::collections::BTreeMap<crate::workbook_edit::CellKey, crate::workbook_edit::CellFormat>,
    pub input_id: Option<egui::Id>,
    pub focus_pending: bool,
    pub input_rect: Option<egui::Rect>,
}

impl SpreadsheetDocument {
    pub fn from_sheets(sheets: Vec<SpreadsheetModel>) -> Self {
        Self {
            sheets,
            ..Self::default()
        }
    }

    pub fn selected_sheet(&self) -> Option<&SpreadsheetModel> {
        self.sheets.get(self.selected_sheet)
    }

    pub fn sheet_names(&self) -> impl Iterator<Item = &str> {
        self.sheets.iter().map(|sheet| sheet.sheet_name.as_str())
    }

    pub fn is_dirty(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|e| e.is_dirty(&self.sheets))
    }

    /// Edits are explicit literal text, including a leading '='. No formulas run.
    pub fn set_cell_text(&mut self, row: usize, column: usize, value: String) {
        if self.editor.is_none() || row >= 1_048_576 || column >= 16_384 {
            return;
        }
        if let Some(sheet) = self.sheets.get_mut(self.selected_sheet) {
            if let Some(cell) = sheet
                .cells
                .iter_mut()
                .find(|c| c.row == row && c.column == column)
            {
                cell.value = value;
            } else {
                sheet
                    .cells
                    .push(crate::xlsx::SpreadsheetCell { row, column, value });
            }
            sheet.row_count = sheet.row_count.max(row + 1);
            sheet.column_count = sheet.column_count.max(column + 1);
        }
    }

    pub fn show_editor(&mut self, ui: &mut egui::Ui, _generation: u64) -> bool {
        if self.editor.is_none() {
            return false;
        }
        let (row, column) = self.selected_cell.unwrap_or((0, 0));
        self.selected_cell = Some((row, column));
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("{}{}", column_name(column), row + 1));
            let key = (self.selected_sheet, row, column);
            let mut format = self
                .editor
                .as_ref()
                .unwrap()
                .formats
                .get(&key)
                .copied()
                .unwrap_or_default();
            ui.label(tr!("Кегль, пт", "Font pt"));
            let size_changed = ui
                .add(egui::DragValue::new(&mut format.size).range(6..=72))
                .changed();
            let bold_changed = ui
                .checkbox(&mut format.bold, tr!("Жирный", "Bold"))
                .changed();
            if size_changed || bold_changed {
                let current = self
                    .selected_sheet()
                    .unwrap()
                    .cell_value(row, column)
                    .to_owned();
                self.set_cell_text(row, column, current);
                self.editor.as_mut().unwrap().formats.insert(key, format);
                self.formats.insert(key, format);
                changed = true;
            }
        });
        changed
    }

    pub fn show_grid(
        &mut self,
        ui: &mut egui::Ui,
        zoom: f32,
        search: &SearchView<'_>,
        generation: u64,
    ) -> bool {
        let Some(sheet) = self.selected_sheet() else {
            return false;
        };
        let mut edit = InlineEdit {
            generation,
            sheet: self.selected_sheet,
            focus: self.focus_pending,
            ..Default::default()
        };
        let selected = render_cells(ui, sheet, self.selected_cell, zoom, search, Some(&mut edit));
        self.input_id = edit.input_id;
        self.input_rect = edit.input_rect;
        if edit.focus_used {
            self.focus_pending = false;
        }
        let changed = !edit.changes.is_empty();
        for (row, column, text) in edit.changes {
            self.set_cell_text(row, column, text);
        }
        if let Some(cell) = edit.navigate.or(selected) {
            if self.selected_cell != Some(cell) {
                self.selected_cell = Some(cell);
                self.focus_pending = self.editor.is_some();
                if edit.navigate.is_some() {
                    self.preview_anchor = Some(cell);
                }
                ui.ctx().request_repaint();
            }
        }
        changed
    }
}

#[derive(Default)]
struct InlineEdit {
    generation: u64,
    sheet: usize,
    focus: bool,
    focus_used: bool,
    input_id: Option<egui::Id>,
    input_rect: Option<egui::Rect>,
    changes: Vec<(usize, usize, String)>,
    navigate: Option<(usize, usize)>,
}

pub fn column_name(mut column: usize) -> String {
    column += 1;
    let mut name = String::new();
    while column > 0 {
        let remainder = (column - 1) % 26;
        name.insert(0, (b'A' + remainder as u8) as char);
        column = (column - 1) / 26;
    }
    name
}

fn column_widths(
    sheet: &SpreadsheetModel,
    columns: std::ops::Range<usize>,
    rows: std::ops::Range<usize>,
    zoom: f32,
) -> Vec<f32> {
    columns
        .map(|column| {
            let content_width = sheet
                .cells
                .iter()
                .filter(|cell| cell.column == column && rows.contains(&cell.row))
                .map(|cell| cell.value.chars().count())
                .max()
                .unwrap_or(0);
            (content_width as f32 * 7.5 + 20.0).clamp(90.0, 260.0) * zoom
        })
        .collect()
}

pub fn render(
    ui: &mut egui::Ui,
    sheet: &SpreadsheetModel,
    active_cell: Option<(usize, usize)>,
    zoom: f32,
) -> Option<(usize, usize)> {
    render_search(ui, sheet, active_cell, zoom, &SearchView::default())
}

#[derive(Default)]
pub struct SearchView<'a> {
    pub query: &'a str,
    pub target: Option<(usize, usize)>,
    pub anchor: Option<(usize, usize)>,
    pub scroll: bool,
    pub formats: Option<(
        &'a std::collections::BTreeMap<
            crate::workbook_edit::CellKey,
            crate::workbook_edit::CellFormat,
        >,
        usize,
    )>,
    pub editable: bool,
}

fn preview_range(
    total: usize,
    minimum: usize,
    limit: usize,
    target: Option<usize>,
) -> std::ops::Range<usize> {
    let total = total.max(minimum);
    let count = total.min(limit);
    let start = target
        .filter(|target| *target >= limit)
        .map(|target| {
            target
                .min(total - 1)
                .saturating_sub(count / 2)
                .min(total - count)
        })
        .unwrap_or(0);
    start..start + count
}

pub fn render_search(
    ui: &mut egui::Ui,
    sheet: &SpreadsheetModel,
    active_cell: Option<(usize, usize)>,
    zoom: f32,
    search: &SearchView<'_>,
) -> Option<(usize, usize)> {
    render_cells(ui, sheet, active_cell, zoom, search, None)
}

fn render_cells(
    ui: &mut egui::Ui,
    sheet: &SpreadsheetModel,
    active_cell: Option<(usize, usize)>,
    zoom: f32,
    search: &SearchView<'_>,
    mut edit: Option<&mut InlineEdit>,
) -> Option<(usize, usize)> {
    const MAX_PREVIEW_COLUMNS: usize = 26;
    const MAX_PREVIEW_ROWS: usize = 200;
    let zoom = zoom.clamp(0.5, 4.0);
    let body_size = 14.0 * zoom;
    let small_size = 12.0 * zoom;
    let columns = preview_range(
        sheet
            .column_count
            .max(active_cell.map(|(_, c)| c + 1).unwrap_or(0)),
        8,
        MAX_PREVIEW_COLUMNS,
        search.anchor.map(|(_, column)| column),
    );
    let rows = preview_range(
        sheet
            .row_count
            .max(active_cell.map(|(r, _)| r + 1).unwrap_or(0)),
        16,
        MAX_PREVIEW_ROWS,
        search.anchor.map(|(row, _)| row),
    );
    let column_widths = column_widths(sheet, columns.clone(), rows.clone(), zoom);
    let mut selected = None;

    egui::Frame::none()
        .fill(egui::Color32::WHITE)
        .inner_margin(16.0 * zoom)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(&sheet.sheet_name)
                    .size(22.0 * zoom)
                    .color(egui::Color32::BLACK),
            );
            ui.label(
                egui::RichText::new(if search.editable { tr!("Щёлкните ячейку и вводите. Tab — следующий столбец; Enter — следующая строка; Shift меняет направление.", "Click a cell and type. Tab: next column; Enter: next row. Shift reverses direction.") } else { tr!("Просмотр таблицы — только чтение", "Spreadsheet preview — read-only") })
                    .size(small_size)
                    .color(egui::Color32::DARK_GRAY),
            );
            if let Some((row, column)) = active_cell {
                ui.label(
                    egui::RichText::new(tfm!("Активная ячейка: {}{}", "Active cell: {}{}", column_name(column), row + 1))
                        .size(small_size)
                        .color(egui::Color32::DARK_GRAY),
                );
                ui.label(egui::RichText::new(sheet.cell_value(row, column).chars().take(2048).collect::<String>()).color(egui::Color32::BLACK));
            }
            ui.add_space(8.0 * zoom);
            ui.horizontal(|ui| {
                ui.add_sized([32.0 * zoom, 24.0 * zoom], egui::Label::new(""));
                for (column, width) in columns.clone().zip(&column_widths) {
                    ui.add_sized(
                        [*width, 24.0 * zoom],
                        egui::Label::new(
                            egui::RichText::new(column_name(column))
                                .strong()
                                .size(body_size)
                                .color(egui::Color32::BLACK),
                        ),
                    );
                }
            });
            egui::ScrollArea::both()
                .id_source(("spreadsheet-cells", &sheet.sheet_name, rows.start, columns.start))
                .show(ui, |ui| {
                    egui::Grid::new("spreadsheet-grid")
                        .spacing([2.0, 2.0])
                        .show(ui, |ui| {
                            for row in rows.clone() {
                                ui.add_sized(
                                    [32.0 * zoom, 22.0 * zoom],
                                    egui::Label::new(
                                        egui::RichText::new((row + 1).to_string())
                                            .size(small_size)
                                            .color(egui::Color32::DARK_GRAY),
                                    ),
                                );
                                for (column, width) in columns.clone().zip(&column_widths) {
                                    let format = search.formats.and_then(|(formats,sheet)| formats.get(&(sheet,row,column))).copied().unwrap_or_default();
                                    let mut value = egui::RichText::new(sheet.cell_value(row,column)).size(format.size as f32 * zoom).color(egui::Color32::BLACK);
                                    if format.bold { value = value.strong(); }
                                    let is_active = active_cell == Some((row, column));
                                    let is_target = search.target == Some((row, column));
                                    let is_match = !crate::search::ranges(sheet.cell_value(row, column), search.query, 1).is_empty();
                                    let response = egui::Frame::none()
                                        .fill(if is_target {
                                            egui::Color32::from_rgb(255, 214, 120)
                                        } else if is_match {
                                            egui::Color32::from_rgb(255, 244, 185)
                                        } else if is_active {
                                            egui::Color32::from_rgb(211, 234, 255)
                                        } else {
                                            egui::Color32::from_gray(250)
                                        })
                                        .stroke(egui::Stroke::new(
                                            1.0_f32,
                                            if is_active {
                                                egui::Color32::from_rgb(55, 125, 190)
                                            } else {
                                                egui::Color32::from_gray(205)
                                            },
                                        ))
                                        .show(ui, |ui| {
                                            if is_active && search.editable {
                                                if let Some(edit) = edit.as_deref_mut() {
                                                    let id = egui::Id::new(("workbook-inline",edit.generation,edit.sheet,row,column));
                                                    if edit.focus && ui.is_enabled() {
                                                        ui.memory_mut(|m|m.request_focus(id));
                                                        edit.focus_used = true;
                                                    }
                                                    if ui.memory(|m|m.has_focus(id)) {
                                                        let reverse = ui.input(|i|i.modifiers.shift);
                                                        let modifiers = if reverse { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE };
                                                        if ui.input_mut(|i|i.consume_key(modifiers,egui::Key::Tab)) {
                                                            edit.navigate = Some((row,if reverse {column.saturating_sub(1)} else {(column+1).min(16383)}));
                                                        } else if ui.input_mut(|i|i.consume_key(modifiers,egui::Key::Enter)) {
                                                            edit.navigate = Some((if reverse {row.saturating_sub(1)} else {(row+1).min(1_048_575)},column));
                                                        }
                                                    }
                                                    let mut text=sheet.cell_value(row,column).to_owned();
                                                    ui.visuals_mut().extreme_bg_color = egui::Color32::WHITE;
                                                    ui.visuals_mut().text_cursor.stroke = egui::Stroke::new(2.0_f32,egui::Color32::BLACK);
                                                    ui.visuals_mut().selection.bg_fill = egui::Color32::from_rgb(180,210,250);
                                                    let response=ui.add_sized(
                                                        [*width-8.0*zoom,(format.size as f32*1.5).max(18.0)*zoom],
                                                        egui::TextEdit::singleline(&mut text).id(id)
                                                            .font(egui::FontId::proportional(format.size as f32*zoom))
                                                            .text_color(egui::Color32::BLACK).char_limit(32767)
                                                    );
                                                    edit.input_id=Some(id);
                                                    edit.input_rect=Some(response.rect);
                                                    if edit.focus {response.scroll_to_me(None);}
                                                    if response.changed() {edit.changes.push((row,column,text));}
                                                    return response;
                                                }
                                            }
                                            let response = ui.add_sized(
                                                [*width - 8.0 * zoom, (format.size as f32 * 1.5).max(18.0) * zoom],
                                                egui::Label::new(
                                                    value,
                                                )
                                                .truncate()
                                                .sense(egui::Sense::click()),
                                            );
                                            if let Some(edit) = edit.as_ref().filter(|_|search.editable) {
                                                let id = egui::Id::new(("workbook-inline",edit.generation,edit.sheet,row,column));
                                                ui.interact(response.rect,id,egui::Sense::click())
                                            } else {response}
                                        })
                                        .inner;
                                    if response.clicked() {
                                        selected = Some((row, column));
                                    }
                                    if is_target && search.scroll { response.scroll_to_me(Some(egui::Align::Center)); }
                                }
                                ui.end_row();
                            }
                        });
                });
            if sheet.row_count > MAX_PREVIEW_ROWS || sheet.column_count > MAX_PREVIEW_COLUMNS {
                ui.add_space(8.0 * zoom);
                ui.label(
                    egui::RichText::new(tfm!(
                        "Ограниченный просмотр: строки {}–{}, столбцы {}–{}. Поиск идёт по всем импортированным ячейкам всех листов.", "Bounded preview: rows {}–{}, columns {}–{}. Find searches all imported cells on all sheets.", rows.start + 1, rows.end, column_name(columns.start), column_name(columns.end - 1)
                    ))
                    .size(small_size)
                    .color(egui::Color32::DARK_GRAY),
                );
            }
        });
    selected
}

#[cfg(test)]
mod tests {
    use super::column_name;

    #[test]
    fn column_names_continue_after_z() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
    }

    #[test]
    fn far_find_target_is_visible_without_allocating_an_unbounded_grid() {
        for target in [0, 199, 200, 499, 9999] {
            let rows = super::preview_range(10000, 16, 200, Some(target));
            assert!(rows.contains(&target));
            assert!(rows.len() <= 200);
        }
        let columns = super::preview_range(30, 8, 26, Some(29));
        assert!(columns.contains(&29));
        assert_eq!(super::preview_range(0, 16, 200, None), 0..16);
    }
}
