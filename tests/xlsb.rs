use calamine::{open_workbook_auto, Reader, SheetType};
use liteoffice::xls::XlsPackage;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/xlsb")
        .join(name)
}

#[test]
fn mixed_xlsb_opens_all_worksheets_without_reading_chart_or_macro_sheets() {
    let path = fixture("any_sheets.xlsb");
    let reader = open_workbook_auto(&path).unwrap();
    let expected: Vec<_> = reader
        .sheets_metadata()
        .iter()
        .filter(|sheet| sheet.typ == SheetType::WorkSheet)
        .map(|sheet| sheet.name.clone())
        .collect();
    assert!(!expected.is_empty());
    let package = XlsPackage::open(&path).unwrap();
    assert_eq!(
        package
            .sheets
            .iter()
            .map(|sheet| sheet.sheet_name.clone())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(expected, ["Visible", "Hidden", "VeryHidden"]);
    assert_eq!(package.model.cell_value(0, 0), "1");
    assert_eq!(package.model.cell_value(2, 1), "6");
    assert!(package.sheets[1].cells.is_empty());
    assert!(package.sheets[2].cells.is_empty());
}

#[test]
fn real_xlsb_dates_display_iso_instead_of_serial_numbers() {
    let path = fixture("date.xlsb");
    let package = XlsPackage::open(path).unwrap();
    assert_eq!(package.model.cell_value(0, 0), "2021-01-01");
    assert_eq!(package.model.cell_value(1, 0), "2021-01-02");
    assert_eq!(package.model.cell_value(2, 0), "255:10:10");
    assert_eq!(package.model.cell_value(0, 1), "15");
}

#[test]
fn real_xlsb_strings_cyrillic_bools_and_numbers_stay_unchanged() {
    for (name, text) in [("issues.xlsb", "test"), ("issues-cyrillic.xlsb", "тест")] {
        let path = fixture(name);
        let before = std::fs::read(&path).unwrap();
        let package = XlsPackage::open(&path).unwrap();
        let values: Vec<_> = package
            .model
            .cells
            .iter()
            .map(|cell| cell.value.as_str())
            .collect();
        for expected in ["1", "1.5", "ab", "false", text, "2016-10-20"] {
            assert!(values.contains(&expected), "{name}: {values:?}");
        }
        assert_eq!(std::fs::read(path).unwrap(), before);
    }
}

#[test]
fn damaged_real_worksheet_is_reported_instead_of_silently_skipped() {
    use std::io::{Read, Write};
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("damaged.xlsb");
    let file = std::fs::File::open(fixture("any_sheets.xlsb")).unwrap();
    let mut source = zip::ZipArchive::new(file).unwrap();
    let mut target = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    for index in 0..source.len() {
        let mut entry = source.by_index(index).unwrap();
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == "xl/worksheets/sheet1.bin" {
            bytes.truncate(1);
        }
        target
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        target.write_all(&bytes).unwrap();
    }
    target.finish().unwrap();
    assert!(XlsPackage::open(path).is_err());
}
