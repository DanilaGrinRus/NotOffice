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
    pub vertical_offset: f32,
    pub decimal_places: Option<u8>,
    pub formula_input: String,
    pub formula_message: Option<String>,
    /// Set by a double-click on a read-only cell; the shell opens the edit dialog.
    pub cell_edit_request: Option<(usize, usize, String)>,
    pub freeze_top_row: bool,
    pub freeze_left_column: bool,
    pub table_clipboard: Option<TableClipboard>,
    pub formulas: std::collections::BTreeMap<(usize, usize, usize), String>,
    pub highlights: CellHighlights,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellAction {
    CopyRow,
    CopyColumn,
    Paste,
    InsertRow,
    InsertColumn,
    Clear,
    Color([u8; 3]),
    ClearColor,
}

pub type CellHighlights = std::collections::BTreeMap<(usize, usize, usize), [u8; 3]>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableClipboard {
    Row(Vec<String>),
    Column(Vec<String>),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TableSelection {
    pub row: Option<usize>,
    pub column: Option<usize>,
    pub range_start: Option<(usize, usize)>,
    pub range_end: Option<(usize, usize)>,
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
        self.formulas.remove(&(self.selected_sheet, row, column));
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

    pub fn take_cell_edit_request(&mut self) -> Option<(usize, usize, String)> {
        self.cell_edit_request.take()
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
                    crate::table_formula::format_number(value, None),
                );
                self.formulas.insert(
                    (self.selected_sheet, row, column),
                    expression.trim().to_owned(),
                );
                self.formula_input = expression.trim().to_owned();
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
            .map(|value| crate::table_formula::format_number(value, None))
    }

    pub fn fill_range(
        &mut self,
        top: usize,
        left: usize,
        bottom: usize,
        right: usize,
        target: usize,
    ) -> bool {
        if self.editor.is_none() || (target >= top && target <= bottom) {
            return false;
        }
        let sources = {
            let Some(sheet) = self.sheets.get(self.selected_sheet) else {
                return false;
            };
            (left..=right)
                .map(|column| {
                    (
                        column,
                        (top..=bottom)
                            .map(|row| sheet.cell_value(row, column).to_owned())
                            .collect::<Vec<_>>(),
                    )
                })
                .collect::<Vec<_>>()
        };
        for (column, source) in sources {
            if source.is_empty() {
                continue;
            }
            let numbers = source
                .iter()
                .map(|value| value.trim().parse::<f64>().ok())
                .collect::<Option<Vec<_>>>()
                .filter(|values| values.len() >= 2);
            let step = numbers
                .as_ref()
                .map(|values| values[values.len() - 1] - values[values.len() - 2]);
            if target > bottom {
                for row in bottom + 1..=target {
                    let value = if let Some(step) = step {
                        let last = numbers.as_ref().unwrap().last().copied().unwrap_or(0.0);
                        format!("{}", last + step * (row - bottom) as f64)
                    } else {
                        source[source.len() - 1].clone()
                    };
                    self.set_cell_text(row, column, value);
                }
            } else {
                for row in target..top {
                    let value = if let Some(step) = step {
                        let first = numbers.as_ref().unwrap()[0];
                        format!("{}", first - step * (top - row) as f64)
                    } else {
                        source[0].clone()
                    };
                    self.set_cell_text(row, column, value);
                }
            }
        }
        true
    }

    pub fn copy_row(&mut self, row: usize) -> Option<String> {
        let sheet = self.selected_sheet()?;
        let values = (0..sheet.column_count.min(16_384))
            .map(|column| sheet.cell_value(row, column).to_owned())
            .collect::<Vec<_>>();
        self.table_clipboard = Some(TableClipboard::Row(values.clone()));
        Some(values.join("\t"))
    }

    pub fn copy_column(&mut self, column: usize) -> Option<String> {
        let sheet = self.selected_sheet()?;
        let values = (0..sheet.row_count.min(10_000))
            .map(|row| sheet.cell_value(row, column).to_owned())
            .collect::<Vec<_>>();
        self.table_clipboard = Some(TableClipboard::Column(values.clone()));
        Some(values.join("\n"))
    }

    pub fn copy_selected_row_or_column(&mut self) -> Option<String> {
        let (row, column) = self
            .selection
            .row
            .map(|row| {
                (
                    row,
                    self.selected_cell.map(|(_, column)| column).unwrap_or(0),
                )
            })
            .or_else(|| {
                self.selection
                    .column
                    .map(|column| (self.selected_cell.map(|(row, _)| row).unwrap_or(0), column))
            })
            .or(self.selected_cell)?;
        let sheet = self.selected_sheet()?;
        let (clipboard, text) = if self.selection.row.is_some() {
            let values = (0..sheet.column_count.min(16_384))
                .map(|column| sheet.cell_value(row, column).to_owned())
                .collect::<Vec<_>>();
            (TableClipboard::Row(values.clone()), values.join("\t"))
        } else {
            let values = (0..sheet.row_count.min(10_000))
                .map(|row| sheet.cell_value(row, column).to_owned())
                .collect::<Vec<_>>();
            (TableClipboard::Column(values.clone()), values.join("\n"))
        };
        self.table_clipboard = Some(clipboard);
        Some(text)
    }

    pub fn paste_selected_row_or_column(&mut self) -> bool {
        if self.editor.is_none() {
            return false;
        }
        let Some(clipboard) = self.table_clipboard.clone() else {
            return false;
        };
        let (row, column) = self
            .selection
            .row
            .map(|row| {
                (
                    row,
                    self.selected_cell.map(|(_, column)| column).unwrap_or(0),
                )
            })
            .or_else(|| {
                self.selection
                    .column
                    .map(|column| (self.selected_cell.map(|(row, _)| row).unwrap_or(0), column))
            })
            .or(self.selected_cell)
            .unwrap_or((0, 0));
        match clipboard {
            TableClipboard::Row(values) => {
                for (offset, value) in values.into_iter().enumerate() {
                    if offset >= 16_384 {
                        break;
                    }
                    self.set_cell_text(row, offset, value);
                }
            }
            TableClipboard::Column(values) => {
                for (offset, value) in values.into_iter().enumerate() {
                    if offset >= 1_048_576 {
                        break;
                    }
                    self.set_cell_text(offset, column, value);
                }
            }
        }
        true
    }

    pub fn clear_active_cell(&mut self) -> bool {
        let Some((row, column)) = self.selected_cell else {
            return false;
        };
        if self.editor.is_none() {
            return false;
        }
        self.set_cell_text(row, column, String::new());
        true
    }

    pub fn clear_selected_range(&mut self) -> bool {
        if self.editor.is_none() {
            return false;
        }
        let Some(start) = self.selection.range_start else {
            return false;
        };
        let Some(end) = self.selection.range_end else {
            return false;
        };
        let (top, bottom) = (start.0.min(end.0), start.0.max(end.0));
        let (left, right) = (start.1.min(end.1), start.1.max(end.1));
        for row in top..=bottom {
            for column in left..=right {
                self.set_cell_text(row, column, String::new());
            }
        }
        true
    }

    pub fn insert_row_after_selected(&mut self) -> bool {
        if self.editor.is_none() {
            return false;
        }
        let row = self
            .selection
            .row
            .or_else(|| self.selected_cell.map(|(row, _)| row))
            .unwrap_or(0);
        let Some(sheet) = self.sheets.get_mut(self.selected_sheet) else {
            return false;
        };
        for cell in &mut sheet.cells {
            if cell.row >= row {
                cell.row += 1;
            }
        }
        sheet.row_count = sheet.row_count.saturating_add(1).min(1_048_576);
        let selected_sheet = self.selected_sheet;
        let mut moved = std::collections::BTreeMap::new();
        for (key, formula) in std::mem::take(&mut self.formulas) {
            let (sheet, formula_row, column) = key;
            let formula_row = if sheet == selected_sheet && formula_row >= row {
                formula_row + 1
            } else {
                formula_row
            };
            moved.insert((sheet, formula_row, column), formula);
        }
        self.formulas = moved;
        let mut moved_formats = std::collections::BTreeMap::new();
        for (key, format) in std::mem::take(&mut self.formats) {
            let (sheet, format_row, column) = key;
            let format_row = if sheet == selected_sheet && format_row >= row {
                format_row + 1
            } else {
                format_row
            };
            moved_formats.insert((sheet, format_row, column), format);
        }
        self.formats = moved_formats;
        if let Some(editor) = self.editor.as_mut() {
            let mut moved_editor_formats = std::collections::BTreeMap::new();
            for (key, format) in std::mem::take(&mut editor.formats) {
                let (sheet, format_row, column) = key;
                let format_row = if sheet == selected_sheet && format_row >= row {
                    format_row + 1
                } else {
                    format_row
                };
                moved_editor_formats.insert((sheet, format_row, column), format);
            }
            editor.formats = moved_editor_formats;
        }
        true
    }

    pub fn insert_column_after_selected(&mut self) -> bool {
        if self.editor.is_none() {
            return false;
        }
        let column = self
            .selection
            .column
            .or_else(|| self.selected_cell.map(|(_, column)| column))
            .unwrap_or(0);
        let Some(sheet) = self.sheets.get_mut(self.selected_sheet) else {
            return false;
        };
        for cell in &mut sheet.cells {
            if cell.column >= column {
                cell.column += 1;
            }
        }
        sheet.column_count = sheet.column_count.saturating_add(1).min(16_384);
        let selected_sheet = self.selected_sheet;
        let mut moved = std::collections::BTreeMap::new();
        for (key, formula) in std::mem::take(&mut self.formulas) {
            let (sheet, formula_row, formula_column) = key;
            let formula_column = if sheet == selected_sheet && formula_column >= column {
                formula_column + 1
            } else {
                formula_column
            };
            moved.insert((sheet, formula_row, formula_column), formula);
        }
        self.formulas = moved;
        let mut moved_formats = std::collections::BTreeMap::new();
        for (key, format) in std::mem::take(&mut self.formats) {
            let (sheet, format_row, format_column) = key;
            let format_column = if sheet == selected_sheet && format_column >= column {
                format_column + 1
            } else {
                format_column
            };
            moved_formats.insert((sheet, format_row, format_column), format);
        }
        self.formats = moved_formats;
        if let Some(editor) = self.editor.as_mut() {
            let mut moved_editor_formats = std::collections::BTreeMap::new();
            for (key, format) in std::mem::take(&mut editor.formats) {
                let (sheet, format_row, format_column) = key;
                let format_column = if sheet == selected_sheet && format_column >= column {
                    format_column + 1
                } else {
                    format_column
                };
                moved_editor_formats.insert((sheet, format_row, format_column), format);
            }
            editor.formats = moved_editor_formats;
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
            ui.allocate_ui_with_layout(
                egui::vec2(58.0, 24.0),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| {
                    ui.label(
                        egui::RichText::new(tr!("Формула", "Formula"))
                            .size(11.0)
                            .color(egui::Color32::from_gray(90)),
                    );
                },
            );
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
            ui.checkbox(
                &mut self.freeze_top_row,
                tr!("Закрепить верхнюю строку", "Freeze top row"),
            )
            .on_hover_text(tr!(
                "Первая строка остаётся видимой при вертикальной прокрутке",
                "Keep the first row visible while scrolling vertically"
            ));
            ui.checkbox(
                &mut self.freeze_left_column,
                tr!("Закрепить левый столбец", "Freeze left column"),
            )
            .on_hover_text(tr!(
                "Первый столбец остаётся видимым при горизонтальной прокрутке",
                "Keep the first column visible while scrolling horizontally"
            ));
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
                .button(tr!("Копировать строку", "Copy row"))
                .on_hover_text(tr!(
                    "Скопировать строку в буфер",
                    "Copy the row to the clipboard"
                ))
                .clicked()
            {
                if let Some(text) = self.copy_row(self.selection.row.unwrap_or(row)) {
                    ui.ctx().copy_text(text);
                }
            }
            if ui
                .button(tr!("Копировать столбец", "Copy column"))
                .on_hover_text(tr!("Скопировать столбец в буфер", "Copy the column to the clipboard"))
                .clicked()
            {
                if let Some(text) = self.copy_column(self.selection.column.unwrap_or(column)) {
                    ui.ctx().copy_text(text);
                }
            }
            if ui
                .add_enabled(
                    self.table_clipboard.is_some(),
                    egui::Button::new(tr!("Вставить строку/столбец", "Paste row/column")),
                )
                .on_hover_text(tr!(
                    "Вставить скопированную строку или столбец в текущую позицию",
                    "Paste the copied row or column at the current position"
                ))
                .clicked()
            {
                changed |= self.paste_selected_row_or_column();
            }
            if ui
                .button(tr!("Добавить строку", "Insert row"))
                .on_hover_text(tr!("Вставить пустую строку выше выбранной", "Insert an empty row above the selected one"))
                .clicked()
            {
                changed |= self.insert_row_after_selected();
            }
            if ui
                .button(tr!("Добавить столбец", "Insert column"))
                .on_hover_text(tr!("Вставить пустой столбец левее выбранного", "Insert an empty column before the selected one"))
                .clicked()
            {
                changed |= self.insert_column_after_selected();
            }
            if ui
                .button(tr!("Очистить ячейку", "Clear cell"))
                .on_hover_text(tr!("Очистить активную ячейку; Ctrl+Z отменяет", "Clear the active cell; Ctrl+Z undoes"))
                .clicked()
            {
                changed |= self.clear_active_cell();
            }
            egui::ComboBox::from_id_source("cell-highlight")
                .selected_text(tr!("Цвет ячейки", "Cell colour"))
                .show_ui(ui, |ui| {
                    let key = (self.selected_sheet, row, column);
                    for (label, color) in [
                        (tr!("Жёлтый", "Yellow"), [255, 235, 150]),
                        (tr!("Зелёный", "Green"), [190, 235, 190]),
                        (tr!("Голубой", "Blue"), [190, 225, 245]),
                        (tr!("Розовый", "Pink"), [248, 200, 210]),
                    ] {
                        if ui.button(label).clicked() {
                            self.highlights.insert(key, color);
                            changed = true;
                        }
                    }
                    if ui.button(tr!("Без цвета", "No colour")).clicked() {
                        self.highlights.remove(&key);
                        changed = true;
                    }
                });
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
            vertical_offset: self.vertical_offset,
            ..Default::default()
        };
        let clear_active = edit.clear_cell_selection;
        if clear_active {
            self.selected_cell = None;
            self.selection.range_start = None;
            self.selection.range_end = None;
            self.focus_pending = false;
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
        if let Some(table_rect) = edit.table_rect {
            let clicked_above = ui.input(|input| input.pointer.any_click())
                && ui
                    .input(|input| input.pointer.interact_pos())
                    .is_some_and(|pos| pos.y < table_rect.top() && ui.max_rect().contains(pos));
            if clicked_above {
                self.selected_cell = None;
                self.selection.range_start = None;
                self.selection.range_end = None;
                self.focus_pending = false;
            }
        }
        self.horizontal_offset = edit.horizontal_offset;
        self.vertical_offset = edit.vertical_offset;
        self.input_id = edit.input_id;
        self.input_rect = edit.input_rect;
        if edit.focus_used {
            self.focus_pending = false;
        }
        let mut changed = !edit.changes.is_empty();
        let submit = edit.submit;
        let mut submitted_applied = false;
        for (row, column, text) in edit.changes {
            let is_submit = submit
                .as_ref()
                .is_some_and(|(submit_row, submit_column, _)| {
                    *submit_row == row && *submit_column == column
                });
            let value = if is_submit {
                submitted_applied = true;
                let formula = Self::looks_like_formula(&text);
                let source = text.clone();
                let value = self.evaluate_cell_input(row, column, &text).unwrap_or(text);
                if formula {
                    self.formulas
                        .insert((self.selected_sheet, row, column), source);
                }
                value
            } else {
                text
            };
            self.set_cell_text(row, column, value);
        }
        if let Some((row, column, text)) = submit {
            if !submitted_applied {
                if let Some(value) = self.evaluate_cell_input(row, column, &text) {
                    self.set_cell_text(row, column, value);
                    if Self::looks_like_formula(&text) {
                        self.formulas
                            .insert((self.selected_sheet, row, column), text.clone());
                    }
                    changed = true;
                }
            }
        }
        if let Some((top, left, bottom, right, target)) = edit.fill {
            changed |= self.fill_range(top, left, bottom, right, target);
        }
        if let Some(cell) = edit.navigate.or(selected) {
            if self.selected_cell != Some(cell) {
                self.selected_cell = Some(cell);
                self.focus_pending = self.editor.is_some();
                self.formula_input = self
                    .formulas
                    .get(&(self.selected_sheet, cell.0, cell.1))
                    .cloned()
                    .or_else(|| {
                        self.selected_sheet()
                            .map(|sheet| sheet.cell_value(cell.0, cell.1).to_owned())
                    })
                    .unwrap_or_default();
                if edit.navigate.is_some() {
                    self.preview_anchor = Some(cell);
                }
                ui.ctx().request_repaint();
            }
        }
        if let (Some((row, column)), Some(action)) = (edit.context_cell, edit.context_action) {
            self.selected_cell = Some((row, column));
            match action {
                CellAction::CopyRow => {
                    if let Some(text) = self.copy_row(row) {
                        ui.ctx().copy_text(text);
                    }
                }
                CellAction::CopyColumn => {
                    if let Some(text) = self.copy_column(column) {
                        ui.ctx().copy_text(text);
                    }
                }
                CellAction::Paste => {
                    changed |= self.paste_selected_row_or_column();
                }
                CellAction::InsertRow => {
                    self.selection.row = Some(row);
                    changed |= self.insert_row_after_selected();
                }
                CellAction::InsertColumn => {
                    self.selection.column = Some(column);
                    changed |= self.insert_column_after_selected();
                }
                CellAction::Clear => {
                    changed |= self.clear_active_cell();
                }
                CellAction::Color(color) => {
                    self.highlights
                        .insert((self.selected_sheet, row, column), color);
                    changed = true;
                }
                CellAction::ClearColor => {
                    self.highlights.remove(&(self.selected_sheet, row, column));
                    changed = true;
                }
            }
        }
        if let Some((row, column)) = edit.dialog {
            if let Some(sheet) = self.selected_sheet() {
                self.cell_edit_request =
                    Some((row, column, sheet.cell_value(row, column).to_owned()));
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
    vertical_offset: f32,
    fill: Option<(usize, usize, usize, usize, usize)>,
    submit: Option<(usize, usize, String)>,
    clear_cell_selection: bool,
    table_rect: Option<egui::Rect>,
    dialog: Option<(usize, usize)>,
    context_cell: Option<(usize, usize)>,
    context_action: Option<CellAction>,
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

#[allow(clippy::too_many_arguments)]
fn draw_frozen_panes(
    ui: &egui::Ui,
    sheet: &SpreadsheetModel,
    viewport: egui::Rect,
    offset: egui::Vec2,
    columns: &std::ops::Range<usize>,
    rows: &std::ops::Range<usize>,
    column_widths: &[f32],
    zoom: f32,
    search: &SearchView<'_>,
) {
    if !search.freeze_top_row && !search.freeze_left_column {
        return;
    }
    let painter = ui.painter().with_clip_rect(viewport);
    let row_header = 32.0 * zoom;
    let header_height = 26.0 * zoom;
    let row_height = 22.0 * zoom;
    let spacing = 2.0 * zoom;
    let text = egui::Color32::from_rgb(37, 35, 31);
    let muted = egui::Color32::from_rgb(105, 99, 91);
    let border = egui::Color32::from_gray(205);
    let background = egui::Color32::from_rgb(245, 242, 235);
    let font = egui::FontId::proportional(12.0 * zoom);
    let small = egui::FontId::proportional(10.5 * zoom);

    if search.freeze_left_column && offset.x > 0.5 {
        let first_width = column_widths.first().copied().unwrap_or(90.0 * zoom);
        let left_rect = egui::Rect::from_min_max(
            viewport.left_top(),
            egui::pos2(
                (viewport.left() + row_header + first_width + spacing).min(viewport.right()),
                viewport.bottom(),
            ),
        );
        painter.rect_filled(left_rect, 0.0, background);
        painter.line_segment(
            [left_rect.right_top(), left_rect.right_bottom()],
            egui::Stroke::new(1.0_f32, border),
        );
        for (index, row) in rows.clone().enumerate() {
            let y = viewport.top() - offset.y
                + header_height
                + row_height * index as f32
                + row_height / 2.0;
            if y < viewport.top() - row_height || y > viewport.bottom() + row_height {
                continue;
            }
            painter.text(
                egui::pos2(viewport.left() + row_header / 2.0, y),
                egui::Align2::CENTER_CENTER,
                (row + 1).to_string(),
                small.clone(),
                muted,
            );
            painter.text(
                egui::pos2(viewport.left() + row_header + first_width / 2.0, y),
                egui::Align2::CENTER_CENTER,
                format_display_value(sheet.cell_value(row, columns.start), search.decimals)
                    .chars()
                    .take(20)
                    .collect::<String>(),
                font.clone(),
                text,
            );
        }
    }

    if search.freeze_top_row && offset.y > 0.5 {
        let top_rect = egui::Rect::from_min_max(
            viewport.left_top(),
            egui::pos2(
                viewport.right(),
                (viewport.top() + header_height + row_height + spacing).min(viewport.bottom()),
            ),
        );
        painter.rect_filled(top_rect, 0.0, background);
        painter.line_segment(
            [top_rect.left_bottom(), top_rect.right_bottom()],
            egui::Stroke::new(1.0_f32, border),
        );
        let mut x = viewport.left() - offset.x + row_header + spacing;
        for (index, column) in columns.clone().enumerate() {
            let width = column_widths.get(index).copied().unwrap_or(90.0 * zoom);
            if x + width > viewport.left() && x < viewport.right() {
                painter.text(
                    egui::pos2(x + width / 2.0, viewport.top() + header_height / 2.0),
                    egui::Align2::CENTER_CENTER,
                    column_name(column),
                    font.clone(),
                    text,
                );
                painter.text(
                    egui::pos2(
                        x + width / 2.0,
                        viewport.top() + header_height + row_height / 2.0,
                    ),
                    egui::Align2::CENTER_CENTER,
                    format_display_value(sheet.cell_value(rows.start, column), search.decimals)
                        .chars()
                        .take(20)
                        .collect::<String>(),
                    font.clone(),
                    text,
                );
            }
            x += width + spacing;
        }
        // Keep the top-left corner readable when both panes are frozen.
        painter.rect_filled(
            egui::Rect::from_min_max(
                viewport.left_top(),
                egui::pos2(
                    viewport.left() + row_header,
                    viewport.top() + header_height + row_height,
                ),
            ),
            0.0,
            background,
        );
        painter.text(
            egui::pos2(
                viewport.left() + row_header / 2.0,
                viewport.top() + header_height + row_height / 2.0,
            ),
            egui::Align2::CENTER_CENTER,
            (rows.start + 1).to_string(),
            small,
            muted,
        );
    }
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
    pub freeze_top_row: bool,
    pub freeze_left_column: bool,
    pub highlights: Option<&'a CellHighlights>,
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
                egui::RichText::new(if search.editable { tr!("Щёлкните ячейку и вводите. Shift+клик — диапазон; синий маркер — заполнение; Shift+колесо — сдвиг.", "Click a cell and type. Shift+click: range; blue handle: fill; Shift+wheel: pan.") } else { tr!("Просмотр таблицы — только чтение. Shift+клик — диапазон; Shift+колесо — сдвиг.", "Spreadsheet preview — read-only. Shift+click: range; Shift+wheel: pan.") })
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
            ui.add_space(6.0 * zoom);
            if let Some(edit) = edit.as_mut() {
                let shift_wheel = ui.rect_contains_pointer(ui.max_rect())
                    && ui.input(|input| input.modifiers.shift);
                if shift_wheel {
                    let delta = ui.input_mut(|input| {
                        let delta = input.raw_scroll_delta;
                        input.raw_scroll_delta = egui::Vec2::ZERO;
                        input.smooth_scroll_delta = egui::Vec2::ZERO;
                        delta
                    });
                    if delta != egui::Vec2::ZERO {
                        edit.horizontal_offset =
                            (edit.horizontal_offset + delta.y + delta.x).clamp(0.0, 5_000.0);
                    }
                }
            }
            let scroll_offset = edit
                .as_ref()
                .map(|edit| edit.horizontal_offset)
                .unwrap_or(0.0);
            let scroll_output = egui::ScrollArea::both()
                .id_source(("spreadsheet-cells", &sheet.sheet_name, rows.start, columns.start))
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                .horizontal_scroll_offset(scroll_offset)
                .show(ui, |ui| {
                    egui::Grid::new("spreadsheet-grid")
                        .spacing([2.0, 2.0])
                        .show(ui, |ui| {
                            // Headers live inside the same Grid and ScrollArea as
                            // the cells, so letters and row numbers move together
                            // with the table and stay centered over their cells.
                            let corner = egui::Frame::none()
                                .fill(egui::Color32::from_rgb(245, 242, 235))
                                .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(205)))
                                .show(ui, |ui| {
                                    ui.set_min_size(egui::vec2(32.0 * zoom, 26.0 * zoom));
                                });
                            let _ = corner;
                            for (column, width) in columns.clone().zip(&column_widths) {
                                let header = egui::Frame::none()
                                    .fill(egui::Color32::from_rgb(245, 242, 235))
                                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(205)))
                                    .show(ui, |ui| {
                                        ui.set_min_size(egui::vec2(*width, 26.0 * zoom));
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(*width, 26.0 * zoom),
                                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                                            |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new(column_name(column))
                                                            .strong()
                                                            .size(body_size)
                                                            .color(egui::Color32::BLACK),
                                                    )
                                                    .sense(egui::Sense::click()),
                                                )
                                            },
                                        )
                                        .inner
                                    })
                                    .inner;
                                let header = header
                                    .on_hover_cursor(egui::CursorIcon::ResizeVertical)
                                    .on_hover_text(tr!("Выделить столбец", "Select column"));
                                if header.clicked() {
                                    if let Some(focused) = ui.memory(|memory| memory.focused()) {
                                        ui.memory_mut(|memory| memory.surrender_focus(focused));
                                    }
                                    if let Some(selection) = selection.as_deref_mut() {
                                        selection.column = Some(column);
                                        selection.row = None;
                                    }
                                }
                            }
                            ui.end_row();
                            for row in rows.clone() {
                                let selected_row = selection.as_ref().is_some_and(|selection| selection.row == Some(row));
                                let response = egui::Frame::none()
                                    .fill(egui::Color32::from_rgb(245, 242, 235))
                                    .stroke(egui::Stroke::new(1.0_f32, egui::Color32::from_gray(205)))
                                    .show(ui, |ui| {
                                        ui.set_min_size(egui::vec2(32.0 * zoom, 22.0 * zoom));
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(32.0 * zoom, 22.0 * zoom),
                                            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                                            |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        egui::RichText::new((row + 1).to_string())
                                                            .size(small_size)
                                                            .color(if selected_row { egui::Color32::from_rgb(0x6B, 0x42, 0x1C) } else { egui::Color32::DARK_GRAY }),
                                                    )
                                                    .sense(egui::Sense::click()),
                                                )
                                            },
                                        )
                                        .inner
                                    })
                                    .inner
                                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                                    .on_hover_text(tr!("Выделить строку", "Select row"));
                                if response.clicked() {
                                    if let Some(focused) = ui.memory(|memory| memory.focused()) {
                                        ui.memory_mut(|memory| memory.surrender_focus(focused));
                                    }
                                    if let Some(edit) = edit.as_deref_mut() {
                                        edit.clear_cell_selection = true;
                                    }
                                    if let Some(selection) = selection.as_deref_mut() {
                                        selection.row = Some(row);
                                        selection.column = None;
                                        selection.range_start = None;
                                        selection.range_end = None;
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
                                    let selected_by_range = selection.as_ref().is_some_and(|selection| {
                                        match (selection.range_start, selection.range_end) {
                                            (Some((r1, c1)), Some((r2, c2))) => {
                                                row >= r1.min(r2)
                                                    && row <= r1.max(r2)
                                                    && column >= c1.min(c2)
                                                    && column <= c1.max(c2)
                                            }
                                            _ => false,
                                        }
                                    });
                                    let fill_preview = edit.as_ref().and_then(|edit| edit.fill).is_some_and(
                                        |(top, left, bottom, right, target)| {
                                            column >= left
                                                && column <= right
                                                && if target > bottom {
                                                    row > bottom && row <= target
                                                } else {
                                                    row >= target && row < top
                                                }
                                        },
                                    );
                                    let response = egui::Frame::none()
                                        .fill(if is_target {
                                            egui::Color32::from_rgb(255, 214, 120)
                                        } else if is_match {
                                            egui::Color32::from_rgb(255, 244, 185)
                                        } else if let Some(color) = search
                                            .highlights
                                            .and_then(|colors| colors.get(&(search.formats.map(|(_, sheet)| sheet).unwrap_or(0), row, column)))
                                            .copied()
                                        {
                                            egui::Color32::from_rgb(color[0], color[1], color[2])
                                        } else if fill_preview {
                                            egui::Color32::from_rgb(220, 236, 250)
                                        } else if selected_by_range {
                                            egui::Color32::from_rgb(239, 226, 202)
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
                                            ui.set_min_width(*width);
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
                                    if is_active && search.editable && !selected_by_table {
                                        if let Some(edit) = edit.as_deref_mut() {
                                            let handle_rect = egui::Rect::from_center_size(
                                                response.rect.right_bottom() + egui::vec2(-3.0, -3.0),
                                                egui::vec2(14.0, 14.0),
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
                                                    let (top, left, bottom, right) = selection
                                                        .as_ref()
                                                        .and_then(|selection| {
                                                            Some((
                                                                selection.range_start?.0.min(selection.range_end?.0),
                                                                selection.range_start?.1.min(selection.range_end?.1),
                                                                selection.range_start?.0.max(selection.range_end?.0),
                                                                selection.range_start?.1.max(selection.range_end?.1),
                                                            ))
                                                        })
                                                        .unwrap_or((row, column, row, column));
                                                    let row_height = 22.0 * zoom;
                                                    let delta = ((pointer.y - response.rect.bottom()) / row_height).ceil() as isize;
                                                    let target = if delta >= 0 {
                                                        bottom.saturating_add(delta as usize)
                                                    } else {
                                                        top.saturating_sub((-delta) as usize)
                                                    };
                                                    if target < top || target > bottom {
                                                        edit.fill = Some((top, left, bottom, right, target));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if response.secondary_clicked() {
                                        selected = Some((row, column));
                                        if let Some(edit) = edit.as_deref_mut() {
                                            edit.context_cell = Some((row, column));
                                        }
                                    }
                                    let _ = response.context_menu(|ui| {
                                        if let Some(edit) = edit.as_deref_mut() {
                                            let set = |edit: &mut InlineEdit, action: CellAction| {
                                                edit.context_action = Some(action);
                                            };
                                            if ui.button(tr!("Копировать строку", "Copy row")).clicked() {
                                                set(edit, CellAction::CopyRow);
                                                ui.close_menu();
                                            }
                                            if ui.button(tr!("Копировать столбец", "Copy column")).clicked() {
                                                set(edit, CellAction::CopyColumn);
                                                ui.close_menu();
                                            }
                                            if ui.button(tr!("Вставить", "Paste")).clicked() {
                                                set(edit, CellAction::Paste);
                                                ui.close_menu();
                                            }
                                            if ui.button(tr!("Вставить строку выше", "Insert row above")).clicked() {
                                                set(edit, CellAction::InsertRow);
                                                ui.close_menu();
                                            }
                                            if ui.button(tr!("Вставить столбец левее", "Insert column left")).clicked() {
                                                set(edit, CellAction::InsertColumn);
                                                ui.close_menu();
                                            }
                                            if ui.button(tr!("Очистить", "Clear")).clicked() {
                                                set(edit, CellAction::Clear);
                                                ui.close_menu();
                                            }
                                            ui.menu_button(tr!("Цвет", "Colour"), |ui| {
                                                for (label, color) in [
                                                    (tr!("Жёлтый", "Yellow"), [255, 235, 150]),
                                                    (tr!("Зелёный", "Green"), [190, 235, 190]),
                                                    (tr!("Голубой", "Blue"), [190, 225, 245]),
                                                    (tr!("Розовый", "Pink"), [248, 200, 210]),
                                                ] {
                                                    if ui.button(label).clicked() {
                                                        set(edit, CellAction::Color(color));
                                                        ui.close_menu();
                                                    }
                                                }
                                                if ui.button(tr!("Без цвета", "No colour")).clicked() {
                                                    set(edit, CellAction::ClearColor);
                                                    ui.close_menu();
                                                }
                                            });
                                        }
                                    });
                                    if response.clicked() {
                                        selected = Some((row, column));
                                        if let Some(selection) = selection.as_deref_mut() {
                                            if ui.input(|input| input.modifiers.shift)
                                                && selection.range_start.is_some()
                                            {
                                                selection.range_end = Some((row, column));
                                            } else {
                                                selection.range_start = Some((row, column));
                                                selection.range_end = Some((row, column));
                                            }
                                            selection.row = None;
                                            selection.column = None;
                                        }
                                    }
                                    if response.double_clicked() && !search.editable {
                                        if let Some(edit) = edit.as_deref_mut() {
                                            edit.dialog = Some((row, column));
                                        }
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
                edit.vertical_offset = scroll_output.state.offset.y;
                edit.table_rect = Some(scroll_output.inner_rect);
            }
            draw_frozen_panes(
                ui,
                sheet,
                scroll_output.inner_rect,
                scroll_output.state.offset,
                &columns,
                &rows,
                &column_widths,
                zoom,
                search,
            );
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
