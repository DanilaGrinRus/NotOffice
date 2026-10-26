//! Explicit values-only XLS/XLSX conversion; never evaluates formulas or copies
//! active content. Original input is protected even through filesystem aliases.
use crate::docx::{
    create_temporary_peer, paths_refer_to_same_file, replace_destination, source_fingerprint,
    DocxError, SourceFingerprint,
};
use crate::spreadsheet_ui::column_name;
use crate::xlsx::{SpreadsheetModel, XlsxPackage};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::{write::SimpleFileOptions, ZipWriter};

pub type CellKey = (usize, usize, usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellFormat {
    pub size: u16,
    pub bold: bool,
}
impl Default for CellFormat {
    fn default() -> Self {
        Self {
            size: 12,
            bold: false,
        }
    }
}

pub struct WorkbookEditor {
    origin: PathBuf,
    saved_path: Option<PathBuf>,
    fingerprint: Option<SourceFingerprint>,
    baseline: Vec<SpreadsheetModel>,
    saved_formats: BTreeMap<CellKey, CellFormat>,
    pub formats: BTreeMap<CellKey, CellFormat>,
}

impl WorkbookEditor {
    pub fn new(origin: PathBuf, sheets: &[SpreadsheetModel]) -> Self {
        Self {
            origin,
            saved_path: None,
            fingerprint: None,
            baseline: sheets.to_vec(),
            saved_formats: BTreeMap::new(),
            formats: BTreeMap::new(),
        }
    }
    pub fn saved_path(&self) -> Option<&Path> {
        self.saved_path.as_deref()
    }
    pub fn is_dirty(&self, sheets: &[SpreadsheetModel]) -> bool {
        self.saved_path.is_none() || self.baseline != sheets || self.saved_formats != self.formats
    }
    pub fn save(&mut self, path: &Path, sheets: &[SpreadsheetModel]) -> Result<(), DocxError> {
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("xlsx"))
        {
            return Err(DocxError::UnsafeSave("choose an .xlsx destination".into()));
        }
        if paths_refer_to_same_file(&self.origin, path)? {
            return Err(DocxError::UnsafeSave(
                "original workbook is protected; save a separate XLSX copy".into(),
            ));
        }
        let replacing_saved = self
            .saved_path
            .as_deref()
            .map(|p| paths_refer_to_same_file(p, path))
            .transpose()?
            .unwrap_or(false);
        let check_conflict = || -> Result<(), DocxError> {
            if replacing_saved && self.fingerprint.as_ref() != Some(&source_fingerprint(path)?) {
                return Err(DocxError::SourceChanged);
            }
            Ok(())
        };
        check_conflict()?;
        let parts = build_parts(sheets, &self.formats)?;
        if parts
            .iter()
            .map(|(name, bytes)| name.len() + bytes.len())
            .sum::<usize>()
            > 48 * 1024 * 1024
        {
            return Err(DocxError::ResourceLimit(
                "exported XML exceeds 48 MiB".into(),
            ));
        }
        let (temp, file) = create_temporary_peer(path)?;
        let result = (|| {
            let mut zip = ZipWriter::new(file);
            for (name, bytes) in parts {
                zip.start_file(name, SimpleFileOptions::default())?;
                zip.write_all(bytes.as_bytes())?;
            }
            let mut file = zip.finish()?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            let reopened = XlsxPackage::open(&temp)
                .map_err(|e| DocxError::UnsafeSave(format!("XLSX verification failed: {e}")))?;
            if reopened.sheets.len() != sheets.len()
                || reopened.sheets.iter().zip(sheets).any(|(a, b)| {
                    a.sheet_name != b.sheet_name
                        || a.cells.len() != b.cells.len()
                        || b.cells
                            .iter()
                            .any(|c| a.cell_value(c.row, c.column) != c.value)
                })
            {
                return Err(DocxError::UnsafeSave(
                    "XLSX values did not round-trip".into(),
                ));
            }
            for (sheet_index, sheet) in sheets.iter().enumerate() {
                for cell in &sheet.cells {
                    let key = (sheet_index, cell.row, cell.column);
                    if reopened.formats.get(&key).copied().unwrap_or_default()
                        != self.formats.get(&key).copied().unwrap_or_default()
                    {
                        return Err(DocxError::UnsafeSave(
                            "XLSX font formatting did not round-trip".into(),
                        ));
                    }
                }
            }
            check_conflict()?;
            replace_destination(&temp, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result?;
        self.fingerprint = Some(source_fingerprint(path)?);
        self.saved_path = Some(path.to_path_buf());
        self.baseline = sheets.to_vec();
        self.saved_formats = self.formats.clone();
        Ok(())
    }
}

fn escaped(value: &str) -> Result<String, DocxError> {
    if value.chars().any(|c| {
        !matches!(c, '\t' | '\n' | '\r') && (c < '\u{20}' || matches!(c, '\u{fffe}' | '\u{ffff}'))
    }) {
        return Err(DocxError::InvalidDocumentXml(
            "cell contains forbidden XML characters".into(),
        ));
    }
    // CR must use a character reference to survive XML line normalization.
    Ok(quick_xml::escape::escape(value).replace('\r', "&#13;"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CellKind {
    Text,
    Number,
    Bool(bool),
    Date,
}

fn looks_number(value: &str) -> bool {
    if value.is_empty() || value.trim() != value {
        return false;
    }
    let body = value.strip_prefix('-').unwrap_or(value);
    if body.len() > 1 && body.starts_with('0') && !body.starts_with("0.") {
        return false;
    }
    value.parse::<f64>().is_ok_and(|number| number.is_finite())
}

fn looks_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let month = value[5..7].parse::<u32>().ok();
    let day = value[8..10].parse::<u32>().ok();
    matches!((month, day), (Some(1..=12), Some(1..=31)))
}

fn classify(value: &str) -> CellKind {
    if value.trim().is_empty() || value != value.trim() {
        return CellKind::Text;
    }
    match value {
        "TRUE" => return CellKind::Bool(true),
        "FALSE" => return CellKind::Bool(false),
        _ => {}
    }
    if looks_iso_date(value) {
        return CellKind::Date;
    }
    if looks_number(value) {
        return CellKind::Number;
    }
    CellKind::Text
}

fn build_parts(
    sheets: &[SpreadsheetModel],
    formats: &BTreeMap<CellKey, CellFormat>,
) -> Result<Vec<(String, String)>, DocxError> {
    if sheets.is_empty()
        || sheets.len() > 64
        || sheets.iter().map(|s| s.cells.len()).sum::<usize>() > 250_000
    {
        return Err(DocxError::ResourceLimit(
            "export supports 1–64 sheets and up to 250000 cells".into(),
        ));
    }
    let mut names = BTreeSet::new();
    let mut workbook = String::from(
        r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>"#,
    );
    let mut rels = String::from(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    let mut types = String::from(
        r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#,
    );
    let mut font_styles = vec![CellFormat::default()];
    for f in formats.values() {
        if !(6..=72).contains(&f.size) {
            return Err(DocxError::UnsafeSave("font size must be 6–72 pt".into()));
        }
        if !font_styles.contains(f) {
            font_styles.push(*f);
        }
    }
    // One style per (font, number format) pair; text keeps Excel's Text format.
    let mut style_entries: Vec<(CellFormat, u32)> = Vec::new();
    let mut parts = Vec::new();
    let mut total_text = 0;
    for (sheet_index, sheet) in sheets.iter().enumerate() {
        if sheet.sheet_name.is_empty()
            || sheet.sheet_name.starts_with('\'')
            || sheet.sheet_name.ends_with('\'')
            || sheet.sheet_name.encode_utf16().count() > 31
            || sheet
                .sheet_name
                .contains(['[', ']', ':', '*', '?', '/', '\\'])
            || !names.insert(sheet.sheet_name.to_lowercase())
        {
            return Err(DocxError::UnsafeSave(
                "invalid or duplicate sheet name".into(),
            ));
        }
        let n = sheet_index + 1;
        workbook.push_str(&format!(
            r#"<sheet name="{}" sheetId="{n}" r:id="s{n}"/>"#,
            escaped(&sheet.sheet_name)?
        ));
        rels.push_str(&format!(r#"<Relationship Id="s{n}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet{n}.xml"/>"#));
        types.push_str(&format!(r#"<Override PartName="/xl/worksheets/sheet{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#));
        let mut xml = String::from(
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
        );
        let mut cells = sheet.cells.iter().collect::<Vec<_>>();
        cells.sort_by_key(|c| (c.row, c.column));
        let mut previous = None;
        let mut row = None;
        for c in cells {
            total_text += c.value.len();
            if c.row >= 1_048_576
                || c.column >= 16_384
                || c.value.encode_utf16().count() > 32767
                || total_text > 32 * 1024 * 1024
                || previous == Some((c.row, c.column))
            {
                return Err(DocxError::ResourceLimit(
                    "cell dimensions, duplicate cell or text budget exceeded".into(),
                ));
            }
            previous = Some((c.row, c.column));
            if row != Some(c.row) {
                if row.is_some() {
                    xml.push_str("</row>");
                }
                xml.push_str(&format!(r#"<row r="{}">"#, c.row + 1));
                row = Some(c.row);
            }
            let format = formats
                .get(&(sheet_index, c.row, c.column))
                .copied()
                .unwrap_or_default();
            let kind = classify(&c.value);
            let number_format = match kind {
                CellKind::Text => 49,
                CellKind::Number | CellKind::Bool(_) | CellKind::Date => 0,
            };
            let entry = (format, number_format);
            let style = if let Some(index) = style_entries.iter().position(|item| *item == entry) {
                index
            } else {
                style_entries.push(entry);
                style_entries.len() - 1
            };
            let reference = format!("{}{}", column_name(c.column), c.row + 1);
            match kind {
                CellKind::Text => xml.push_str(&format!(
                    r#"<c r="{reference}" s="{style}" t="inlineStr"><is><t xml:space="preserve">{}</t></is></c>"#,
                    escaped(&c.value)?
                )),
                CellKind::Number => xml.push_str(&format!(
                    r#"<c r="{reference}" s="{style}" t="n"><v>{}</v></c>"#,
                    escaped(&c.value)?
                )),
                CellKind::Bool(value) => xml.push_str(&format!(
                    r#"<c r="{reference}" s="{style}" t="b"><v>{}</v></c>"#,
                    u8::from(value)
                )),
                CellKind::Date => xml.push_str(&format!(
                    r#"<c r="{reference}" s="{style}" t="d"><v>{}</v></c>"#,
                    escaped(&c.value)?
                )),
            }
        }
        if row.is_some() {
            xml.push_str("</row>");
        }
        xml.push_str("</sheetData></worksheet>");
        parts.push((format!("xl/worksheets/sheet{n}.xml"), xml));
    }
    workbook.push_str("</sheets></workbook>");
    rels.push_str(r#"<Relationship Id="styles" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#);
    types.push_str("</Types>");
    let mut style_xml = format!(
        r#"<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="{}">"#,
        font_styles.len()
    );
    for f in &font_styles {
        style_xml.push_str(&format!(
            r#"<font><sz val="{}"/><name val="Calibri"/>{}</font>"#,
            f.size,
            if f.bold { "<b/>" } else { "" }
        ));
    }
    style_xml.push_str(&format!(r#"</fonts><fills count="2"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="{}">"#,style_entries.len()));
    for (format, number_format) in &style_entries {
        let font = font_styles
            .iter()
            .position(|candidate| candidate == format)
            .unwrap_or(0);
        style_xml.push_str(&format!(r#"<xf numFmtId="{number_format}" fontId="{font}" fillId="0" borderId="0" xfId="0" applyFont="1" applyNumberFormat="1"/>"#));
    }
    style_xml.push_str(r#"</cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#);
    parts.extend([
        ("[Content_Types].xml".into(),types),
        ("_rels/.rels".into(),r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.into()),
        ("xl/workbook.xml".into(),workbook), ("xl/_rels/workbook.xml.rels".into(),rels),
        ("xl/styles.xml".into(),style_xml),
    ]);
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xlsx::SpreadsheetCell;
    fn sheets() -> Vec<SpreadsheetModel> {
        vec![
            SpreadsheetModel {
                sheet_name: "Данные & заметки".into(),
                cells: vec![
                    SpreadsheetCell {
                        row: 0,
                        column: 0,
                        value: " Ёжик & <текст> ".into(),
                    },
                    SpreadsheetCell {
                        row: 1,
                        column: 2,
                        value: "".into(),
                    },
                    SpreadsheetCell {
                        row: 4,
                        column: 3,
                        value: "=HYPERLINK(\"https://invalid.test\")".into(),
                    },
                ],
                row_count: 5,
                column_count: 4,
            },
            SpreadsheetModel {
                sheet_name: "Second".into(),
                ..Default::default()
            },
        ]
    }
    #[test]
    fn xlsx_values_fonts_empty_cells_and_empty_sheets_roundtrip_without_active_parts() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.xls");
        std::fs::write(&source, b"original bytes").unwrap();
        let data = sheets();
        let mut editor = WorkbookEditor::new(source.clone(), &data);
        editor.formats.insert(
            (0, 0, 0),
            CellFormat {
                size: 20,
                bold: true,
            },
        );
        let output = dir.path().join("copy.xlsx");
        editor.save(&output, &data).unwrap();
        assert!(!editor.is_dirty(&data));
        let reopened = XlsxPackage::open(&output).unwrap();
        assert_eq!(reopened.sheets, data);
        use calamine::Reader;
        let mut independent = calamine::open_workbook_auto(&output).unwrap();
        assert_eq!(independent.sheet_names(), &["Данные & заметки", "Second"]);
        let range = independent.worksheet_range("Данные & заметки").unwrap();
        assert_eq!(
            range.get_value((4, 3)),
            Some(&calamine::Data::String(
                "=HYPERLINK(\"https://invalid.test\")".into()
            ))
        );
        assert_eq!(
            reopened.formats[&(0, 0, 0)],
            CellFormat {
                size: 20,
                bold: true
            }
        );
        assert_eq!(std::fs::read(&source).unwrap(), b"original bytes");
        let mut archive = zip::ZipArchive::new(std::fs::File::open(output).unwrap()).unwrap();
        use std::io::Read;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            assert!(!entry.name().contains("vba"));
            let mut xml = String::new();
            entry.read_to_string(&mut xml).unwrap();
            assert!(!xml.contains("<f>"));
            assert!(!xml.contains("TargetMode=\"External\""));
        }
    }
    #[test]
    fn typed_numbers_bools_and_dates_export_and_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.xls");
        std::fs::write(&source, b"original").unwrap();
        let data = vec![SpreadsheetModel {
            sheet_name: "Typed".into(),
            cells: vec![
                SpreadsheetCell {
                    row: 0,
                    column: 0,
                    value: "42".into(),
                },
                SpreadsheetCell {
                    row: 0,
                    column: 1,
                    value: "3.50".into(),
                },
                SpreadsheetCell {
                    row: 0,
                    column: 2,
                    value: "TRUE".into(),
                },
                SpreadsheetCell {
                    row: 0,
                    column: 3,
                    value: "FALSE".into(),
                },
                SpreadsheetCell {
                    row: 0,
                    column: 4,
                    value: "2026-10-25".into(),
                },
                SpreadsheetCell {
                    row: 1,
                    column: 0,
                    value: "007".into(),
                },
                SpreadsheetCell {
                    row: 1,
                    column: 1,
                    value: "text".into(),
                },
            ],
            row_count: 2,
            column_count: 5,
        }];
        let mut editor = WorkbookEditor::new(source, &data);
        let output = dir.path().join("typed.xlsx");
        editor.save(&output, &data).unwrap();
        assert_eq!(XlsxPackage::open(&output).unwrap().sheets, data);
        let file = std::fs::File::open(&output).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut xml = String::new();
        std::io::Read::read_to_string(
            &mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(),
            &mut xml,
        )
        .unwrap();
        assert!(xml.contains(r#"t="n"><v>42</v>"#), "{xml}");
        assert!(xml.contains(r#"t="n"><v>3.50</v>"#), "{xml}");
        assert!(xml.contains(r#"t="b"><v>1</v>"#), "{xml}");
        assert!(xml.contains(r#"t="b"><v>0</v>"#), "{xml}");
        assert!(xml.contains(r#"t="d"><v>2026-10-25</v>"#), "{xml}");
        assert!(
            xml.contains(r#"t="inlineStr"><is><t xml:space="preserve">007</t>"#),
            "{xml}"
        );
    }

    #[test]
    fn original_alias_conflicts_failures_and_backups_keep_workbook_data() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("original.xlsx");
        std::fs::write(&origin, b"protected original").unwrap();
        let data = sheets();
        let mut editor = WorkbookEditor::new(origin.clone(), &data);
        assert!(editor.save(&origin, &data).is_err());
        let alias = dir.path().join("alias.xlsx");
        std::fs::hard_link(&origin, &alias).unwrap();
        assert!(editor.save(&alias, &data).is_err());
        assert!(editor
            .save(&dir.path().join("missing/output.xlsx"), &data)
            .is_err());
        assert!(editor.is_dirty(&data));
        let output = dir.path().join("copy.xlsx");
        editor.save(&output, &data).unwrap();
        let before = std::fs::read(&output).unwrap();
        let mut edited = data.clone();
        edited[0].cells[0].value = "new text".into();
        editor.save(&output, &edited).unwrap();
        assert_eq!(
            std::fs::read(output.with_extension("xlsx.bak")).unwrap(),
            before
        );
        std::fs::write(&output, b"changed elsewhere").unwrap();
        assert!(matches!(
            editor.save(&output, &data),
            Err(DocxError::SourceChanged)
        ));
        assert_eq!(std::fs::read(&output).unwrap(), b"changed elsewhere");
        assert!(editor.is_dirty(&data));
        let invalid = vec![SpreadsheetModel {
            sheet_name: "Bad/Name".into(),
            ..Default::default()
        }];
        assert!(editor.save(&dir.path().join("bad.xlsx"), &invalid).is_err());
        assert_eq!(std::fs::read(origin).unwrap(), b"protected original");
    }
}
