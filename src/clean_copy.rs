//! Build a fresh passive-only DOCX/XLSX copy from the parsed model.
//!
//! Nothing from the source package is copied wholesale: only generated XML,
//! values, tables and re-encoded raster images are written. The original file
//! is fingerprinted before and after the operation and is never a destination.

use crate::docx::{
    create_temporary_peer, paths_refer_to_same_file, replace_destination, source_fingerprint,
    DocxError, DocxPackage,
};
use crate::docx::{DocumentBlock, DocumentModel, DocxImage, Paragraph, ParagraphAlignment};
use crate::workbook_edit::{CellFormat, CellKey, WorkbookEditor};
use crate::xlsx::SpreadsheetModel;
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

const MAX_IMAGE_INPUT_BYTES: usize = 16 * 1024 * 1024;
const MAX_IMAGE_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u32 = 32_000_000;
const MAX_IMAGE_SIDE: u32 = 8_192;
const MAX_TOTAL_IMAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_DOCUMENT_XML_BYTES: usize = 48 * 1024 * 1024;

type CleanParts = Vec<(String, Vec<u8>)>;
type CleanBuild = (CleanParts, CleanCopyReport);

#[derive(Debug, Clone, Default)]
pub struct CleanCopyReport {
    pub paragraphs: usize,
    pub tables: usize,
    pub cells: usize,
    pub images: usize,
    pub images_skipped: usize,
    pub sheets: usize,
    pub values: usize,
}

impl CleanCopyReport {
    pub fn summary(&self) -> String {
        if self.sheets > 0 {
            format!(
                "XLSX: листов {}, значений {}; формулы, внешние связи и активные части не переносились",
                self.sheets, self.values
            )
        } else {
            format!(
                "DOCX: абзацев {}, таблиц {}, ячеек {}, изображений {} (пропущено {}); макросы, OLE/ActiveX, поля, внешние связи и часть оформления не переносились",
                self.paragraphs,
                self.tables,
                self.cells,
                self.images,
                self.images_skipped
            )
        }
    }
}

/// Write a clean DOCX from the already parsed package model.
pub fn clean_docx(
    source: &Path,
    package: &DocxPackage,
    destination: &Path,
) -> Result<CleanCopyReport, DocxError> {
    if !destination
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"))
    {
        return Err(DocxError::UnsafeSave(
            "choose a .docx destination for the clean copy".into(),
        ));
    }
    if paths_refer_to_same_file(source, destination)? {
        return Err(DocxError::UnsafeSave(
            "the clean copy cannot overwrite its source".into(),
        ));
    }

    let before = source_fingerprint(source)?;
    let (parts, report) = build_clean_parts(&package.model, &package.images)?;
    let total = parts
        .iter()
        .map(|(name, bytes)| name.len() + bytes.len())
        .sum::<usize>();
    if total > 96 * 1024 * 1024 {
        return Err(DocxError::ResourceLimit(
            "clean DOCX package exceeds 96 MiB".into(),
        ));
    }

    let (temp, file) = create_temporary_peer(destination)?;
    let result = (|| {
        let mut zip = ZipWriter::new(file);
        for (name, bytes) in &parts {
            zip.start_file(name.as_str(), SimpleFileOptions::default())?;
            zip.write_all(bytes)?;
        }
        let mut file = zip.finish()?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        validate_clean_package(&temp, &package.model, report.images)?;
        if source_fingerprint(source)? != before {
            return Err(DocxError::SourceChanged);
        }
        replace_destination(&temp, destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result?;
    Ok(report)
}

/// Write a values-only XLSX through the existing protected workbook writer.
/// The writer already emits only workbook/styles/worksheet XML and never copies
/// external links, formulas or binary parts from the source.
pub fn clean_workbook(
    source: &Path,
    sheets: &[SpreadsheetModel],
    formats: &BTreeMap<CellKey, CellFormat>,
    destination: &Path,
) -> Result<CleanCopyReport, DocxError> {
    if !destination
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xlsx"))
    {
        return Err(DocxError::UnsafeSave(
            "choose an .xlsx destination for the clean copy".into(),
        ));
    }
    if paths_refer_to_same_file(source, destination)? {
        return Err(DocxError::UnsafeSave(
            "the clean copy cannot overwrite its source".into(),
        ));
    }
    let before = source_fingerprint(source)?;
    let mut editor = WorkbookEditor::new(source.to_path_buf(), sheets);
    editor.formats = formats.clone();
    editor.save(destination, sheets)?;
    if source_fingerprint(source)? != before {
        return Err(DocxError::SourceChanged);
    }
    Ok(CleanCopyReport {
        sheets: sheets.len(),
        values: sheets.iter().map(|sheet| sheet.cells.len()).sum(),
        ..Default::default()
    })
}

fn validate_clean_package(
    path: &Path,
    expected: &DocumentModel,
    expected_images: usize,
) -> Result<(), DocxError> {
    let reopened = DocxPackage::open(path)?;
    if reopened.model.paragraphs.len() != expected.paragraphs.len()
        || reopened
            .model
            .paragraphs
            .iter()
            .zip(&expected.paragraphs)
            .any(|(a, b)| texts(a) != texts(b))
    {
        return Err(DocxError::UnsafeSave(
            "clean DOCX paragraph text did not round-trip".into(),
        ));
    }
    if reopened.images.len() != expected_images {
        return Err(DocxError::UnsafeSave(
            "clean DOCX image count did not round-trip".into(),
        ));
    }
    let file = std::fs::File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let allowed = |name: &str| {
        matches!(
            name,
            "[Content_Types].xml"
                | "_rels/.rels"
                | "word/document.xml"
                | "word/_rels/document.xml.rels"
        ) || (name.starts_with("word/media/image") && name.ends_with(".png"))
    };
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name().to_string();
        if !allowed(&name) {
            return Err(DocxError::UnsafeSave(format!(
                "clean DOCX contains an unexpected part: {name}"
            )));
        }
    }
    if archive.by_name("word/document.xml").is_err() {
        return Err(DocxError::UnsafeSave(
            "clean DOCX has no word/document.xml".into(),
        ));
    }
    if let Ok(mut rels) = archive.by_name("word/_rels/document.xml.rels") {
        let mut xml = String::new();
        rels.read_to_string(&mut xml)?;
        if xml.contains("TargetMode=\"External\"")
            || xml.contains("TargetMode='External'")
            || xml.contains("Target=\"http://")
            || xml.contains("Target='http://")
            || xml.contains("Target=\"https://")
            || xml.contains("Target='https://")
        {
            return Err(DocxError::UnsafeSave(
                "clean DOCX contains an external relationship".into(),
            ));
        }
    }
    Ok(())
}

fn texts(paragraph: &Paragraph) -> String {
    paragraph
        .fragments
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect()
}

fn build_clean_parts(model: &DocumentModel, images: &[DocxImage]) -> Result<CleanBuild, DocxError> {
    let mut report = CleanCopyReport::default();
    let mut clean_images = Vec::new();
    let mut total_image_bytes = 0usize;
    for image in images {
        match sanitize_image(&image.bytes) {
            Some((bytes, width, height))
                if total_image_bytes.saturating_add(bytes.len()) <= MAX_TOTAL_IMAGE_BYTES =>
            {
                total_image_bytes += bytes.len();
                clean_images.push(CleanImage {
                    bytes,
                    cx: (u64::from(width) * 9525).max(1),
                    cy: (u64::from(height) * 9525).max(1),
                    r_id: format!("rIdCleanImage{}", clean_images.len() + 1),
                    paragraph_index: image.paragraph_index,
                });
            }
            _ => report.images_skipped += 1,
        }
    }
    report.images = clean_images.len();

    let mut by_paragraph: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (index, image) in clean_images.iter().enumerate() {
        by_paragraph
            .entry(image.paragraph_index)
            .or_default()
            .push(index);
    }
    let mut placed = vec![false; clean_images.len()];
    let mut body = String::new();
    for block in &model.blocks {
        match block {
            DocumentBlock::Paragraph(index) => {
                if let Some(paragraph) = model.paragraphs.get(*index) {
                    body.push_str(&paragraph_xml(
                        *index,
                        paragraph,
                        &by_paragraph,
                        &clean_images,
                        &mut placed,
                    )?);
                    report.paragraphs += 1;
                }
            }
            DocumentBlock::Table { rows, grid_widths } => {
                report.tables += 1;
                body.push_str("<w:tbl><w:tblPr><w:tblW w:w=\"0\" w:type=\"auto\"/></w:tblPr>");
                if !grid_widths.is_empty() {
                    body.push_str("<w:tblGrid>");
                    for width in grid_widths {
                        body.push_str(&format!("<w:gridCol w:w=\"{width}\"/>"));
                    }
                    body.push_str("</w:tblGrid>");
                }
                for row in rows {
                    body.push_str("<w:tr>");
                    for cell in &row.cells {
                        report.cells += 1;
                        body.push_str("<w:tc><w:tcPr><w:tcW w:w=\"0\" w:type=\"auto\"/></w:tcPr>");
                        if cell.is_empty() {
                            body.push_str("<w:p/>");
                        } else {
                            for index in cell {
                                if let Some(paragraph) = model.paragraphs.get(*index) {
                                    body.push_str(&paragraph_xml(
                                        *index,
                                        paragraph,
                                        &by_paragraph,
                                        &clean_images,
                                        &mut placed,
                                    )?);
                                    report.paragraphs += 1;
                                }
                            }
                        }
                        body.push_str("</w:tc>");
                    }
                    body.push_str("</w:tr>");
                }
                body.push_str("</w:tbl>");
            }
        }
    }
    // Images whose anchor paragraph could not be represented are still kept as
    // a trailing gallery paragraph; they are never silently dropped.
    for (index, image) in clean_images.iter().enumerate() {
        if !placed[index] {
            body.push_str("<w:p><w:r>");
            body.push_str(&drawing_xml(index, image));
            body.push_str("</w:r></w:p>");
        }
    }
    if body.is_empty() {
        body.push_str("<w:p/>");
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" \
         xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" \
         xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" \
         xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" \
         xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/>\
         <w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" \
         w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    if document.len() > MAX_DOCUMENT_XML_BYTES {
        return Err(DocxError::ResourceLimit(
            "clean DOCX document.xml exceeds 48 MiB".into(),
        ));
    }

    let mut content_types = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
         <Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
         <Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
         <Default Extension=\"xml\" ContentType=\"application/xml\"/>\
         <Default Extension=\"png\" ContentType=\"image/png\"/>\
         <Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
         </Types>",
    );
    if clean_images.is_empty() {
        // The default PNG declaration is harmless but unnecessary without images.
        content_types =
            content_types.replace("<Default Extension=\"png\" ContentType=\"image/png\"/>", "");
    }
    let mut parts = vec![
        ("[Content_Types].xml".to_string(), content_types.into_bytes()),
        (
            "_rels/.rels".to_string(),
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
             <Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/>\
             </Relationships>"
                .as_bytes()
                .to_vec(),
        ),
        ("word/document.xml".to_string(), document.into_bytes()),
    ];
    if !clean_images.is_empty() {
        let mut rels = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
             <Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">",
        );
        for (index, image) in clean_images.iter().enumerate() {
            rels.push_str(&format!(
                "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/image{}.png\"/>",
                image.r_id,
                index + 1
            ));
            parts.push((
                format!("word/media/image{}.png", index + 1),
                image.bytes.clone(),
            ));
        }
        rels.push_str("</Relationships>");
        parts.push((
            "word/_rels/document.xml.rels".to_string(),
            rels.into_bytes(),
        ));
    }
    Ok((parts, report))
}

struct CleanImage {
    bytes: Vec<u8>,
    cx: u64,
    cy: u64,
    r_id: String,
    paragraph_index: usize,
}

fn paragraph_xml(
    paragraph_index: usize,
    paragraph: &Paragraph,
    by_paragraph: &BTreeMap<usize, Vec<usize>>,
    images: &[CleanImage],
    placed: &mut [bool],
) -> Result<String, DocxError> {
    let mut xml = String::from("<w:p>");
    if paragraph.alignment == ParagraphAlignment::Center {
        xml.push_str("<w:pPr><w:jc w:val=\"center\"/></w:pPr>");
    }
    for fragment in &paragraph.fragments {
        let text = xml_escape(&fragment.text)?;
        if text.is_empty() && fragment.font_size_half_points.is_none() {
            continue;
        }
        xml.push_str("<w:r>");
        if let Some(size) = fragment.font_size_half_points {
            xml.push_str(&format!(
                "<w:rPr><w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/></w:rPr>"
            ));
        }
        xml.push_str("<w:t xml:space=\"preserve\">");
        xml.push_str(&text);
        xml.push_str("</w:t></w:r>");
    }
    if paragraph.page_break_after {
        xml.push_str("<w:r><w:br w:type=\"page\"/></w:r>");
    }
    if let Some(indexes) = by_paragraph.get(&paragraph_index) {
        for index in indexes {
            if let Some(image) = images.get(*index) {
                placed[*index] = true;
                xml.push_str("<w:r>");
                xml.push_str(&drawing_xml(*index, image));
                xml.push_str("</w:r>");
            }
        }
    }
    xml.push_str("</w:p>");
    Ok(xml)
}

fn drawing_xml(index: usize, image: &CleanImage) -> String {
    let cx = image.cx.max(1);
    let cy = image.cy.max(1);
    format!(
        "<w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\">\
         <wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\
         <wp:docPr id=\"{id}\" name=\"Picture {id}\"/>\
         <a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\
         <pic:pic><pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"image{id}.png\"/><pic:cNvPicPr/></pic:nvPicPr>\
         <pic:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\
         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm>\
         <a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>\
         </a:graphicData></a:graphic></wp:inline></w:drawing>",
        id = index + 1,
        rid = image.r_id
    )
}

fn sanitize_image(bytes: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    if bytes.is_empty() || bytes.len() > MAX_IMAGE_INPUT_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let format = reader.format()?;
    if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    if decoded.width() == 0
        || decoded.height() == 0
        || u64::from(decoded.width()) * u64::from(decoded.height()) > u64::from(MAX_IMAGE_PIXELS)
    {
        return None;
    }
    let mut output = Cursor::new(Vec::new());
    decoded
        .write_to(&mut output, image::ImageFormat::Png)
        .ok()?;
    let encoded = output.into_inner();
    if encoded.len() > MAX_IMAGE_OUTPUT_BYTES {
        return None;
    }
    Some((encoded, decoded.width(), decoded.height()))
}

fn xml_escape(value: &str) -> Result<String, DocxError> {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            '\t' | '\n' | '\r' => output.push(character),
            c if c < '\u{20}' || c == '\u{fffe}' || c == '\u{ffff}' => {
                return Err(DocxError::InvalidDocumentXml(
                    "text contains a control character that invalid XML character".into(),
                ))
            }
            c => output.push(c),
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::LegacyDocPackage;
    use crate::docx::TableRow;

    fn png() -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 120, 80, 255]));
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn clean_docx_drops_active_parts_and_keeps_text_and_image() {
        let folder = tempfile::tempdir().unwrap();
        let source_doc = folder.path().join("source.doc");
        std::fs::write(&source_doc, b"dummy legacy source").unwrap();
        let legacy = LegacyDocPackage {
            paragraphs: vec!["Кириллица =SUM(A1) & <tag>".into()],
            images: vec![crate::doc::LegacyDocImage {
                bytes: png(),
                format: crate::doc::LegacyImageFormat::Png,
            }],
            ..Default::default()
        };
        let source = folder.path().join("source.docx");
        let package = DocxPackage::from_legacy(&source_doc, &legacy, &[], &[]).unwrap();
        package.save_as(&source).unwrap();
        let package = DocxPackage::open(&source).unwrap();
        let destination = folder.path().join("copy-clean.docx");
        let report = clean_docx(&source, &package, &destination).unwrap();
        assert_eq!(report.images, 1);
        let reopened = DocxPackage::open(&destination).unwrap();
        assert_eq!(
            texts(&reopened.model.paragraphs[0]),
            "Кириллица =SUM(A1) & <tag>"
        );
        let mut zip = ZipArchive::new(std::fs::File::open(&destination).unwrap()).unwrap();
        let names = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect::<Vec<_>>();
        assert!(names.iter().all(|name| name == "[Content_Types].xml"
            || name == "_rels/.rels"
            || name == "word/document.xml"
            || name == "word/_rels/document.xml.rels"
            || name.starts_with("word/media/image")));
        assert!(!names
            .iter()
            .any(|name| name.contains("vba") || name.contains("ole")));
        let mut document = String::new();
        zip.by_name("word/document.xml")
            .unwrap()
            .read_to_string(&mut document)
            .unwrap();
        assert!(document.contains("&amp; &lt;tag&gt;"));
        assert!(document.contains("=SUM(A1)"));
        assert!(!document.contains("<w:fldSimple"));
        assert!(!document.contains("<w:instrText"));
    }

    #[test]
    fn clean_docx_renders_table_structure() {
        let model = DocumentModel {
            paragraphs: vec![
                Paragraph {
                    fragments: vec![crate::docx::TextFragment {
                        id: 0,
                        text: "Левая".into(),
                        run_id: 0,
                        font_size_half_points: Some(24),
                    }],
                    ..Default::default()
                },
                Paragraph {
                    fragments: vec![crate::docx::TextFragment {
                        id: 1,
                        text: "Правая".into(),
                        run_id: 1,
                        font_size_half_points: None,
                    }],
                    ..Default::default()
                },
            ],
            blocks: vec![DocumentBlock::Table {
                rows: vec![TableRow {
                    cells: vec![vec![0], vec![1]],
                }],
                grid_widths: vec![2000, 3000],
            }],
        };
        let (parts, report) = build_clean_parts(&model, &[]).unwrap();
        assert_eq!(report.tables, 1);
        assert_eq!(report.cells, 2);
        let document = parts
            .iter()
            .find(|(name, _)| name == "word/document.xml")
            .map(|(_, bytes)| String::from_utf8(bytes.clone()).unwrap())
            .unwrap();
        assert!(document.contains("<w:tbl>"));
        assert!(document.contains("<w:gridCol w:w=\"2000\"/>"));
        assert!(document.contains(">Левая</w:t>"));
        assert!(document.contains(">Правая</w:t>"));
        assert!(document.contains("<w:sz w:val=\"24\"/>"));
    }

    #[test]
    fn clean_docx_ignores_unexpected_parts_in_the_source_package() {
        let folder = tempfile::tempdir().unwrap();
        let source_doc = folder.path().join("source.doc");
        std::fs::write(&source_doc, b"dummy legacy source").unwrap();
        let legacy = LegacyDocPackage {
            paragraphs: vec!["Текст".into()],
            ..Default::default()
        };
        let source = folder.path().join("source.docx");
        let package = DocxPackage::from_legacy(&source_doc, &legacy, &[], &[]).unwrap();
        package.save_as(&source).unwrap();
        // Add an active binary part and an external relationship to the source.
        let mut entries = Vec::new();
        {
            let mut zip = ZipArchive::new(std::fs::File::open(&source).unwrap()).unwrap();
            for index in 0..zip.len() {
                let mut entry = zip.by_index(index).unwrap();
                let name = entry.name().to_string();
                let mut bytes = Vec::new();
                entry.read_to_end(&mut bytes).unwrap();
                entries.push((name, bytes));
            }
        }
        for entry in &mut entries {
            if entry.0 == "word/_rels/document.xml.rels" {
                let text = String::from_utf8(entry.1.clone()).unwrap();
                entry.1 = text
                    .replace(
                        "</Relationships>",
                        "<Relationship Id=\"ext\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"https://invalid.test\" TargetMode=\"External\"/></Relationships>",
                    )
                    .into_bytes();
            }
        }
        entries.push(("word/vbaProject.bin".into(), vec![1, 2, 3]));
        {
            let file = std::fs::File::create(&source).unwrap();
            let mut zip = ZipWriter::new(file);
            for (name, bytes) in entries {
                zip.start_file(name, SimpleFileOptions::default()).unwrap();
                zip.write_all(&bytes).unwrap();
            }
            zip.finish().unwrap();
        }
        let package = DocxPackage::open(&source).unwrap();
        let destination = folder.path().join("clean.docx");
        clean_docx(&source, &package, &destination).unwrap();
        let mut zip = ZipArchive::new(std::fs::File::open(&destination).unwrap()).unwrap();
        let names = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect::<Vec<_>>();
        assert!(!names.iter().any(|name| name.contains("vba")));
        if let Ok(mut rels) = zip.by_name("word/_rels/document.xml.rels") {
            let mut xml = String::new();
            rels.read_to_string(&mut xml).unwrap();
            assert!(!xml.contains("invalid.test"));
            assert!(!xml.contains("TargetMode"));
        };
    }

    #[test]
    fn clean_docx_skips_non_raster_payloads() {
        let model = DocumentModel {
            paragraphs: vec![Paragraph {
                fragments: vec![crate::docx::TextFragment {
                    id: 0,
                    text: "Текст".into(),
                    run_id: 0,
                    font_size_half_points: None,
                }],
                ..Default::default()
            }],
            blocks: vec![DocumentBlock::Paragraph(0)],
        };
        let images = vec![DocxImage {
            bytes: b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec(),
            paragraph_index: 0,
            extent_emu: None,
        }];
        let (_parts, report) = build_clean_parts(&model, &images).unwrap();
        assert_eq!(report.images, 0);
        assert_eq!(report.images_skipped, 1);
    }

    #[test]
    fn clean_docx_refuses_a_hard_link_to_the_source() {
        let folder = tempfile::tempdir().unwrap();
        let source_doc = folder.path().join("source.doc");
        std::fs::write(&source_doc, b"dummy legacy source").unwrap();
        let legacy = LegacyDocPackage {
            paragraphs: vec!["Оригинал".into()],
            ..Default::default()
        };
        let source = folder.path().join("source.docx");
        let package = DocxPackage::from_legacy(&source_doc, &legacy, &[], &[]).unwrap();
        package.save_as(&source).unwrap();
        let package = DocxPackage::open(&source).unwrap();
        let alias = folder.path().join("alias.docx");
        std::fs::hard_link(&source, &alias).unwrap();
        let before = std::fs::read(&source).unwrap();
        assert!(clean_docx(&source, &package, &alias).is_err());
        assert_eq!(std::fs::read(&source).unwrap(), before);
    }

    #[test]
    fn clean_workbook_keeps_values_as_text_not_formulas() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("source.xlsx");
        std::fs::write(&source, b"dummy original").unwrap();
        let sheets = vec![SpreadsheetModel {
            sheet_name: "Данные".into(),
            cells: vec![
                crate::xlsx::SpreadsheetCell {
                    row: 0,
                    column: 0,
                    value: "=SUM(A1)".into(),
                },
                crate::xlsx::SpreadsheetCell {
                    row: 0,
                    column: 1,
                    value: "Кириллица".into(),
                },
                crate::xlsx::SpreadsheetCell {
                    row: 0,
                    column: 2,
                    value: "2024-01-02".into(),
                },
            ],
            row_count: 1,
            column_count: 3,
        }];
        let destination = folder.path().join("copy-clean.xlsx");
        let report = clean_workbook(&source, &sheets, &BTreeMap::new(), &destination).unwrap();
        assert_eq!(report.values, 3);
        let reopened = crate::xlsx::XlsxPackage::open(&destination).unwrap();
        assert_eq!(reopened.model.cell_value(0, 0), "=SUM(A1)");
        assert_eq!(reopened.model.cell_value(0, 1), "Кириллица");
        assert_eq!(reopened.model.cell_value(0, 2), "2024-01-02");
        let mut zip = ZipArchive::new(std::fs::File::open(&destination).unwrap()).unwrap();
        let mut sheet = String::new();
        zip.by_name("xl/worksheets/sheet1.xml")
            .unwrap()
            .read_to_string(&mut sheet)
            .unwrap();
        assert!(sheet.contains("inlineStr"));
        assert!(!sheet.contains("<f>"));
    }

    #[test]
    fn clean_copy_refuses_the_source_alias_and_leaves_it_unchanged() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("source.docx");
        std::fs::write(&source, b"not a real package").unwrap();
        let before = std::fs::read(&source).unwrap();
        let package = DocxPackage::open(&source);
        assert!(package.is_err());
        assert_eq!(std::fs::read(&source).unwrap(), before);
    }
}
