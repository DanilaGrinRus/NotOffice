//! Versioned sidecar storage for manual legacy DOC image placement.
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_SIDECAR_BYTES: u64 = 128 * 1024;

pub type Layout = (Vec<Option<usize>>, Vec<f32>);

pub fn path_for(source: &Path) -> PathBuf {
    let mut name = source.as_os_str().to_os_string();
    name.push(".lo-layout");
    PathBuf::from(name)
}

pub fn save(
    source: &Path,
    paragraphs: usize,
    image_count: usize,
    anchors: &[Option<usize>],
    widths: &[f32],
) -> Result<PathBuf, String> {
    let length = std::fs::metadata(source)
        .map_err(|error| error.to_string())?
        .len();
    let sidecar = path_for(source);
    let mut data = format!("NotOffice-layout-v1\n{length} {paragraphs} {image_count}\n");
    for index in 0..image_count {
        let anchor = anchors
            .get(index)
            .copied()
            .flatten()
            .filter(|anchor| *anchor < paragraphs);
        let width = widths
            .get(index)
            .copied()
            .unwrap_or(85.0)
            .clamp(15.0, 100.0);
        data.push_str(&format!(
            "{} {width:.1}\n",
            anchor
                .map(|anchor| anchor.to_string())
                .unwrap_or_else(|| "-".into())
        ));
    }
    let temporary = sidecar.with_extension("lo-layout.tmp");
    std::fs::write(&temporary, data).map_err(|error| error.to_string())?;
    if sidecar.exists() {
        std::fs::remove_file(&sidecar).map_err(|error| error.to_string())?;
    }
    std::fs::rename(&temporary, &sidecar).map_err(|error| error.to_string())?;
    Ok(sidecar)
}

pub fn load(
    source: &Path,
    paragraphs: usize,
    image_count: usize,
) -> Result<Option<Layout>, String> {
    let sidecar = path_for(source);
    if !sidecar.exists() {
        return Ok(None);
    }
    let metadata = std::fs::metadata(&sidecar).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_SIDECAR_BYTES {
        return Err("Layout file is too large".into());
    }
    let mut data = String::new();
    std::fs::File::open(&sidecar)
        .map_err(|error| error.to_string())?
        .take(MAX_SIDECAR_BYTES + 1)
        .read_to_string(&mut data)
        .map_err(|error| error.to_string())?;
    if data.len() as u64 > MAX_SIDECAR_BYTES {
        return Err("Layout file is too large".into());
    }
    let mut lines = data.lines();
    if lines.next() != Some("NotOffice-layout-v1") {
        return Err("Unknown layout file version".into());
    }
    let header = lines.next().ok_or("Missing layout header")?;
    let parts: Vec<_> = header.split_whitespace().collect();
    if parts.len() != 3 {
        return Err("Invalid layout header".into());
    }
    let expected_size: u64 = parts[0].parse().map_err(|_| "Invalid source size")?;
    let expected_paragraphs: usize = parts[1].parse().map_err(|_| "Invalid paragraph count")?;
    let expected_images: usize = parts[2].parse().map_err(|_| "Invalid image count")?;
    let actual_size = std::fs::metadata(source)
        .map_err(|error| error.to_string())?
        .len();
    if expected_size != actual_size
        || expected_paragraphs != paragraphs
        || expected_images != image_count
    {
        return Err("Layout does not match this DOC; ignored".into());
    }
    let mut anchors = Vec::with_capacity(image_count);
    let mut widths = Vec::with_capacity(image_count);
    for _ in 0..image_count {
        let line = lines.next().ok_or("Incomplete layout file")?;
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() != 2 {
            return Err("Invalid layout entry".into());
        }
        let anchor = if fields[0] == "-" {
            None
        } else {
            let index: usize = fields[0].parse().map_err(|_| "Invalid paragraph index")?;
            if index >= paragraphs {
                return Err("Paragraph index outside document".into());
            }
            Some(index)
        };
        let width: f32 = fields[1].parse().map_err(|_| "Invalid width")?;
        if !width.is_finite() || !(15.0..=100.0).contains(&width) {
            return Err("Width outside allowed range".into());
        }
        anchors.push(anchor);
        widths.push(width);
    }
    Ok(Some((anchors, widths)))
}

#[cfg(test)]
mod tests {
    use super::{load, save};

    #[test]
    fn round_trips_the_existing_layout_v1_format() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("Документ.doc");
        std::fs::write(&source, b"legacy document bytes").unwrap();

        save(&source, 2, 2, &[Some(0), None], &[55.0, 85.0]).unwrap();

        assert_eq!(
            load(&source, 2, 2).unwrap(),
            Some((vec![Some(0), None], vec![55.0, 85.0]))
        );
    }

    #[test]
    fn rejects_a_sidecar_that_does_not_match_its_source_document() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("document.doc");
        std::fs::write(&source, b"before").unwrap();
        save(&source, 1, 0, &[], &[]).unwrap();
        std::fs::write(&source, b"after with another size").unwrap();

        assert!(load(&source, 1, 0).unwrap_err().contains("does not match"));
    }
}
