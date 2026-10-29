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
    /// Set by the explicit Paste range action; consumed by the shell.
    pub range_paste_requested: bool,
    pub selection: TableSelection,
    pub horizontal_offset: f32,
    pub decimal_places: Option<u8>,
    pub formula_input: String,
    pub formula_message: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TableSelection {
    pub row: Option<usize>,
    pub column: Option<usize>,
}

impl SpreadsheetDocument {
    pub fn from_sheets(sheets: Vec<SpreadsheetModel>) -> Self {
        Self {
            sheets,
            decimal_places: Some(2),
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

    /// Paste a rectangular text range at the active cell. All limits are
    /// checked before the first mutation, so an oversized range changes nothing.
    pub fn paste_grid(
        &mut self,
        text: &str,
    ) -> Result<(usize, usize), crate::range_paste::RangePasteError> {
        use crate::range_paste::{parse_grid, RangePasteError};
        if self.editor.is_none() {
            return Err(RangePasteError::ReadOnly);
        }
        if self.selected_sheet().is_none() {
            return Err(RangePasteError::OutOfBounds);
        }
        let grid = parse_grid(text)?;
        let (row, column) = self.selected_cell.unwrap_or((0, 0));
        let end_row = row
            .checked_add(grid.rows)
            .ok_or(RangePasteError::OutOfBounds)?;
        let end_column = column
            .checked_add(grid.columns)
            .ok_or(RangePasteError::OutOfBounds)?;
        if end_row > 1_048_576 || end_column > 16_384 {
            return Err(RangePasteError::OutOfBounds);
        }
        for (row_offset, values) in grid.cells.iter().enumerate() {
            for (column_offset, value) in values.iter().enumerate() {
                self.set_cell_text(row + row_offset, column + column_offset, value.clone());
            }
        }
        Ok((grid.rows, grid.columns))
    }

    pub fn take_range_paste_request(&mut self) -> bool {
        std::mem::take(&mut self.range_paste_requested)
    }

    pub fn set_formula_input(&mut self) {
        let (row, column) = self.selected_cell.unwrap_or((0, 0));
        if let Some(sheet) = self.selected_sheet() {
            self.formula_input = sheet.cell_value(row, column).to_owned();
        }
    }

    pub fn display_value(&self, row: usize, column: usize) -> String {
        let Some(sheet) = self.selected_sheet() else {
            return String::new();
        };
        format_display_value(sheet.cell_value(row, column), self.decimal_places)
    }

    pub fn apply_formula(&mut self) -> bool {
        let Some((row, column)) = self.selected_cell else {
            self.formula_message = Some(tr!("Выберите ячейку", "Select a cell").into());
            return false;
        };
        let Some(sheet) = self.selected_sheet() else {
            return false;
        };
        let expression = self.formula_input.clone();
        let result = crate::table_formula::evaluate(&expression, |reference| {
            crate::table_formula::parse_cell_reference(reference)
                .filter(|(row, column)| *row < sheet.row_count && *column < sheet.column_count)
                .map(|(row, column)| sheet.cell_value(row, column).to_owned())
        });
        match result {
            Ok(value) => {
                self.set_cell_text(
                    row,
                    column,
                    crate::table_formula::format_number(value, self.decimal_places),
                );
                self.formula_message = None;
                true
            }
            Err(error) => {
                use crate::table_formula::FormulaError;
                let reason = match error {
                    FormulaError::Empty => tfm!("пустое выражение", "empty expression"),
                    FormulaError::TooLong => {
                        tfm!("слишком длинное выражение", "expression is too long")
                    }
                    FormulaError::BadReference => {
                        tfm!("неверная ссылка на ячейку", "invalid cell reference")
                    }
                    FormulaError::BadNumber => tfm!(
                        "неверное число (используйте точку или запятую)",
                        "invalid number (use a dot or comma)"
                    ),
                    FormulaError::Unexpected => tfm!(
                        "неожиданный символ или оператор",
                        "unexpected character or operator"
                    ),
                    FormulaError::DivideByZero => tfm!("деление на ноль", "division by zero"),
                    FormulaError::NotFinite => {
                        tfm!("результат не является числом", "result is not finite")
                    }
                    FormulaError::NonNumericCell(reference) => tfm!(
                        "ячейка {} не содержит число",
                        "cell {} is not numeric",
                        reference
                    ),
                };
                self.formula_message = Some(tfm!(
                    "Формула не вычислена: {reason}",
                    "Formula not evaluated: {reason}"
                ));
                false
            }
        }
    }

    fn looks_like_formula(text: &str) -> bool {
        let text = text.trim();
        if text.starts_with('=') {
            return true;
        }
        let date_parts: Vec<&str> = text.split(['-', '.', '/']).collect();
        if date_parts.len() == 3
            && date_parts[0].len() == 4
            && date_parts
                .iter()
                .all(|part| part.chars().all(|c| c.is_ascii_digit()))
        {
            return false;
        }
        text.contains('+') || text.contains('*') || text.contains('/') || text.contains('-')
    }

    fn evaluate_cell_input(&self, row: usize, column: usize, text: &str) -> Option<String> {
        if !Self::looks_like_formula(text) {
            return None;
        }
        let sheet = self.selected_sheet()?;
        let result = crate::table_formula::evaluate(text, |reference| {
            crate::table_formula::parse_cell_reference(reference)
                .filter(|(row, column)| *row < sheet.row_count && *column < sheet.column_count)
                .map(|(row, column)| sheet.cell_value(row, column).to_owned())
        });
        let _ = (row, column);
        result
            .ok()
            .map(|value| crate::table_formula::format_number(value, self.decimal_places))
    }

    pub fn fill_down(&mut self, source_row: usize, column: usize, target_row: usize) -> bool {
        if target_row <= source_row || self.editor.is_none() {
            return false;
        }
        let Some(sheet) = self.sheets.get_mut(self.selected_sheet) else {
            return false;
        };
        let source = sheet.cell_value(source_row, column).to_owned();
        let previous = source_row
            .checked_sub(1)
            .map(|row| sheet.cell_value(row, column).to_owned());
        let step = previous
            .as_deref()
            .and_then(|value| value.trim().parse::<f64>().ok())
            .zip(source.trim().parse::<f64>().ok())
            .map(|(previous, current)| current - previous);
        for row in source_row + 1..=target_row {
            let value = if let Some(step) = step {
                let current = source.trim().parse::<f64>().unwrap_or(0.0);
                format!("{}", current + step * (row - source_row) as f64)
            } else {
                source.clone()
            };
            self.set_cell_text(row, column, value);
        }
        true
    }

    pub fn delete_selected_row_or_column(&mut self) -> bool {
        let row = self.selection.row;
        let column = self.selection.column;
        if row.is_none() && column.is_none() {
            return false;
        }
        let Some(sheet) = self.sheets.get_mut(self.selected_sheet) else {
            return false;
        };
        let before = sheet.cells.len();
        sheet
            .cells
            .retain(|cell| !(row == Some(cell.row) || column == Some(cell.column)));
        sheet.cells.len() != before
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
            ui.label(tr!("Формула", "Formula"));
            let formula_id = egui::Id::new("workbook-formula-input");
            let formula_enter_before = ui.memory(|memory| memory.has_focus(formula_id))
                && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            let formula = ui.add(
                egui::TextEdit::singleline(&mut self.formula_input)
                    .id_source(formula_id)
                    .desired_width(150.0)
                    .hint_text("=A1+B1"),
            );
            let formula = if let Some(message) = self.formula_message.clone() {
                formula.on_hover_text(message)
            } else {
                formula
            };
            if formula.gained_focus() {
                formula.request_focus();
            }
            let formula_enter_after = formula.lost_focus()
                && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if formula_enter_before || formula_enter_after {
                changed |= self.apply_formula();
            }
            if ui
                .button("=")
                .on_hover_text(tr!(
                    "Вычислить +, −, ×, ÷ и ссылки на ячейки; обычный текст с «=» не меняется. Enter в поле формулы вычисляет.",
                    "Evaluate +, −, ×, ÷ and cell references; ordinary '=' text is unchanged"
                ))
                .clicked()
            {
                changed |= self.apply_formula();
            }
            ui.label(tr!("Знаков после запятой", "Decimal places"));
            egui::ComboBox::from_id_source("workbook-decimals")
                .selected_text(match self.decimal_places {
                    Some(value) => value.to_string(),
                    None => tr!("Авто", "Auto").to_owned(),
                })
                .width(72.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.decimal_places, None, tr!("Авто", "Auto"));
                    for value in 0..=6_u8 {
                        ui.selectable_value(
                            &mut self.decimal_places,
                            Some(value),
                            value.to_string(),
                        );
                    }
                });
            if ui
                .add(egui::Button::new("◀"))
                .on_hover_text(tr!("Сдвинуть таблицу влево", "Move table left"))
                .clicked()
            {
                self.horizontal_offset = (self.horizontal_offset - 160.0).max(0.0);
            }
            if ui
                .add(egui::Button::new("▶"))
                .on_hover_text(tr!("Сдвинуть таблицу вправо", "Move table right"))
                .clicked()
            {
                self.horizontal_offset += 160.0;
            }
            if ui
                .button(tr!("Вставить диапазон", "Paste range"))
                .on_hover_text(tr!(
                    "Вставить текст из буфера как значения, начиная с активной ячейки; формулы не выполняются. Обычная вставка внутри редактора ячейки не перехватывается.",
                    "Paste clipboard text as values starting at the active cell; formulas are not executed. Normal paste inside the cell editor is not intercepted."
                ))
                .clicked()
            {
                self.range_paste_requested = true;
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
        let mut edit = InlineEdit {
            generation,
            sheet: self.selected_sheet,
            focus: self.focus_pending,
            horizontal_offset: self.horizontal_offset,
            ..Default::default()
        };
        let delete_pressed = (self.selection.row.is_some() || self.selection.column.is_some())
            && ui.memory(|memory| memory.focused().is_none())
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Delete));
        let mut deleted_selection = false;
        if delete_pressed {
            deleted_selection = self.delete_selected_row_or_column();
        }
        let Some(sheet) = self.selected_sheet() else {
            return false;
        };
        let mut table_selection = self.selection;
        let selected = render_cells(
            ui,
            sheet,
            self.selected_cell,
            zoom,
            search,
            Some(&mut edit),
            Some(&mut table_selection),
        );
        self.selection = table_selection;
        self.horizontal_offset = edit.horizontal_offset;
        self.input_id = edit.input_id;
        self.input_rect = edit.input_rect;
        if edit.focus_used {
            self.focus_pending = false;
        }
        let mut changed = deleted_selection || !edit.changes.is_empty();
        let submit = edit.submit;
        for (row, column, text) in edit.changes {
            let value = if submit
                .as_ref()
                .is_some_and(|(submit_row, submit_column, _)| {
                    *submit_row == row && *submit_column == column
                }) {
                self.evaluate_cell_input(row, column, &text).unwrap_or(text)
            } else {
                text
            };
            self.set_cell_text(row, column, value);
        }
        if let Some((source_row, column, target_row)) = edit.fill {
            changed |= self.fill_down(source_row, column, target_row);
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
    horizontal_offset: f32,
    fill: Option<(usize, usize, usize)>,
    submit: Option<(usize, usize, String)>,
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

fn format_display_value(value: &str, decimals: Option<u8>) -> String {
    let Some(decimals) = decimals else {
        return value.to_owned();
    };
    let Some(number) = value.trim().parse::<f64>().ok().filter(|n| n.is_finite()) else {
        return value.to_owned();
    };
    crate::table_formula::format_number(number, Some(decimals))
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
    pub decimals: Option<u8>,
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
    render_cells(ui, sheet, active_cell, zoom, search, None, None)
}

fn render_cells(
    ui: &mut egui::Ui,
    sheet: &SpreadsheetModel,
    active_cell: Option<(usize, usize)>,
    zoom: f32,
    search: &SearchView<'_>,
    mut edit: Option<&mut InlineEdit>,
    mut selection: Option<&mut TableSelection>,
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
                ui.label(egui::RichText::new(format_display_value(sheet.cell_value(row, column), search.decimals).chars().take(2048).collect::<String>()).color(egui::Color32::BLACK));
            }
            ui.add_space(8.0 * zoom);
            ui.horizontal(|ui| {
                ui.add_sized([32.0 * zoom, 24.0 * zoom], egui::Label::new(""));
                for (column, width) in columns.clone().zip(&column_widths) {
                    let response = ui.add_sized(
                        [*width, 24.0 * zoom],
                        egui::Label::new(
                            egui::RichText::new(column_name(column))
                                .strong()
                                .size(body_size)
                                .color(egui::Color32::BLACK),
                        )
                        .sense(egui::Sense::click()),
                    )
                    .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                    .on_hover_text(tr!("Выделить столбец", "Select column"));
                    if response.clicked() {
                        if let Some(focused) = ui.memory(|memory| memory.focused()) {
                            ui.memory_mut(|memory| memory.surrender_focus(focused));
                        }
                        if let Some(selection) = selection.as_deref_mut() {
                            selection.column = Some(column);
                            selection.row = None;
                        }
                    }
                }
            });
            let scroll_offset = edit
                .as_ref()
                .map(|edit| edit.horizontal_offset)
                .unwrap_or(0.0);
            let scroll_output = egui::ScrollArea::both()
                .id_source(("spreadsheet-cells", &sheet.sheet_name, rows.start, columns.start))
                .horizontal_scroll_offset(scroll_offset)
                .show(ui, |ui| {
                    egui::Grid::new("spreadsheet-grid")
                        .spacing([2.0, 2.0])
                        .show(ui, |ui| {
                            for row in rows.clone() {
                                let selected_row = selection.as_ref().is_some_and(|selection| selection.row == Some(row));
                                let response = ui.add_sized(
                                    [32.0 * zoom, 22.0 * zoom],
                                    egui::Label::new(
                                        egui::RichText::new((row + 1).to_string())
                                            .size(small_size)
                                            .color(if selected_row { egui::Color32::from_rgb(0x6B, 0x42, 0x1C) } else { egui::Color32::DARK_GRAY }),
                                    )
                                    .sense(egui::Sense::click()),
                                )
                                .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                                .on_hover_text(tr!("Выделить строку", "Select row"));
                                if response.clicked() {
                                    if let Some(focused) = ui.memory(|memory| memory.focused()) {
                                        ui.memory_mut(|memory| memory.surrender_focus(focused));
                                    }
                                    if let Some(selection) = selection.as_deref_mut() {
                                        selection.row = Some(row);
                                        selection.column = None;
                                    }
                                }
                                for (column, width) in columns.clone().zip(&column_widths) {
                                    let format = search.formats.and_then(|(formats,sheet)| formats.get(&(sheet,row,column))).copied().unwrap_or_default();
                                    let mut value = egui::RichText::new(format_display_value(sheet.cell_value(row,column), search.decimals)).size(format.size as f32 * zoom).color(egui::Color32::BLACK);
                                    if format.bold { value = value.strong(); }
                                    let is_active = active_cell == Some((row, column));
                                    let is_target = search.target == Some((row, column));
                                    let is_match = !crate::search::ranges(sheet.cell_value(row, column), search.query, 1).is_empty();
                                    let selected_by_table = selection.as_ref().is_some_and(|selection| {
                                        selection.row == Some(row) || selection.column == Some(column)
                                    });
                                    let response = egui::Frame::none()
                                        .fill(if is_target {
                                            egui::Color32::from_rgb(255, 214, 120)
                                        } else if is_match {
                                            egui::Color32::from_rgb(255, 244, 185)
                                        } else if is_active {
                                            egui::Color32::from_rgb(211, 234, 255)
                                        } else if selected_by_table {
                                            egui::Color32::from_rgb(243, 230, 205)
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
                                                    let mut text=sheet.cell_value(row,column).to_owned();
                                                    if ui.memory(|m|m.has_focus(id)) {
                                                        let reverse = ui.input(|i|i.modifiers.shift);
                                                        let modifiers = if reverse { egui::Modifiers::SHIFT } else { egui::Modifiers::NONE };
                                                        if ui.input_mut(|i|i.consume_key(modifiers,egui::Key::Tab)) {
                                                            edit.navigate = Some((row,if reverse {column.saturating_sub(1)} else {(column+1).min(16383)}));
                                                        } else if ui.input_mut(|i|i.consume_key(modifiers,egui::Key::Enter)) {
                                                            edit.navigate = Some((if reverse {row.saturating_sub(1)} else {(row+1).min(1_048_575)},column));
                                                            edit.submit = Some((row,column,text.clone()));
                                                        }
                                                    }
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
                                    if is_active && search.editable {
                                        if let Some(edit) = edit.as_deref_mut() {
                                            let handle_rect = egui::Rect::from_center_size(
                                                response.rect.right_bottom() + egui::vec2(-3.0, -3.0),
                                                egui::vec2(9.0, 9.0),
                                            );
                                            let handle = ui
                                                .interact(
                                                    handle_rect,
                                                    egui::Id::new(("workbook-fill", edit.generation, row, column)),
                                                    egui::Sense::drag(),
                                                )
                                                .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                                                .on_hover_text(tr!(
                                                    "Потяните вниз, чтобы заполнить ячейки",
                                                    "Drag down to fill cells"
                                                ));
                                            ui.painter().rect_filled(handle_rect, 1.0, egui::Color32::from_rgb(55, 125, 190));
                                            if handle.dragged() {
                                                if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
                                                    let row_height = 22.0 * zoom;
                                                    let delta = ((pointer.y - response.rect.bottom()) / row_height).ceil() as isize;
                                                    let target = row.saturating_add_signed(delta);
                                                    if target > row {
                                                        edit.fill = Some((row, column, target));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if response.clicked() {
                                        selected = Some((row, column));
                                    }
                                    if is_target && search.scroll { response.scroll_to_me(Some(egui::Align::Center)); }
                                }
                                ui.end_row();
                            }
                        });
                });
            if let Some(edit) = edit.as_mut() {
                edit.horizontal_offset = scroll_output.state.offset.x;
                ui.add_space(4.0 * zoom);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(tr!("Положение таблицы", "Table position"))
                            .size(small_size)
                            .color(egui::Color32::DARK_GRAY),
                    );
                    ui.add(
                        egui::Slider::new(&mut edit.horizontal_offset, 0.0..=5_000.0)
                            .show_value(false),
                    )
                    .on_hover_text(tr!(
                        "Потяните ползунок, чтобы двигать таблицу влево/вправо",
                        "Drag to move the table left/right"
                    ));
                });
            }
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
    #[test]
    fn decimal_display_is_view_only_and_non_numeric_values_are_unchanged() {
        assert_eq!(super::format_display_value("3.14159", Some(2)), "3.14");
        assert_eq!(super::format_display_value("3.1000", Some(2)), "3.1");
        assert_eq!(
            super::format_display_value("Кириллица", Some(2)),
            "Кириллица"
        );
        assert_eq!(super::format_display_value("=A1+B1", Some(2)), "=A1+B1");
        assert_eq!(super::format_display_value("3.14159", None), "3.14159");
    }
}
