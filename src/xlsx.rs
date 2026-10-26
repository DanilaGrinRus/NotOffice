use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use std::fs::File;
use std::io::{Cursor, Read};
use std::path::Path;
use zip::read::ZipArchive;

const WORKBOOK_XML: &str = "xl/workbook.xml";
const WORKBOOK_RELS_XML: &str = "xl/_rels/workbook.xml.rels";
const SHARED_STRINGS_XML: &str = "xl/sharedStrings.xml";
const MAX_XLSX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_XLSX_PART_BYTES: u64 = 32 * 1024 * 1024;
const MAX_XLSX_UNCOMPRESSED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_XLSX_PARTS: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpreadsheetCell {
    pub row: usize,
    pub column: usize,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SpreadsheetModel {
    pub sheet_name: String,
    pub cells: Vec<SpreadsheetCell>,
    pub row_count: usize,
    pub column_count: usize,
}

impl SpreadsheetModel {
    pub fn cell_value(&self, row: usize, column: usize) -> &str {
        self.cells
            .iter()
            .find(|cell| cell.row == row && cell.column == column)
            .map(|cell| cell.value.as_str())
            .unwrap_or("")
    }
}

#[derive(Debug)]
pub enum XlsxError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Xml(quick_xml::Error),
    MissingWorkbook,
    MissingWorksheet,
    InvalidWorkbook(String),
    ResourceLimit(String),
}

impl std::fmt::Display for XlsxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "file operation failed: {error}"),
            Self::Zip(error) => write!(f, "XLSX ZIP operation failed: {error}"),
            Self::Xml(error) => write!(f, "XLSX XML operation failed: {error}"),
            Self::MissingWorkbook => write!(f, "XLSX does not contain a workbook"),
            Self::MissingWorksheet => write!(f, "XLSX does not contain a readable worksheet"),
            Self::InvalidWorkbook(reason) => write!(f, "invalid XLSX workbook: {reason}"),
            Self::ResourceLimit(reason) => write!(f, "XLSX exceeds a safety limit: {reason}"),
        }
    }
}

impl std::error::Error for XlsxError {}
impl From<std::io::Error> for XlsxError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<zip::result::ZipError> for XlsxError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}
impl From<quick_xml::Error> for XlsxError {
    fn from(value: quick_xml::Error) -> Self {
        Self::Xml(value)
    }
}

pub struct XlsxPackage {
    pub model: SpreadsheetModel,
    pub sheets: Vec<SpreadsheetModel>,
    pub formats:
        std::collections::BTreeMap<crate::workbook_edit::CellKey, crate::workbook_edit::CellFormat>,
}

impl XlsxPackage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, XlsxError> {
        let path = path.as_ref();
        if std::fs::metadata(path)?.len() > MAX_XLSX_PACKAGE_BYTES {
            return Err(XlsxError::ResourceLimit(
                "package is larger than 64 MiB".into(),
            ));
        }
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)?;
        if archive.len() > MAX_XLSX_PARTS {
            return Err(XlsxError::ResourceLimit(
                "package has more than 4096 entries".into(),
            ));
        }
        let mut total_uncompressed = 0u64;
        let workbook = read_zip_part(&mut archive, WORKBOOK_XML, &mut total_uncompressed).map_err(
            |error| match error {
                XlsxError::Zip(zip::result::ZipError::FileNotFound) => XlsxError::MissingWorkbook,
                other => other,
            },
        )?;
        let relationships =
            read_zip_part(&mut archive, WORKBOOK_RELS_XML, &mut total_uncompressed)?;
        let shared_strings =
            match read_zip_part(&mut archive, SHARED_STRINGS_XML, &mut total_uncompressed) {
                Ok(xml) => parse_shared_strings(&xml)?,
                Err(XlsxError::Zip(zip::result::ZipError::FileNotFound)) => Vec::new(),
                Err(error) => return Err(error),
            };
        let (styles, date_styles) =
            match read_zip_part(&mut archive, "xl/styles.xml", &mut total_uncompressed) {
                Ok(xml) => (parse_font_styles(&xml)?, parse_date_styles(&xml)?),
                Err(XlsxError::Zip(zip::result::ZipError::FileNotFound)) => {
                    (Vec::new(), Vec::new())
                }
                Err(error) => return Err(error),
            };
        let mut formats = std::collections::BTreeMap::new();
        let mut sheets = Vec::new();
        for (sheet_name, relationship_id) in workbook_sheets(&workbook)? {
            let worksheet_path = worksheet_path(&relationships, &relationship_id)?;
            let worksheet = read_zip_part(&mut archive, &worksheet_path, &mut total_uncompressed)?;
            let mut reader = Reader::from_reader(worksheet.as_slice());
            loop {
                match reader.read_event()? {
                    Event::Start(e) | Event::Empty(e) if e.name().as_ref() == b"c" => {
                        if let Some((position, format)) =
                            attribute(&e, b"r").and_then(|r| cell_position(&r)).zip(
                                attribute(&e, b"s")
                                    .and_then(|s| s.parse::<usize>().ok())
                                    .and_then(|s| styles.get(s)),
                            )
                        {
                            formats.insert((sheets.len(), position.0, position.1), *format);
                        }
                    }
                    Event::Eof => break,
                    _ => {}
                }
            }
            sheets.push(parse_worksheet(
                &worksheet,
                &shared_strings,
                sheet_name,
                &date_styles,
            )?);
        }
        let model = sheets.first().cloned().ok_or(XlsxError::MissingWorksheet)?;
        Ok(Self {
            model,
            sheets,
            formats,
        })
    }
}

fn read_zip_part<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
    total_uncompressed: &mut u64,
) -> Result<Vec<u8>, XlsxError> {
    let mut entry = archive.by_name(name)?;
    let declared = entry.size();
    if declared > MAX_XLSX_PART_BYTES
        || total_uncompressed.saturating_add(declared) > MAX_XLSX_UNCOMPRESSED_BYTES
    {
        return Err(XlsxError::ResourceLimit(
            "uncompressed XLSX part exceeds the 32 MiB / 128 MiB budget".into(),
        ));
    }
    let mut content = Vec::new();
    entry
        .by_ref()
        .take(MAX_XLSX_PART_BYTES + 1)
        .read_to_end(&mut content)?;
    if content.len() as u64 > MAX_XLSX_PART_BYTES
        || total_uncompressed.saturating_add(content.len() as u64) > MAX_XLSX_UNCOMPRESSED_BYTES
    {
        return Err(XlsxError::ResourceLimit(
            "uncompressed XLSX part exceeds the 32 MiB / 128 MiB budget".into(),
        ));
    }
    *total_uncompressed = total_uncompressed.saturating_add(content.len() as u64);
    Ok(content)
}

fn attribute(event: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    event
        .attributes()
        .flatten()
        .find(|attribute| attribute.key.as_ref() == name)
        .and_then(|attribute| attribute.unescape_value().ok().map(|v| v.into_owned()))
}

fn parse_font_styles(xml: &[u8]) -> Result<Vec<crate::workbook_edit::CellFormat>, XlsxError> {
    use crate::workbook_edit::CellFormat;
    let mut fonts = Vec::new();
    let mut styles = Vec::new();
    let mut reader = Reader::from_reader(xml);
    let mut font = None;
    let mut in_xfs = false;
    loop {
        match reader.read_event()? {
            Event::Start(e) if e.name().as_ref() == b"font" => font = Some(CellFormat::default()),
            Event::Empty(e) if e.name().as_ref() == b"font" => fonts.push(CellFormat::default()),
            Event::End(e) if e.name().as_ref() == b"font" => {
                if let Some(f) = font.take() {
                    fonts.push(f);
                }
            }
            Event::Start(e) if e.name().as_ref() == b"cellXfs" => in_xfs = true,
            Event::End(e) if e.name().as_ref() == b"cellXfs" => in_xfs = false,
            Event::Start(e) | Event::Empty(e) => {
                if let Some(f) = font.as_mut() {
                    if e.name().as_ref() == b"sz" {
                        if let Some(size) = attribute(&e, b"val")
                            .and_then(|s| s.parse::<u16>().ok())
                            .filter(|s| (6..=72).contains(s))
                        {
                            f.size = size;
                        }
                    } else if e.name().as_ref() == b"b" {
                        f.bold = !attribute(&e, b"val").is_some_and(|v| v == "0" || v == "false");
                    }
                }
                if in_xfs && e.name().as_ref() == b"xf" {
                    let id = attribute(&e, b"fontId")
                        .and_then(|s| s.parse::<usize>().ok())
                        .unwrap_or(0);
                    styles.push(fonts.get(id).copied().unwrap_or_default());
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(styles)
}

fn workbook_sheets(xml: &[u8]) -> Result<Vec<(String, String)>, XlsxError> {
    let mut reader = Reader::from_reader(Cursor::new(xml));
    let mut buffer = Vec::new();
    let mut sheets = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) | Event::Start(event) if event.name().as_ref() == b"sheet" => {
                let name = attribute(&event, b"name")
                    .ok_or_else(|| XlsxError::InvalidWorkbook("sheet without a name".into()))?
                    .to_owned();
                let relationship_id = attribute(&event, b"r:id")
                    .ok_or_else(|| {
                        XlsxError::InvalidWorkbook("sheet without a relationship".into())
                    })?
                    .to_owned();
                sheets.push((name, relationship_id));
            }
            Event::Eof => {
                return if sheets.is_empty() {
                    Err(XlsxError::MissingWorksheet)
                } else {
                    Ok(sheets)
                }
            }
            _ => {}
        }
        buffer.clear();
    }
}

fn worksheet_path(xml: &[u8], relationship_id: &str) -> Result<String, XlsxError> {
    let mut reader = Reader::from_reader(Cursor::new(xml));
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) | Event::Start(event)
                if event.name().as_ref() == b"Relationship" =>
            {
                let id = attribute(&event, b"Id");
                if id.as_deref() == Some(relationship_id) {
                    if attribute(&event, b"TargetMode")
                        .is_some_and(|mode| mode.eq_ignore_ascii_case("External"))
                    {
                        return Err(XlsxError::InvalidWorkbook(
                            "external worksheet relationships are not followed".into(),
                        ));
                    }
                    let target = attribute(&event, b"Target").ok_or_else(|| {
                        XlsxError::InvalidWorkbook("worksheet relationship without a target".into())
                    })?;
                    let part = if target.starts_with('/') {
                        target.trim_start_matches('/').to_owned()
                    } else {
                        format!("xl/{target}")
                    };
                    if part.contains([':', '\\'])
                        || part
                            .split('/')
                            .any(|segment| segment.is_empty() || segment == "." || segment == "..")
                    {
                        return Err(XlsxError::InvalidWorkbook(
                            "unsafe worksheet relationship target".into(),
                        ));
                    }
                    return Ok(part);
                }
            }
            Event::Eof => return Err(XlsxError::MissingWorksheet),
            _ => {}
        }
        buffer.clear();
    }
}

fn parse_shared_strings(xml: &[u8]) -> Result<Vec<String>, XlsxError> {
    let mut reader = Reader::from_reader(Cursor::new(xml));
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut strings = Vec::new();
    let mut value = String::new();
    let mut in_string = false;
    let mut in_text = false;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.name().as_ref() == b"si" => {
                in_string = true;
                value.clear();
            }
            Event::End(event) if event.name().as_ref() == b"si" => {
                in_string = false;
                strings.push(std::mem::take(&mut value));
            }
            Event::Start(event) if in_string && event.name().as_ref() == b"t" => in_text = true,
            Event::End(event) if event.name().as_ref() == b"t" => in_text = false,
            Event::Text(text) if in_text => {
                value.push_str(
                    &text
                        .unescape()
                        .map_err(|error| XlsxError::InvalidWorkbook(error.to_string()))?,
                );
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(strings)
}

fn cell_position(reference: &str) -> Option<(usize, usize)> {
    let letters: String = reference
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .collect();
    let row = reference[letters.len()..]
        .parse::<usize>()
        .ok()?
        .checked_sub(1)?;
    let column = letters.chars().fold(0usize, |value, letter| {
        value * 26 + (letter.to_ascii_uppercase() as usize - 'A' as usize + 1)
    });
    column.checked_sub(1).map(|column| (row, column))
}

fn parse_date_styles(xml: &[u8]) -> Result<Vec<bool>, XlsxError> {
    use std::collections::HashMap;
    let mut custom = HashMap::<u32, String>::new();
    let mut styles = Vec::new();
    let mut in_cell_xfs = false;
    let mut reader = Reader::from_reader(xml);
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == b"numFmt" => {
                if let (Some(id), Some(code)) =
                    (attribute(&e, b"numFmtId"), attribute(&e, b"formatCode"))
                {
                    if let Ok(id) = id.parse::<u32>() {
                        custom.insert(id, code);
                    }
                }
            }
            Event::Start(e) if e.name().as_ref() == b"cellXfs" => in_cell_xfs = true,
            Event::End(e) if e.name().as_ref() == b"cellXfs" => in_cell_xfs = false,
            Event::Start(e) | Event::Empty(e) if in_cell_xfs && e.name().as_ref() == b"xf" => {
                let id = attribute(&e, b"numFmtId")
                    .and_then(|value| value.parse::<u32>().ok())
                    .unwrap_or(0);
                let is_date = matches!(id, 14..=22 | 27..=36 | 45..=47 | 50..=58)
                    || custom
                        .get(&id)
                        .is_some_and(|code| format_looks_like_date(code));
                styles.push(is_date);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(styles)
}

fn format_looks_like_date(code: &str) -> bool {
    let lower = code.to_ascii_lowercase();
    lower
        .chars()
        .any(|character| character == 'y' || character == 'd')
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month as u32, day as u32)
}

/// Converts an Excel 1900-system serial (modern dates only) to ISO YYYY-MM-DD.
fn excel_serial_to_iso(serial: f64) -> Option<String> {
    if !serial.is_finite() {
        return None;
    }
    let days = serial.floor() as i64;
    // Serial 61 is 1900-03-01; earlier values are affected by Excel's fake 1900-02-29.
    if days < 61 {
        return None;
    }
    let (year, month, day) = civil_from_days(days - 25_569);
    if !(1900..=9999).contains(&year) {
        return None;
    }
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

fn parse_worksheet(
    xml: &[u8],
    shared_strings: &[String],
    sheet_name: String,
    date_styles: &[bool],
) -> Result<SpreadsheetModel, XlsxError> {
    let mut reader = Reader::from_reader(Cursor::new(xml));
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut cells = Vec::new();
    let mut current_reference = None;
    let mut current_type = String::new();
    let mut current_style = None;
    let mut value = String::new();
    let mut in_value = false;
    let mut in_inline_text = false;

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.name().as_ref() == b"c" => {
                current_reference = attribute(&event, b"r");
                current_type = attribute(&event, b"t").unwrap_or_default();
                current_style =
                    attribute(&event, b"s").and_then(|style| style.parse::<usize>().ok());
                value.clear();
            }
            Event::Start(event) if event.name().as_ref() == b"v" => in_value = true,
            Event::End(event) if event.name().as_ref() == b"v" => in_value = false,
            Event::Start(event) if current_type == "inlineStr" && event.name().as_ref() == b"t" => {
                in_inline_text = true
            }
            Event::End(event) if event.name().as_ref() == b"t" => in_inline_text = false,
            Event::Text(text) if in_value || in_inline_text => {
                value.push_str(
                    &text
                        .unescape()
                        .map_err(|error| XlsxError::InvalidWorkbook(error.to_string()))?,
                );
            }
            Event::End(event) if event.name().as_ref() == b"c" => {
                if let Some(reference) = current_reference.take() {
                    let (row, column) = cell_position(&reference).ok_or_else(|| {
                        XlsxError::InvalidWorkbook(format!("invalid cell reference {reference}"))
                    })?;
                    let mut displayed_value = match current_type.as_str() {
                        "s" => value
                            .parse::<usize>()
                            .ok()
                            .and_then(|index| shared_strings.get(index))
                            .cloned()
                            .unwrap_or_else(|| value.clone()),
                        "b" if value == "1" => "TRUE".into(),
                        "b" if value == "0" => "FALSE".into(),
                        _ => value.clone(),
                    };
                    let date_style = current_style
                        .and_then(|index| date_styles.get(index))
                        .copied()
                        .unwrap_or(false);
                    if date_style && current_type != "d" {
                        if let Some(iso) = value.parse::<f64>().ok().and_then(excel_serial_to_iso) {
                            displayed_value = iso;
                        }
                    }
                    cells.push(SpreadsheetCell {
                        row,
                        column,
                        value: displayed_value,
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    let row_count = cells.iter().map(|cell| cell.row + 1).max().unwrap_or(0);
    let column_count = cells.iter().map(|cell| cell.column + 1).max().unwrap_or(0);
    Ok(SpreadsheetModel {
        sheet_name,
        cells,
        row_count,
        column_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    const WORKBOOK: &str = r#"<workbook xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/><sheet name="Notes" sheetId="2" r:id="rId2"/></sheets></workbook>"#;
    const RELATIONSHIPS: &str = r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Target="worksheets/sheet2.xml"/></Relationships>"#;
    const SHARED_STRINGS: &str = r#"<sst><si><t>Name</t></si><si><t>Ada</t></si></sst>"#;
    const SHEET: &str = r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1"><v>42</v></c></row><row r="2"><c r="A2" t="s"><v>1</v></c><c r="B2" t="b"><v>1</v></c></row></sheetData></worksheet>"#;
    const SECOND_SHEET: &str = r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Second sheet</t></is></c></row></sheetData></worksheet>"#;

    #[test]
    fn formulas_use_cached_values_and_external_or_escaping_worksheet_targets_are_rejected() {
        let sheet = parse_worksheet(br#"<worksheet><sheetData><row r="1"><c r="A1" t="str"><f>WEBSERVICE("https://example.invalid/script")</f><v>Cached only</v></c></row></sheetData></worksheet>"#, &[], "Data".into(), &[]).unwrap();
        assert_eq!(sheet.cell_value(0, 0), "Cached only");
        for xml in [
            r#"<Relationships><Relationship Id="r" Target="https://example.invalid/sheet.xml" TargetMode="External"/></Relationships>"#,
            r#"<Relationships><Relationship Id="r" Target="../outside.xml"/></Relationships>"#,
            r#"<Relationships><Relationship Id="r" Target="file:///C:/outside.xml"/></Relationships>"#,
        ] {
            assert!(worksheet_path(xml.as_bytes(), "r").is_err());
        }
        assert_eq!(worksheet_path(br#"<Relationships><Relationship Id="r" Target="/xl/worksheets/sheet1.xml"/></Relationships>"#, "r").unwrap(), "xl/worksheets/sheet1.xml");
    }

    fn fixture(path: &Path) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        for (name, content) in [
            (WORKBOOK_XML, WORKBOOK),
            (WORKBOOK_RELS_XML, RELATIONSHIPS),
            (SHARED_STRINGS_XML, SHARED_STRINGS),
            ("xl/worksheets/sheet1.xml", SHEET),
            ("xl/worksheets/sheet2.xml", SECOND_SHEET),
        ] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn rejects_an_xlsx_part_that_expands_beyond_the_budget() {
        let folder = tempdir().unwrap();
        let file = folder.path().join("bomb.xlsx");
        let mut zip = ZipWriter::new(File::create(&file).unwrap());
        for (name, content) in [(WORKBOOK_XML, WORKBOOK), (WORKBOOK_RELS_XML, RELATIONSHIPS)] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.start_file("xl/worksheets/sheet1.xml", SimpleFileOptions::default())
            .unwrap();
        let chunk = vec![b' '; 1024 * 1024];
        for _ in 0..33 {
            zip.write_all(&chunk).unwrap();
        }
        zip.finish().unwrap();
        assert!(matches!(
            XlsxPackage::open(&file),
            Err(XlsxError::ResourceLimit(_))
        ));
    }

    #[test]
    fn date_styles_become_iso_dates_while_numbers_and_booleans_keep_types() {
        let folder = tempdir().unwrap();
        let file = folder.path().join("typed.xlsx");
        let sheet = r#"<worksheet><sheetData><row r="1"><c r="A1" s="1"><v>25569</v></c><c r="B1" s="2"><v>25569</v></c><c r="C1" s="1"><v>42</v></c><c r="D1" t="b"><v>1</v></c></row></sheetData></worksheet>"#;
        let styles = r#"<styleSheet><numFmts count="1"><numFmt numFmtId="164" formatCode="dd.mm.yyyy"/></numFmts><fonts count="1"><font/></fonts><cellXfs count="3"><xf numFmtId="0"/><xf numFmtId="14"/><xf numFmtId="164"/></cellXfs></styleSheet>"#;
        let mut zip = ZipWriter::new(File::create(&file).unwrap());
        for (name, content) in [
            (WORKBOOK_XML, WORKBOOK),
            (WORKBOOK_RELS_XML, RELATIONSHIPS),
            ("xl/styles.xml", styles),
            ("xl/worksheets/sheet1.xml", sheet),
            ("xl/worksheets/sheet2.xml", SECOND_SHEET),
        ] {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        let package = XlsxPackage::open(&file).unwrap();
        assert_eq!(package.model.cell_value(0, 0), "1970-01-01");
        assert_eq!(package.model.cell_value(0, 1), "1970-01-01");
        assert_eq!(package.model.cell_value(0, 2), "42");
        assert_eq!(package.model.cell_value(0, 3), "TRUE");
    }

    #[test]
    fn opens_first_sheet_with_shared_strings_and_numbers() {
        let folder = tempdir().unwrap();
        let file = folder.path().join("sample.xlsx");
        fixture(&file);
        let package = XlsxPackage::open(&file).unwrap();
        assert_eq!(package.model.sheet_name, "Data");
        assert_eq!(package.model.cell_value(0, 0), "Name");
        assert_eq!(package.model.cell_value(0, 1), "42");
        assert_eq!(package.model.cell_value(1, 0), "Ada");
        assert_eq!(package.model.cell_value(1, 1), "TRUE");
        assert_eq!(package.sheets.len(), 2);
        assert_eq!(package.sheets[1].sheet_name, "Notes");
        assert_eq!(package.sheets[1].cell_value(0, 0), "Second sheet");
    }
}
