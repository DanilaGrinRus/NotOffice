//! Print the immutable opened PDF through passive Windows rasterization only.
use crate::print::PrintOutcome;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PdfPrintSettings {
    pub first: usize,
    pub last: usize,
    /// None fits each page to printable area; Some(100) uses original size.
    pub percent: Option<u16>,
}
impl Default for PdfPrintSettings {
    fn default() -> Self {
        Self {
            first: 1,
            last: 1,
            percent: None,
        }
    }
}
impl PdfPrintSettings {
    pub fn validate(self, pages: usize) -> Result<(), String> {
        if self.first == 0
            || self.last < self.first
            || self.last > pages
            || self.percent.is_some_and(|p| !(10..=400).contains(&p))
        {
            return Err("Invalid PDF page range or scale (10–400%)".into());
        }
        Ok(())
    }
}

pub fn print_pdf(
    snapshot: std::sync::Arc<Vec<u8>>,
    settings: PdfPrintSettings,
) -> Result<PrintOutcome, String> {
    #[cfg(windows)]
    {
        print_pdf_to(snapshot, settings, None)
    }
    #[cfg(not(windows))]
    {
        let _ = (snapshot, settings);
        Err("PDF printing requires Windows".into())
    }
}

pub fn print_pdf_isolated(
    snapshot: std::sync::Arc<Vec<u8>>,
    settings: PdfPrintSettings,
) -> Result<PrintOutcome, String> {
    #[cfg(windows)]
    {
        print_pdf_to_inner(snapshot, settings, None, true)
    }
    #[cfg(not(windows))]
    {
        let _ = (snapshot, settings);
        Err("PDF printing requires Windows".into())
    }
}

#[cfg(windows)]
#[doc(hidden)]
pub fn print_pdf_to(
    snapshot: std::sync::Arc<Vec<u8>>,
    settings: PdfPrintSettings,
    output: Option<&std::path::Path>,
) -> Result<PrintOutcome, String> {
    print_pdf_to_inner(snapshot, settings, output, false)
}

fn print_pdf_to_inner(
    snapshot: std::sync::Arc<Vec<u8>>,
    settings: PdfPrintSettings,
    output: Option<&std::path::Path>,
    isolated: bool,
) -> Result<PrintOutcome, String> {
    use crate::pdf::PdfSession;
    let session = if isolated {
        PdfSession::from_snapshot_isolated(snapshot)?
    } else {
        PdfSession::from_snapshot(snapshot)?
    };
    settings.validate(session.pages.len())?;
    let scale = settings
        .percent
        .map(|p| p.to_string())
        .unwrap_or_else(|| "fit".into());
    let mut manifest = format!("PDF|{scale}\n");
    for index in settings.first - 1..settings.last {
        let size = session.pages[index];
        manifest.push_str(&format!(
            "P|page-{index:04}.png|{:.4}|{:.4}\n",
            size[0] * 100.0 / 96.0,
            size[1] * 100.0 / 96.0
        ));
    }
    let mut files =
        crate::print::PrintJobFiles::new(&manifest, include_str!("pdf_print_windows.ps1"))?;
    let mut total = 0;
    for index in settings.first - 1..settings.last {
        session.request_page(index)?;
        let (actual, png) = session
            .responses
            .recv_timeout(std::time::Duration::from_secs(30))
            .map_err(|e| format!("PDF print raster timeout: {e}"))?;
        if actual != index {
            return Err("Unexpected PDF print page".into());
        }
        let png = png?;
        total += png.len();
        if total > 256 * 1024 * 1024 {
            return Err("PDF print rasters exceed 256 MiB; select a smaller page range".into());
        }
        files.add_asset(&format!("page-{index:04}.png"), &png)?;
    }
    let result = files
        .run(output)
        .map_err(|e| format!("Cannot open PDF print dialog: {e}"))?;
    if !result.status.success() {
        return Err(format!(
            "PDF printing failed: {} {}",
            String::from_utf8_lossy(&result.stderr),
            String::from_utf8_lossy(&result.stdout)
        ));
    }
    crate::print::print_outcome_from_helper_output(&String::from_utf8_lossy(&result.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranges_and_scales_are_validated_without_printing() {
        assert!(PdfPrintSettings {
            first: 2,
            last: 3,
            percent: Some(100)
        }
        .validate(3)
        .is_ok());
        for settings in [
            PdfPrintSettings {
                first: 0,
                ..Default::default()
            },
            PdfPrintSettings {
                first: 2,
                last: 1,
                percent: None,
            },
            PdfPrintSettings {
                last: 4,
                ..Default::default()
            },
            PdfPrintSettings {
                percent: Some(0),
                ..Default::default()
            },
        ] {
            assert!(settings.validate(3).is_err());
        }
    }
}
