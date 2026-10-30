use crate::xlsx::{SpreadsheetCell, SpreadsheetModel};
use calamine::{open_workbook_auto, Data, Range, Reader, SheetType};
use std::path::Path;

const MAX_XLS_BYTES: u64 = 64 * 1024 * 1024;
const MAX_XLS_SHEETS: usize = 64;
const MAX_XLS_CELLS: usize = 500_000;
const MAX_XLS_ROWS: usize = 1_048_576;
const MAX_XLS_COLUMNS: usize = 16_384;

#[derive(Debug)]
pub enum XlsError {
    Workbook(calamine::Error),
    MissingWorksheet,
    ResourceLimit(String),
    Io(std::io::Error),
}

impl std::fmt::Display for XlsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Workbook(error) => write!(f, "XLS workbook error: {error}"),
            Self::MissingWorksheet => write!(f, "XLS does not contain a readable worksheet"),
            Self::ResourceLimit(reason) => write!(f, "XLS exceeds a safety limit: {reason}"),
            Self::Io(error) => write!(f, "XLS file operation failed: {error}"),
        }
    }
}

impl std::error::Error for XlsError {}

impl From<std::io::Error> for XlsError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<calamine::Error> for XlsError {
    fn from(value: calamine::Error) -> Self {
        Self::Workbook(value)
    }
}

pub struct XlsPackage {
    pub model: SpreadsheetModel,
    pub sheets: Vec<SpreadsheetModel>,
}

impl XlsPackage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, XlsError> {
        let path = path.as_ref();
        if std::fs::metadata(path)?.len() > MAX_XLS_BYTES {
            return Err(XlsError::ResourceLimit("file is larger than 64 MiB".into()));
        }
        let mut workbook = open_workbook_auto(path)?;
        // sheet_names() includes charts, dialogs and macro sheets. Their streams
        // are not worksheet cell data and must never be passed to worksheet_range.
        let sheet_names: Vec<_> = workbook
            .sheets_metadata()
            .iter()
            .filter(|sheet| sheet.typ == SheetType::WorkSheet)
            .map(|sheet| sheet.name.clone())
            .collect();
        if sheet_names.len() > MAX_XLS_SHEETS {
            return Err(XlsError::ResourceLimit("too many worksheets".into()));
        }
        let mut sheets = Vec::new();
        let mut total_cells = 0usize;
        for sheet_name in sheet_names {
            let sheet =
                spreadsheet_from_range(sheet_name.clone(), workbook.worksheet_range(&sheet_name)?)?;
            total_cells = total_cells.saturating_add(sheet.cells.len());
            if total_cells > MAX_XLS_CELLS {
                return Err(XlsError::ResourceLimit(
                    "workbook cell count exceeds the 500000-cell budget".into(),
                ));
            }
            sheets.push(sheet);
        }
        let model = sheets.first().cloned().ok_or(XlsError::MissingWorksheet)?;
        Ok(Self { model, sheets })
    }
}

fn spreadsheet_from_range(
    sheet_name: String,
    range: Range<Data>,
) -> Result<SpreadsheetModel, XlsError> {
    let (start_row, start_column) = range.start().unwrap_or((0, 0));
    let rows = range.height();
    let columns = range.width();
    if rows > MAX_XLS_ROWS || columns > MAX_XLS_COLUMNS {
        return Err(XlsError::ResourceLimit(
            "worksheet dimensions exceed Excel limits".into(),
        ));
    }
    let dimensions = rows
        .checked_mul(columns)
        .ok_or_else(|| XlsError::ResourceLimit("worksheet dimensions overflowed".into()))?;
    if dimensions > MAX_XLS_CELLS {
        return Err(XlsError::ResourceLimit(
            "worksheet dimensions exceed the 500000-cell budget".into(),
        ));
    }
    let mut cells = Vec::new();
    for (row, column, value) in range.cells() {
        if matches!(value, Data::Empty) {
            continue;
        }
        if cells.len() >= MAX_XLS_CELLS {
            return Err(XlsError::ResourceLimit(
                "worksheet cell count exceeds the 500000-cell budget".into(),
            ));
        }
        let displayed = displayed_cell_value(value);
        if displayed.len() > 1_048_576 {
            return Err(XlsError::ResourceLimit("a cell value exceeds 1 MiB".into()));
        }
        cells.push(SpreadsheetCell {
            row: start_row as usize + row,
            column: start_column as usize + column,
            value: displayed,
        });
    }
    let row_count = (start_row as usize).saturating_add(rows).min(MAX_XLS_ROWS);
    let column_count = (start_column as usize)
        .saturating_add(columns)
        .min(MAX_XLS_COLUMNS);
    Ok(SpreadsheetModel {
        sheet_name,
        cells,
        row_count,
        column_count,
    })
}

fn displayed_cell_value(value: &Data) -> String {
    if let Data::DateTime(datetime) = value {
        let serial = datetime.as_f64();
        if datetime.is_datetime() && serial.is_finite() && (0.0..2_958_466.0).contains(&serial) {
            let (year, month, day, hour, minute, second, millis) = datetime.to_ymd_hms_milli();
            let date = format!("{year:04}-{month:02}-{day:02}");
            return if (hour, minute, second, millis) == (0, 0, 0, 0) {
                date
            } else if millis == 0 {
                format!("{date}T{hour:02}:{minute:02}:{second:02}")
            } else {
                format!("{date}T{hour:02}:{minute:02}:{second:02}.{millis:03}")
            };
        }
        // Elapsed times can exceed 24 hours and are not calendar dates.
        let milliseconds = serial * 86_400_000.0;
        if datetime.is_duration()
            && milliseconds.is_finite()
            && milliseconds.abs() < i64::MAX as f64
        {
            let total = milliseconds.round() as i64;
            let sign = if total < 0 { "-" } else { "" };
            let total = total.unsigned_abs();
            let (hours, minutes, seconds, millis) = (
                total / 3_600_000,
                total / 60_000 % 60,
                total / 1_000 % 60,
                total % 1_000,
            );
            return if millis == 0 {
                format!("{sign}{hours:02}:{minutes:02}:{seconds:02}")
            } else {
                format!("{sign}{hours:02}:{minutes:02}:{seconds:02}.{millis:03}")
            };
        }
    }
    value.to_string()
}

#[cfg(test)]
mod ods_tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn opens_a_minimal_ods_sheet_read_only() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("sample.ods");
        let content = r#"<?xml version="1.0" encoding="UTF-8"?><office:document-content xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.2"><office:body><office:spreadsheet><table:table table:name="Data"><table:table-row><table:table-cell office:value-type="string"><text:p>Hello</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="42"><text:p>42</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet></office:body></office:document-content>"#;
        let file = std::fs::File::create(&path).unwrap();
        let mut zip = ZipWriter::new(file);
        zip.start_file(
            "mimetype",
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(b"application/vnd.oasis.opendocument.spreadsheet")
            .unwrap();
        zip.start_file("META-INF/manifest.xml", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.spreadsheet"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>"#).unwrap();
        zip.start_file("content.xml", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(content.as_bytes()).unwrap();
        zip.finish().unwrap();
        let package = XlsPackage::open(&path).unwrap();
        assert_eq!(package.model.sheet_name, "Data");
        assert_eq!(package.model.cell_value(0, 0), "Hello");
        assert_eq!(package.model.cell_value(0, 1), "42");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{Cell, ExcelDateTime, ExcelDateTimeType};

    #[test]
    fn dates_respect_1900_and_1904_epochs_and_preserve_time() {
        for (serial, epoch_1904, expected) in [
            (44197.0, false, "2021-01-01"),
            (42735.0, true, "2021-01-01"),
            (0.0, true, "1904-01-01"),
            (61.0, false, "1900-03-01"),
            (44197.5, false, "2021-01-01T12:00:00"),
            (44197.0 + 1.25 / 86400.0, false, "2021-01-01T00:00:01.250"),
        ] {
            let value = Data::DateTime(ExcelDateTime::new(
                serial,
                ExcelDateTimeType::DateTime,
                epoch_1904,
            ));
            assert_eq!(displayed_cell_value(&value), expected);
        }
        assert_eq!(displayed_cell_value(&Data::Float(44197.0)), "44197");
        assert_eq!(
            displayed_cell_value(&Data::DateTimeIso("2021-01-01T12:00:00Z".into())),
            "2021-01-01T12:00:00Z"
        );
    }

    #[test]
    fn durations_and_invalid_serials_do_not_become_calendar_dates() {
        for (serial, expected) in [(1.5, "36:00:00"), (-1.5, "-36:00:00"), (0.0, "00:00:00")] {
            let value = Data::DateTime(ExcelDateTime::new(
                serial,
                ExcelDateTimeType::TimeDelta,
                false,
            ));
            assert_eq!(displayed_cell_value(&value), expected);
        }
        for serial in [f64::NAN, f64::INFINITY, -1.0, 3_000_000.0] {
            let value = Data::DateTime(ExcelDateTime::new(
                serial,
                ExcelDateTimeType::DateTime,
                false,
            ));
            assert_eq!(displayed_cell_value(&value), value.to_string());
        }
    }

    #[test]
    fn preserves_absolute_xls_cell_coordinates() {
        let range = Range::from_sparse(vec![
            Cell::new((1, 2), Data::String("Budget".into())),
            Cell::new((3, 4), Data::Int(42)),
        ]);
        let model = spreadsheet_from_range("Sheet1".into(), range).unwrap();
        assert_eq!(model.cell_value(1, 2), "Budget");
        assert_eq!(model.cell_value(3, 4), "42");
        assert_eq!((model.row_count, model.column_count), (4, 5));
    }
}
