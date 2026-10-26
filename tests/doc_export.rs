use liteoffice::doc::{LegacyDocImage, LegacyDocPackage, LegacyImageFormat};
use liteoffice::docx::DocxPackage;
use std::fs::{self, File};
use std::io::{Cursor, Read, Write};
use std::path::Path;

fn fixture(path: &Path) {
    let text = "Кот & <Ёжик>\rВторая строка\r";
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
    let mut compound = cfb::CompoundFile::create(File::create(path).unwrap()).unwrap();
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
    compound.create_storage("/Macros").unwrap();
    compound
        .create_stream("/Macros/INERT_TEST")
        .unwrap()
        .write_all(b"DO NOT COPY EXECUTABLE STREAMS")
        .unwrap();
    compound.flush().unwrap();
}

#[test]
fn imported_doc_edits_and_font_size_roundtrip_without_overwriting_or_copying_macros() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("исходник.doc");
    let output = folder.path().join("результат.docx");
    fixture(&source);
    let original = fs::read(&source).unwrap();
    let legacy = LegacyDocPackage::open(&source).unwrap();
    let mut package = DocxPackage::from_legacy(&source, &legacy, &[], &[]).unwrap();
    assert!(package.requires_save_as());
    assert_eq!(package.model.fragment(0).unwrap().text, "Кот & <Ёжик>");
    package.model.fragment_mut(0).unwrap().text = " Изменено & <сохранено> ".into();
    assert!(package.model.set_all_run_font_sizes(36));
    assert!(package.save_as(&source).is_err());
    let alias = folder.path().join("alias.docx");
    fs::hard_link(&source, &alias).unwrap();
    assert!(package.save_as(&alias).is_err());
    assert_eq!(fs::read(&source).unwrap(), original);
    package.save(&output).unwrap();
    assert!(!package.requires_save_as());
    assert_eq!(package.source_path(), output);
    let reopened = DocxPackage::open(&output).unwrap();
    assert_eq!(reopened.model, package.model);
    assert_eq!(reopened.model.common_font_size(), Some(36));
    assert_eq!(fs::read(&source).unwrap(), original);
    let mut archive = zip::ZipArchive::new(File::open(&output).unwrap()).unwrap();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        assert!(!entry.name().to_lowercase().contains("macro"));
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("DO NOT COPY"));
    }
}

#[test]
fn conversion_keeps_anchored_and_unplaced_images_and_does_not_silently_drop_bad_rasters() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("images.doc");
    fixture(&source);
    let mut legacy = LegacyDocPackage::open(&source).unwrap();
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(32, 16)
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
    legacy.images = vec![
        LegacyDocImage {
            bytes: png.into_inner(),
            format: LegacyImageFormat::Png
        };
        2
    ];
    let package =
        DocxPackage::from_legacy(&source, &legacy, &[Some(0), None], &[50.0, 85.0]).unwrap();
    assert_eq!(package.images.len(), 2);
    assert_eq!(package.images[0].paragraph_index, 1);
    assert_eq!(package.images[1].paragraph_index, 3);
    for image in &package.images {
        assert_eq!(image.bytes, legacy.images[0].bytes);
        let (w, h) = image.extent_emu.unwrap();
        assert_eq!(w, h * 2);
    }
    let output = folder.path().join("images.docx");
    package.save_as(&output).unwrap();
    assert_eq!(DocxPackage::open(&output).unwrap().images, package.images);
    legacy.images[0].bytes = b"invalid raster".to_vec();
    assert!(DocxPackage::from_legacy(&source, &legacy, &[], &[]).is_err());
}

#[test]
fn converted_unsaved_edits_can_be_checkpointed_and_restored_without_touching_doc() {
    let folder = tempfile::tempdir().unwrap();
    let source = folder.path().join("recover.doc");
    fixture(&source);
    let original = fs::read(&source).unwrap();
    let legacy = LegacyDocPackage::open(&source).unwrap();
    let mut package = DocxPackage::from_legacy(&source, &legacy, &[], &[]).unwrap();
    package.model.fragment_mut(0).unwrap().text = "Несохранённая копия".into();
    let mut session =
        liteoffice::recovery::Session::create(&folder.path().join("recovery"), &source).unwrap();
    session.checkpoint(&package).unwrap();
    assert_eq!(session.restore().unwrap().0.model, package.model);
    assert_eq!(fs::read(&source).unwrap(), original);
}
