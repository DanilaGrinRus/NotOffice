//! Passive DOC recovery -> minimal editable OOXML. No legacy streams are copied.
use crate::doc::{LegacyDocPackage, LegacyImageFormat};
use crate::docx::DocxError;

type Parts = Vec<(String, Vec<u8>)>;

fn escaped(text: &str) -> Result<String, DocxError> {
    if text.chars().any(|c| {
        !matches!(c, '\t' | '\n' | '\r') && (c < '\u{20}' || matches!(c, '\u{fffe}' | '\u{ffff}'))
    }) {
        return Err(DocxError::InvalidDocumentXml(
            "recovered text contains characters forbidden in XML".into(),
        ));
    }
    Ok(quick_xml::escape::escape(text).into_owned())
}

pub(crate) fn build_package_parts(
    legacy: &LegacyDocPackage,
    anchors: &[Option<usize>],
    widths: &[f32],
) -> Result<Parts, DocxError> {
    if legacy.images.len() > 64 {
        return Err(DocxError::ResourceLimit("too many recovered images".into()));
    }
    let mut parts = vec![
        ("[Content_Types].xml".into(), br#"<?xml version="1.0" encoding="UTF-8"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="jpg" ContentType="image/jpeg"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.to_vec()),
        ("_rels/.rels".into(), br#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="office" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#.to_vec()),
    ];
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>"#,
    );
    let mut rels = String::from(
        r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    let mut drawings = vec![Vec::new(); legacy.paragraphs.len().max(1)];
    let mut gallery = Vec::new();
    let mut total_bytes = 0;
    for (index, image) in legacy.images.iter().enumerate() {
        total_bytes += image.bytes.len();
        if image.bytes.len() > 12 * 1024 * 1024 || total_bytes > 48 * 1024 * 1024 {
            return Err(DocxError::ResourceLimit(
                "recovered images exceed export budget".into(),
            ));
        }
        // Dimensions are read under the same pixel budget as the preview. An
        // undecodable recovered raster blocks conversion rather than disappearing.
        let reader =
            image::ImageReader::new(std::io::Cursor::new(&image.bytes)).with_guessed_format()?;
        let (pixel_width, pixel_height) = reader.into_dimensions().map_err(|e| {
            DocxError::InvalidPackage(format!("recovered image {}: {e}", index + 1))
        })?;
        if pixel_width == 0
            || pixel_height == 0
            || u64::from(pixel_width) * u64::from(pixel_height) > 40_000_000
        {
            return Err(DocxError::ResourceLimit(
                "recovered image dimensions are unsafe".into(),
            ));
        }
        let width_pct = widths
            .get(index)
            .copied()
            .filter(|w| w.is_finite())
            .unwrap_or(85.0)
            .clamp(10.0, 100.0);
        let width = (6_858_000.0_f64 * f64::from(width_pct) / 100.0)
            .min(f64::from(pixel_width) * 9525.0) as u64;
        let height = width * u64::from(pixel_height) / u64::from(pixel_width);
        let ext = match image.format {
            LegacyImageFormat::Png => "png",
            LegacyImageFormat::Jpeg => "jpg",
        };
        let number = index + 1;
        parts.push((
            format!("word/media/image{number}.{ext}"),
            image.bytes.clone(),
        ));
        rels.push_str(&format!(r#"<Relationship Id="img{number}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image{number}.{ext}"/>"#));
        let drawing = format!(
            r#"<w:p><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{width}" cy="{height}"/><wp:docPr id="{number}" name="Recovered image {number}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{number}" name="image{number}.{ext}"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="img{number}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{width}" cy="{height}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#
        );
        let anchor = anchors.get(index).copied().flatten().or_else(|| {
            legacy
                .verified_shape_images
                .iter()
                .find(|a| a.image_index == index)
                .map(|a| a.paragraph_index)
        });
        if let Some(at) = anchor.filter(|at| *at < drawings.len()) {
            drawings[at].push(drawing);
        } else {
            gallery.push(drawing);
        }
    }
    for (index, paragraph) in legacy.paragraphs.iter().enumerate() {
        // Soft line breaks become editable paragraphs; original table cells
        // were already flattened by the importer. This is disclosed in the UI.
        for line in paragraph.split('\n') {
            xml.push_str(&format!(r#"<w:p><w:r><w:rPr><w:sz w:val="24"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#, escaped(line)?));
        }
        for drawing in &drawings[index] {
            xml.push_str(drawing);
        }
    }
    if legacy.paragraphs.is_empty() {
        xml.push_str("<w:p><w:r><w:t></w:t></w:r></w:p>");
        for drawing in &drawings[0] {
            xml.push_str(drawing);
        }
    }
    for drawing in gallery {
        xml.push_str(&drawing);
    }
    xml.push_str(r#"<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="567" w:right="567" w:bottom="567" w:left="567" w:header="0" w:footer="0" w:gutter="0"/></w:sectPr></w:body></w:document>"#);
    rels.push_str("</Relationships>");
    parts.push(("word/document.xml".into(), xml.into_bytes()));
    parts.push(("word/_rels/document.xml.rels".into(), rels.into_bytes()));
    Ok(parts)
}
