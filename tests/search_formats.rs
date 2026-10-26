//! Synthetic importer-to-search tests; no user documents and no print/PDF work.
use liteoffice::{
    doc::LegacyDocPackage, docx::DocxPackage, search, xls::XlsPackage, xlsx::XlsxPackage,
};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

fn new_file(path: &Path) -> File {
    OpenOptions::new()
        .write(true)
        .read(true)
        .create_new(true)
        .open(path)
        .unwrap()
}

fn zip_parts(path: &Path, parts: &[(&str, &str)]) {
    let mut zip = zip::ZipWriter::new(new_file(path));
    for (name, text) in parts {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().sync_all().unwrap();
}

fn docx(path: &Path) {
    zip_parts(
        path,
        &[
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
            ),
            (
                "word/document.xml",
                r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Ё</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>жик ёжик</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>ЁЖИК</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#,
            ),
        ],
    );
}

fn xlsx(path: &Path) {
    zip_parts(
        path,
        &[
            (
                "[Content_Types].xml",
                r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#,
            ),
            (
                "_rels/.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="doc" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
            ),
            (
                "xl/workbook.xml",
                r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Main" sheetId="1" r:id="s1"/><sheet name="Notes" sheetId="2" r:id="s2"/></sheets></workbook>"#,
            ),
            (
                "xl/_rels/workbook.xml.rels",
                r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s1" Target="worksheets/sheet1.xml" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet"/><Relationship Id="s2" Target="worksheets/sheet2.xml" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Ёжик</t></is></c><c r="B1"><v>142</v></c></row></sheetData></worksheet>"#,
            ),
            (
                "xl/worksheets/sheet2.xml",
                r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData><row r="500"><c r="AD500" t="inlineStr"><is><t>ЁЖИК ёжик</t></is></c></row></sheetData></worksheet>"#,
            ),
        ],
    );
}

// Minimal synthetic main-story CLX/CFB fixture, not a Word fidelity sample.
fn doc(path: &Path) {
    doc_text(path, "Ёжик ёжик\rЁЖИК\r");
}

fn doc_text(path: &Path, text: &str) {
    let length = text.encode_utf16().count() as u32;
    let mut word = vec![0_u8; 1024];
    word[0..2].copy_from_slice(&0xA5EC_u16.to_le_bytes());
    word[2..4].copy_from_slice(&0x00C1_u16.to_le_bytes());
    word[0x20..0x22].copy_from_slice(&14_u16.to_le_bytes());
    word[0x3e..0x40].copy_from_slice(&22_u16.to_le_bytes());
    word[0x4c..0x50].copy_from_slice(&length.to_le_bytes());
    word[0x98..0x9a].copy_from_slice(&93_u16.to_le_bytes());
    word[0x1a6..0x1aa].copy_from_slice(&21_u32.to_le_bytes());
    let utf16 = text
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    word[512..512 + utf16.len()].copy_from_slice(&utf16);
    let mut table = vec![2];
    table.extend(16_u32.to_le_bytes());
    table.extend(0_u32.to_le_bytes());
    table.extend(length.to_le_bytes());
    table.extend([0; 2]);
    table.extend(512_u32.to_le_bytes());
    table.extend([0; 2]);
    let mut compound = cfb::CompoundFile::create(new_file(path)).unwrap();
    compound
        .create_stream("/WordDocument")
        .unwrap()
        .write_all(&word)
        .unwrap();
    compound
        .create_stream("/0Table")
        .unwrap()
        .write_all(&table)
        .unwrap();
    compound.flush().unwrap();
}

fn record(output: &mut Vec<u8>, kind: u16, data: &[u8]) {
    output.extend(kind.to_le_bytes());
    output.extend((data.len() as u16).to_le_bytes());
    output.extend(data);
}

fn bof(output: &mut Vec<u8>, kind: u16) {
    let mut data = Vec::new();
    for value in [0x0600_u16, kind, 0x0DBB, 1996] {
        data.extend(value.to_le_bytes());
    }
    data.extend(0x41_u32.to_le_bytes());
    data.extend(6_u32.to_le_bytes());
    record(output, 0x0809, &data);
}

fn xls(path: &Path) {
    let mut sheets = Vec::new();
    for (row, column, string) in [(0_u16, 0_u16, 0_u32), (499, 29, 1)] {
        let mut data = Vec::new();
        bof(&mut data, 0x0010);
        let mut dimensions = Vec::new();
        dimensions.extend(0_u32.to_le_bytes());
        dimensions.extend((u32::from(row) + 1).to_le_bytes());
        dimensions.extend(0_u16.to_le_bytes());
        dimensions.extend((column + 1).to_le_bytes());
        dimensions.extend(0_u16.to_le_bytes());
        record(&mut data, 0x0200, &dimensions);
        let mut cell = Vec::new();
        cell.extend(row.to_le_bytes());
        cell.extend(column.to_le_bytes());
        cell.extend(0_u16.to_le_bytes());
        cell.extend(string.to_le_bytes());
        record(&mut data, 0x00FD, &cell);
        if row == 0 {
            // FORMULA 1+1 has a deliberately different cached value (142).
            // The viewer must read the cache, not evaluate/recalculate tokens.
            let mut formula = Vec::new();
            formula.extend(0_u16.to_le_bytes());
            formula.extend(1_u16.to_le_bytes());
            formula.extend(0_u16.to_le_bytes());
            formula.extend(142_f64.to_le_bytes());
            formula.extend([0; 6]);
            formula.extend(7_u16.to_le_bytes());
            formula.extend([0x1E, 1, 0, 0x1E, 1, 0, 0x03]);
            record(&mut data, 0x0006, &formula);
        }
        record(&mut data, 0x000A, &[]);
        sheets.push(data);
    }
    let mut global = Vec::new();
    bof(&mut global, 0x0005);
    record(&mut global, 0x0042, &1200_u16.to_le_bytes());
    let mut sst = Vec::new();
    sst.extend(2_u32.to_le_bytes());
    sst.extend(2_u32.to_le_bytes());
    for text in ["Ёжик", "ЁЖИК ёжик"] {
        sst.extend((text.encode_utf16().count() as u16).to_le_bytes());
        sst.push(1);
        sst.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
    }
    record(&mut global, 0x00FC, &sst);
    let mut offset_fields = Vec::new();
    for name in ["Main", "Notes"] {
        offset_fields.push(global.len() + 4);
        let mut data = vec![0; 6];
        data.push(name.len() as u8);
        data.push(0);
        data.extend(name.as_bytes());
        record(&mut global, 0x0085, &data);
    }
    record(&mut global, 0x000A, &[]);
    let mut offset = global.len();
    for (field, sheet) in offset_fields.into_iter().zip(&sheets) {
        global[field..field + 4].copy_from_slice(&(offset as u32).to_le_bytes());
        offset += sheet.len();
    }
    for sheet in sheets {
        global.extend(sheet);
    }
    let mut compound = cfb::CompoundFile::create(new_file(path)).unwrap();
    compound
        .create_stream("/Workbook")
        .unwrap()
        .write_all(&global)
        .unwrap();
    compound.flush().unwrap();
}

#[test]
fn legacy_and_modern_workbook_values_export_as_safe_xlsx_copies() {
    let dir = tempfile::tempdir().unwrap();
    for extension in ["xls", "xlsx"] {
        let source = dir.path().join(format!("source.{extension}"));
        let mut sheets = if extension == "xls" {
            xls(&source);
            XlsPackage::open(&source).unwrap().sheets
        } else {
            xlsx(&source);
            XlsxPackage::open(&source).unwrap().sheets
        };
        let original = std::fs::read(&source).unwrap();
        let mut editor = liteoffice::workbook_edit::WorkbookEditor::new(source.clone(), &sheets);
        sheets[0].cells[0].value = "Изменено & сохранено".into();
        let output = dir.path().join(format!("from-{extension}.xlsx"));
        editor.save(&output, &sheets).unwrap();
        let result = XlsxPackage::open(output).unwrap();
        assert_eq!(result.sheets, sheets);
        assert_eq!(std::fs::read(source).unwrap(), original);
    }
}

#[test]
#[ignore = "explicit synthetic workbook fixture export; no printing or recovery regression"]
fn export_manual_workbook_fixtures() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("manual_phase4_artifacts/workbook-rc9");
    std::fs::create_dir(&folder).unwrap();
    xls(&folder.join("workbook-test.xls"));
    xlsx(&folder.join("workbook-test.xlsx"));
    println!("Created {}", folder.display());
}

fn create_and_verify(folder: &Path) {
    let paths =
        ["search.docx", "search.doc", "search.xlsx", "search.xls"].map(|name| folder.join(name));
    docx(&paths[0]);
    doc(&paths[1]);
    xlsx(&paths[2]);
    xls(&paths[3]);
    let original = paths
        .iter()
        .map(|path| fs::read(path).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        search::docx(&DocxPackage::open(&paths[0]).unwrap().model, "ёжик")
            .hits
            .len(),
        3
    );
    assert_eq!(
        search::legacy(
            &LegacyDocPackage::open(&paths[1]).unwrap().paragraphs,
            "ЁЖИК"
        )
        .hits
        .len(),
        3
    );
    let xlsx = XlsxPackage::open(&paths[2]).unwrap();
    let xls = XlsPackage::open(&paths[3]).unwrap();
    assert_eq!(
        xls.sheets[0].cell_value(0, 1),
        "142",
        "XLS formula must use its cache, not execute/recalculate"
    );
    for sheets in [&xlsx.sheets, &xls.sheets] {
        let result = search::sheets(sheets, "ёжик");
        assert_eq!(result.hits.len(), 3);
        assert_eq!(
            result.hits[1].target,
            search::Target::Cell {
                sheet: 1,
                row: 499,
                column: 29
            }
        );
    }
    for (path, bytes) in paths.iter().zip(original) {
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
}

#[test]
fn all_four_importers_feed_the_same_read_only_find_contract() {
    let folder = tempfile::tempdir().unwrap();
    create_and_verify(folder.path());
}

#[test]
fn docx_replace_save_reopen_keeps_empty_runs_styles_and_all_other_package_parts() {
    use liteoffice::replace::{self, Selection};
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("source.docx");
    docx(&source);
    let original = fs::read(&source).unwrap();
    for (index, replacement) in ["Кот", "", " <&> "].into_iter().enumerate() {
        let mut package = DocxPackage::open(&source).unwrap();
        let edit = replace::docx(&package.model, "ёжик", replacement, Selection::All).unwrap();
        assert_eq!(edit.count, 3);
        package.model = edit.model;
        let output = folder.path().join(format!("replaced-{index}.docx"));
        package.save_as(&output).unwrap();
        assert_eq!(DocxPackage::open(&output).unwrap().model, package.model);
        assert!(search::docx(&package.model, "ёжик").hits.is_empty());
        let mut before = zip::ZipArchive::new(std::io::Cursor::new(&original)).unwrap();
        let mut after = zip::ZipArchive::new(fs::File::open(output).unwrap()).unwrap();
        assert_eq!(before.len(), after.len());
        for part in 0..before.len() {
            use std::io::Read;
            let mut input = before.by_index(part).unwrap();
            let mut output = after.by_name(input.name()).unwrap();
            let mut original_bytes = Vec::new();
            let mut saved_bytes = Vec::new();
            input.read_to_end(&mut original_bytes).unwrap();
            output.read_to_end(&mut saved_bytes).unwrap();
            if input.name() == "word/document.xml" {
                assert!(String::from_utf8(saved_bytes).unwrap().contains("<w:b/>"));
            } else {
                assert_eq!(original_bytes, saved_bytes, "part {} changed", input.name());
            }
        }
    }
    assert_eq!(fs::read(source).unwrap(), original);
}

#[test]
#[ignore = "explicit export of synthetic replacement fixtures; no print or recovery regression"]
fn export_manual_replace_fixtures() {
    use liteoffice::replace::{self, Selection};
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("manual_phase4_artifacts/replace-rc5");
    fs::create_dir(&folder).expect("new directory required; never overwrite user files");
    let source = folder.join("replace.docx");
    docx(&source);
    for (name, replacement) in [("expected-cats.docx", "Кот"), ("expected-deleted.docx", "")] {
        let mut package = DocxPackage::open(&source).unwrap();
        package.model = replace::docx(&package.model, "ёжик", replacement, Selection::All)
            .unwrap()
            .model;
        package.save_as(folder.join(name)).unwrap();
    }
    println!("REPLACE_FIXTURES={}", folder.display());
}

#[test]
fn office_macros_embedded_scripts_and_field_instructions_are_not_executed() {
    let folder = tempfile::tempdir().unwrap();
    let marker = folder.path().join("SHOULD_NOT_EXIST.txt");
    let payload = format!("Sub AutoOpen(): Shell \"cmd.exe /c echo BAD > {}\": End Sub\nfunction startup() {{ throw 'document script executed'; }}", marker.display());
    let docx_path = folder.path().join("active.docx");
    docx(&docx_path);
    {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&docx_path)
            .unwrap();
        let mut zip = zip::ZipWriter::new_append(file).unwrap();
        for name in [
            "word/vbaProject.bin",
            "word/embeddings/payload.js",
            "word/embeddings/command.cmd",
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(payload.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    let package = DocxPackage::open(&docx_path).unwrap();
    assert_eq!(search::docx(&package.model, "ёжик").hits.len(), 3);
    assert!(!marker.exists());
    package.save_as(folder.path().join("copy.docx")).unwrap();
    assert!(!marker.exists());
    for (name, spreadsheet) in [("active.doc", false), ("active.xls", true)] {
        let path = folder.path().join(name);
        if spreadsheet {
            xls(&path);
        } else {
            doc_text(&path, &format!("Safe text\r\u{0013} DDEAUTO cmd.exe /c echo BAD > {}\u{0014}Cached field text\u{0015}\r", marker.display()));
        }
        {
            let mut compound = cfb::open_rw(&path).unwrap();
            compound.create_storage_all("/Macros/VBA").unwrap();
            compound
                .create_stream("/Macros/VBA/AutoOpen")
                .unwrap()
                .write_all(payload.as_bytes())
                .unwrap();
            compound.flush().unwrap();
        }
        if spreadsheet {
            assert_eq!(XlsPackage::open(&path).unwrap().sheets.len(), 2);
        } else {
            let package = LegacyDocPackage::open(&path).unwrap();
            assert!(package
                .paragraphs
                .iter()
                .any(|text| text.contains("Cached field text")));
            assert!(!package.paragraphs.join("\n").contains("DDEAUTO"));
        }
        assert!(!marker.exists());
    }
    let xlsx_path = folder.path().join("active.xlsx");
    xlsx(&xlsx_path);
    {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&xlsx_path)
            .unwrap();
        let mut zip = zip::ZipWriter::new_append(file).unwrap();
        zip.start_file(
            "xl/vbaProject.bin",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(payload.as_bytes()).unwrap();
        zip.start_file(
            "xl/embeddings/script.js",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(payload.as_bytes()).unwrap();
        zip.finish().unwrap();
    }
    assert_eq!(XlsxPackage::open(&xlsx_path).unwrap().sheets.len(), 2);
    assert!(!marker.exists());
}

#[test]
#[ignore = "explicit export of new synthetic manual Find fixtures; no PDF or recovery regression"]
fn export_manual_find_fixtures() {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("manual_phase4_artifacts/search-rc3");
    fs::create_dir(&folder).expect("must be a new directory; never overwrite manual test files");
    create_and_verify(&folder);
    println!("FIND_FIXTURES={}", folder.display());
}
