//! Quoted CSV/TSV import into the same read-only spreadsheet model as XLS/XLSX.
use crate::xlsx::{SpreadsheetCell, SpreadsheetModel};
use std::path::Path;

const MAX_BYTES: usize = 8 * 1024 * 1024;
const MAX_CELLS: usize = 500_000;

pub fn open(path: &Path) -> Result<SpreadsheetModel, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if metadata.len() as usize > MAX_BYTES {
        return Err("CSV/TSV is larger than 8 MiB".into());
    }
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let text = decode(&bytes);
    let delimiter = detect_delimiter(&text, path);
    let rows = parse(&text, delimiter)?;
    let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut cells = Vec::new();
    for (row, values) in rows.iter().enumerate() {
        for (column, value) in values.iter().enumerate() {
            if !value.is_empty() {
                cells.push(SpreadsheetCell {
                    row,
                    column,
                    value: value.clone(),
                });
            }
        }
    }
    let sheet_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("CSV")
        .to_owned();
    Ok(SpreadsheetModel {
        sheet_name,
        cells,
        row_count: rows.len(),
        column_count,
    })
}

fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_owned();
    }
    let (decoded, _, _) = encoding_rs::WINDOWS_1251.decode(bytes);
    decoded.into_owned()
}

fn detect_delimiter(text: &str, path: &Path) -> char {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("tsv"))
    {
        return '\t';
    }
    let mut commas = 0;
    let mut semicolons = 0;
    let mut tabs = 0;
    let mut quoted = false;
    for character in text.chars() {
        match character {
            '"' => quoted = !quoted,
            ',' if !quoted => commas += 1,
            ';' if !quoted => semicolons += 1,
            '\t' if !quoted => tabs += 1,
            '\n' | '\r' if !quoted => break,
            _ => {}
        }
    }
    if tabs > commas && tabs > semicolons {
        '\t'
    } else if semicolons > commas {
        ';'
    } else {
        ','
    }
}

fn parse(text: &str, delimiter: char) -> Result<Vec<Vec<String>>, String> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut characters = text.chars().peekable();
    let mut total_cells = 0usize;
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted => {
                if characters.peek() == Some(&'"') {
                    cell.push('"');
                    characters.next();
                } else {
                    quoted = false;
                }
            }
            '"' if cell.is_empty() && !quoted => quoted = true,
            character if character == delimiter && !quoted => {
                row.push(std::mem::take(&mut cell));
                total_cells += 1;
            }
            '\r' if !quoted => {
                if characters.peek() == Some(&'\n') {
                    characters.next();
                }
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                total_cells += 1;
            }
            '\n' if !quoted => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
                total_cells += 1;
            }
            character => cell.push(character),
        }
        if total_cells > MAX_CELLS {
            return Err("CSV/TSV exceeds the 500,000-cell limit".into());
        }
    }
    if quoted {
        return Err("CSV/TSV has an unterminated quoted field".into());
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    if rows.is_empty() {
        return Err("CSV/TSV is empty".into());
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(name: &str, content: &[u8]) -> (tempfile::TempDir, std::path::PathBuf) {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content).unwrap();
        (folder, path)
    }

    #[test]
    fn quoted_commas_newlines_and_semicolon_detection_are_preserved() {
        let (_folder, path) = write(
            "data.csv",
            b"Name,Note\r\n\"Ada, A.\",\"line 1\nline 2\"\r\nBob,plain\r\n",
        );
        let sheet = open(&path).unwrap();
        assert_eq!(sheet.row_count, 3);
        assert_eq!(sheet.column_count, 2);
        assert_eq!(sheet.cell_value(1, 0), "Ada, A.");
        assert_eq!(sheet.cell_value(1, 1), "line 1\nline 2");
        let (_folder, path) = write("data.csv", "a;b;c\n1;2;3\n".as_bytes());
        assert_eq!(open(&path).unwrap().column_count, 3);
    }

    #[test]
    fn tsv_and_windows_1251_are_supported() {
        let (_folder, path) = write("data.tsv", "Имя\tВозраст\nЁжик\t5\n".as_bytes());
        let sheet = open(&path).unwrap();
        assert_eq!(sheet.cell_value(1, 0), "Ёжик");
        assert_eq!(sheet.cell_value(1, 1), "5");
        let bytes = [0xC8, 0xEC, 0xFF, b',', b'x', b'\n'];
        let (_folder, path) = write("cp1251.csv", &bytes);
        let sheet = open(&path).unwrap();
        assert_eq!(sheet.cell_value(0, 0), "Имя");
    }
}
