//! Bounded TSV/value paste for an explicitly editable spreadsheet copy.
//!
//! Only text values are transferred. A leading '=' is kept as literal text by
//! the existing workbook value policy; no formula is parsed or executed.
use std::fmt;

pub const MAX_PASTE_TEXT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_PASTE_ROWS: usize = 10_000;
pub const MAX_PASTE_COLUMNS: usize = 1_000;
pub const MAX_PASTE_CELLS: usize = 100_000;
pub const MAX_CELL_CHARS: usize = 32_767;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGrid {
    pub rows: usize,
    pub columns: usize,
    pub cells: Vec<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RangePasteError {
    Empty,
    TooLarge,
    TooManyCells,
    ReadOnly,
    OutOfBounds,
    InvalidText,
}

impl fmt::Display for RangePasteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "clipboard contains no cells"),
            Self::TooLarge => write!(f, "clipboard exceeds the paste size limit"),
            Self::TooManyCells => write!(f, "clipboard range exceeds the cell limit"),
            Self::ReadOnly => write!(f, "the source workbook is read-only"),
            Self::OutOfBounds => write!(f, "the range does not fit the worksheet"),
            Self::InvalidText => write!(f, "clipboard contains unsupported control characters"),
        }
    }
}

/// Parse a tab-separated rectangular clipboard text. Quoted TSV fields are
/// accepted (Excel quoting); tabs/newlines inside quotes remain cell text.
/// CRLF and LF both delimit rows. Trailing tabs/newlines do not enlarge the
/// range; internal empty cells are preserved.
pub fn parse_grid(text: &str) -> Result<ParsedGrid, RangePasteError> {
    if text.len() > MAX_PASTE_TEXT_BYTES {
        return Err(RangePasteError::TooLarge);
    }
    if text.is_empty() {
        return Err(RangePasteError::Empty);
    }

    let chars: Vec<char> = text.chars().collect();
    let mut rows: Vec<Vec<String>> = vec![Vec::new()];
    let mut field = String::new();
    let mut in_quotes = false;
    let mut index = 0usize;

    let finish_field = |rows: &mut Vec<Vec<String>>,
                        field: &mut String,
                        in_quotes: &mut bool|
     -> Result<(), RangePasteError> {
        if field.chars().count() > MAX_CELL_CHARS {
            return Err(RangePasteError::TooLarge);
        }
        rows.last_mut()
            .ok_or(RangePasteError::InvalidText)?
            .push(std::mem::take(field));
        *in_quotes = false;
        Ok(())
    };

    while index < chars.len() {
        let character = chars[index];
        if in_quotes {
            if character == '"' {
                if chars.get(index + 1) == Some(&'"') {
                    field.push('"');
                    index += 2;
                    continue;
                }
                in_quotes = false;
            } else {
                if character.is_control() && !matches!(character, '\t' | '\n' | '\r') {
                    return Err(RangePasteError::InvalidText);
                }
                field.push(character);
            }
            index += 1;
            continue;
        }
        match character {
            '"' if field.is_empty() => {
                in_quotes = true;
            }
            '\t' => finish_field(&mut rows, &mut field, &mut in_quotes)?,
            '\r' => {
                finish_field(&mut rows, &mut field, &mut in_quotes)?;
                if chars.get(index + 1) == Some(&'\n') {
                    index += 1;
                }
                rows.push(Vec::new());
            }
            '\n' => {
                finish_field(&mut rows, &mut field, &mut in_quotes)?;
                rows.push(Vec::new());
            }
            _ => {
                if character.is_control() {
                    return Err(RangePasteError::InvalidText);
                }
                field.push(character);
            }
        }
        index += 1;
        if rows.len() > MAX_PASTE_ROWS {
            return Err(RangePasteError::TooLarge);
        }
    }
    finish_field(&mut rows, &mut field, &mut in_quotes)?;

    // A final newline/tab only terminates the last row; it must not create an
    // extra empty row or column. All-empty text is treated as an empty clipboard.
    if rows.iter().all(|row| row.iter().all(String::is_empty)) {
        return Err(RangePasteError::Empty);
    }
    while rows.len() > 1
        && rows
            .last()
            .is_some_and(|row| row.iter().all(String::is_empty))
    {
        rows.pop();
    }
    if rows.is_empty() {
        return Err(RangePasteError::Empty);
    }
    for row in &mut rows {
        while row.len() > 1 && row.last().is_some_and(String::is_empty) {
            row.pop();
        }
        if row.is_empty() {
            row.push(String::new());
        }
    }
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    if columns == 0 || columns > MAX_PASTE_COLUMNS || rows.len() > MAX_PASTE_ROWS {
        return Err(RangePasteError::TooLarge);
    }
    let count = rows
        .len()
        .checked_mul(columns)
        .ok_or(RangePasteError::TooLarge)?;
    if count > MAX_PASTE_CELLS {
        return Err(RangePasteError::TooManyCells);
    }
    for row in &mut rows {
        row.resize(columns, String::new());
    }
    Ok(ParsedGrid {
        rows: rows.len(),
        columns,
        cells: rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_lf_empty_cells_and_trailing_delimiters_are_rectangular() {
        let grid = parse_grid("A\tB\t\r\n\t2\t\r\nC\t\t3\r\n").unwrap();
        assert_eq!((grid.rows, grid.columns), (3, 3));
        assert_eq!(grid.cells[0], ["A", "B", ""]);
        assert_eq!(grid.cells[1], ["", "2", ""]);
        assert_eq!(grid.cells[2], ["C", "", "3"]);
        assert_eq!(parse_grid("A\tB\n").unwrap().cells, vec![["A", "B"]]);
        assert_eq!(parse_grid("A\tB\t\t").unwrap().cells, vec![["A", "B"]]);
    }

    #[test]
    fn quoted_cells_keep_tabs_and_newlines_as_text() {
        let grid = parse_grid("\"A\tB\"\tC\r\n\"line1\nline2\"\tD").unwrap();
        assert_eq!(grid.rows, 2);
        assert_eq!(grid.columns, 2);
        assert_eq!(grid.cells[0][0], "A\tB");
        assert_eq!(grid.cells[1][0], "line1\nline2");
        assert_eq!(parse_grid("\"a\"\"b\"\t2").unwrap().cells[0][0], "a\"b");
    }

    #[test]
    fn leading_equals_is_data_not_a_formula() {
        let grid = parse_grid("=SUM(A1)\t=1+1\n").unwrap();
        assert_eq!(grid.cells[0], ["=SUM(A1)", "=1+1"]);
    }

    #[test]
    fn oversized_ranges_are_rejected_before_filling() {
        assert_eq!(parse_grid(""), Err(RangePasteError::Empty));
        let wide = "x\t".repeat(MAX_PASTE_COLUMNS + 1);
        assert!(parse_grid(&wide).is_err());
        let tall = "x\n".repeat(MAX_PASTE_ROWS + 1);
        assert!(parse_grid(&tall).is_err());
    }

    #[test]
    fn utf8_and_empty_grid_failures_are_deliberate() {
        assert_eq!(parse_grid("Кириллица\t2").unwrap().cells[0][0], "Кириллица");
        assert_eq!(parse_grid("\n\n").unwrap_err(), RangePasteError::Empty);
        assert_eq!(parse_grid("\0").unwrap_err(), RangePasteError::InvalidText);
    }
}
