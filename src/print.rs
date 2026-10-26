//! Windows printing of ordered paragraphs and recovered raster images.
//! Printed positions approximate the NotOffice preview, NOT the source Word layout.
#[cfg(windows)]
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSettings {
    pub landscape: bool,
    pub margin_mm: f32,
    pub image_scale: ImageScale,
    pub print_page_numbers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageScale {
    OriginalSize,
    FitToWidth,
    CustomPercent,
}

impl ImageScale {
    fn token(self) -> &'static str {
        match self {
            Self::OriginalSize => "original",
            Self::FitToWidth => "fit",
            Self::CustomPercent => "percent",
        }
    }
}

impl Default for PageSettings {
    fn default() -> Self {
        Self {
            landscape: false,
            margin_mm: 16.5,
            image_scale: ImageScale::CustomPercent,
            print_page_numbers: true,
        }
    }
}

impl PageSettings {
    pub fn sanitized(self) -> Self {
        Self {
            landscape: self.landscape,
            margin_mm: if self.margin_mm.is_finite() {
                self.margin_mm.clamp(5.0, 40.0)
            } else {
                16.5
            },
            image_scale: self.image_scale,
            print_page_numbers: self.print_page_numbers,
        }
    }
    pub fn page_size_mm(self) -> (f32, f32) {
        if self.landscape {
            (297.0, 210.0)
        } else {
            (210.0, 297.0)
        }
    }
    pub fn printable_width_mm(self) -> f32 {
        self.page_size_mm().0 - 2.0 * self.sanitized().margin_mm
    }

    pub fn printable_height_mm(self) -> f32 {
        self.page_size_mm().1 - 2.0 * self.sanitized().margin_mm
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrintBlock {
    Text(String),
    /// A DOCX table row. The Windows renderer draws a border around each cell.
    TableRow(Vec<String>),
    Image {
        bytes: Vec<u8>,
        width_percent: f32,
    },
    /// DOCX image fitted once by the shared preview/print page plan, in points.
    SizedImage {
        bytes: Vec<u8>,
        width_pt: f32,
        height_pt: f32,
    },
    /// A page boundary from the shared DOC or DOCX page plan.
    PageBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintOutcome {
    Printed,
    Cancelled,
}

pub(crate) fn print_outcome_from_helper_output(output: &str) -> Result<PrintOutcome, String> {
    if output
        .lines()
        .any(|line| line.trim() == "LITEOFFICE_PRINT_RESULT:printed")
    {
        Ok(PrintOutcome::Printed)
    } else if output
        .lines()
        .any(|line| line.trim() == "LITEOFFICE_PRINT_RESULT:cancelled")
    {
        Ok(PrintOutcome::Cancelled)
    } else {
        Err("Windows print helper completed without a result marker".into())
    }
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let a = chunk[0];
        let b = *chunk.get(1).unwrap_or(&0);
        let c = *chunk.get(2).unwrap_or(&0);
        result.push(ALPHABET[(a >> 2) as usize] as char);
        result.push(ALPHABET[(((a & 3) << 4) | (b >> 4)) as usize] as char);
        result.push(if chunk.len() >= 2 {
            ALPHABET[(((b & 15) << 2) | (c >> 6)) as usize] as char
        } else {
            '='
        });
        result.push(if chunk.len() >= 3 {
            ALPHABET[(c & 63) as usize] as char
        } else {
            '='
        });
    }
    result
}

#[cfg(test)]
fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

#[cfg(test)]
fn base64_decode(encoded: &str) -> Result<Vec<u8>, String> {
    let bytes = encoded.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("base64 length is not divisible by four".into());
    }
    let mut decoded = Vec::with_capacity(bytes.len() / 4 * 3);
    let (chunks, _) = bytes.as_chunks::<4>();
    for (index, chunk) in chunks.iter().enumerate() {
        let is_last = index + 1 == bytes.len() / 4;
        let a = base64_value(chunk[0]).ok_or("invalid base64 data")?;
        let b = base64_value(chunk[1]).ok_or("invalid base64 data")?;
        let c = if chunk[2] == b'=' {
            None
        } else {
            Some(base64_value(chunk[2]).ok_or("invalid base64 data")?)
        };
        let d = if chunk[3] == b'=' {
            None
        } else {
            Some(base64_value(chunk[3]).ok_or("invalid base64 data")?)
        };
        if (!is_last && (c.is_none() || d.is_none())) || (c.is_none() && d.is_some()) {
            return Err("invalid base64 padding".into());
        }
        decoded.push((a << 2) | (b >> 4));
        if let Some(c) = c {
            decoded.push((b << 4) | (c >> 2));
            if let Some(d) = d {
                decoded.push((c << 6) | d);
            }
        }
    }
    Ok(decoded)
}

fn validate_blocks(blocks: &[PrintBlock]) -> Result<(), String> {
    if blocks.is_empty() {
        return Err("Nothing to print".into());
    }
    if blocks.len() > 20_000 {
        return Err("Print job contains too many blocks".into());
    }
    let mut total_image_bytes = 0usize;
    let mut previous_was_break = true;
    for block in blocks {
        match block {
            PrintBlock::Text(text) => {
                if text.len() > 4_000_000 {
                    return Err("Text block too large".into());
                }
                previous_was_break = false;
            }
            PrintBlock::TableRow(cells) => {
                if cells.is_empty()
                    || cells.len() > 128
                    || cells.iter().any(|cell| cell.len() > 1_000_000)
                {
                    return Err("Invalid print table row".into());
                }
                previous_was_break = false;
            }
            PrintBlock::Image { bytes, .. } | PrintBlock::SizedImage { bytes, .. } => {
                if bytes.len() > 12 * 1024 * 1024 || bytes.is_empty() {
                    return Err("Invalid print image size".into());
                }
                match block {
                    PrintBlock::Image { width_percent, .. } if !width_percent.is_finite() => {
                        return Err("Invalid print image width".into())
                    }
                    PrintBlock::SizedImage {
                        width_pt,
                        height_pt,
                        ..
                    } if !width_pt.is_finite()
                        || !height_pt.is_finite()
                        || *width_pt <= 0.0
                        || *height_pt <= 0.0
                        || *width_pt > 842.0
                        || *height_pt > 842.0 =>
                    {
                        return Err("Invalid sized print image geometry".into())
                    }
                    _ => {}
                }
                total_image_bytes = total_image_bytes
                    .checked_add(bytes.len())
                    .ok_or("Print image budget overflow")?;
                if total_image_bytes > 48 * 1024 * 1024 {
                    return Err("Print image budget exceeded".into());
                }
                previous_was_break = false;
            }
            PrintBlock::PageBreak => {
                if previous_was_break {
                    return Err("Page breaks must separate non-empty planned pages".into());
                }
                previous_was_break = true;
            }
        }
    }
    if previous_was_break {
        return Err("Print job cannot end with a page break".into());
    }
    Ok(())
}

/// Serialize a bounded print job as data consumed by the Windows renderer.
pub fn spool_manifest(blocks: &[PrintBlock], settings: PageSettings) -> Result<String, String> {
    validate_blocks(blocks)?;
    let settings = settings.sanitized();
    // S record contains only trusted numeric page configuration; content stays base64-encoded.
    let mut lines = Vec::with_capacity(blocks.len() + 1);
    lines.push(format!(
        "S|{}|{:.2}|{}|{}",
        if settings.landscape { "L" } else { "P" },
        settings.margin_mm,
        settings.image_scale.token(),
        if settings.print_page_numbers { 1 } else { 0 }
    ));
    for block in blocks {
        match block {
            PrintBlock::Text(text) => {
                lines.push(format!("T|{}", base64(text.as_bytes())));
            }
            PrintBlock::TableRow(cells) => {
                let encoded = cells
                    .iter()
                    .map(|cell| base64(cell.as_bytes()))
                    .collect::<Vec<_>>();
                lines.push(format!("R|{}|{}", cells.len(), encoded.join("|")));
            }
            PrintBlock::Image {
                bytes,
                width_percent,
            } => {
                let width = width_percent.clamp(15.0, 100.0);
                lines.push(format!("I|{width:.1}|{}", base64(bytes)));
            }
            PrintBlock::SizedImage {
                bytes,
                width_pt,
                height_pt,
            } => {
                lines.push(format!("D|{width_pt:.3}|{height_pt:.3}|{}", base64(bytes)));
            }
            PrintBlock::PageBreak => lines.push("P".into()),
        }
    }
    Ok(lines.join("\n"))
}

#[cfg(test)]
fn parse_manifest(manifest: &str) -> Result<(PageSettings, Vec<PrintBlock>), String> {
    let mut lines = manifest.lines();
    let header = lines.next().ok_or("Missing print settings record")?;
    let fields = header.split('|').collect::<Vec<_>>();
    if fields.len() != 5 || fields[0] != "S" {
        return Err("Invalid print settings record".into());
    }
    let landscape = match fields[1] {
        "P" => false,
        "L" => true,
        _ => return Err("Invalid page orientation".into()),
    };
    let margin_mm: f32 = fields[2].parse().map_err(|_| "Invalid page margin")?;
    if !margin_mm.is_finite() || !(5.0..=40.0).contains(&margin_mm) {
        return Err("Invalid page margin".into());
    }
    let image_scale = match fields[3] {
        "original" => ImageScale::OriginalSize,
        "fit" => ImageScale::FitToWidth,
        "percent" => ImageScale::CustomPercent,
        _ => return Err("Invalid image scale mode".into()),
    };
    let print_page_numbers = match fields[4] {
        "0" => false,
        "1" => true,
        _ => return Err("Invalid page number setting".into()),
    };
    let mut blocks = Vec::new();
    for line in lines {
        if line == "P" {
            blocks.push(PrintBlock::PageBreak);
            continue;
        }
        let fields = line.split('|').collect::<Vec<_>>();
        match fields.as_slice() {
            ["T", encoded] => {
                let bytes = base64_decode(encoded)?;
                let text = String::from_utf8(bytes).map_err(|_| "Text is not UTF-8")?;
                blocks.push(PrintBlock::Text(text));
            }
            ["R", count, encoded @ ..] => {
                let count: usize = count.parse().map_err(|_| "Invalid table cell count")?;
                if count == 0 || count != encoded.len() {
                    return Err("Invalid table row".into());
                }
                let cells = encoded
                    .iter()
                    .map(|value| {
                        String::from_utf8(base64_decode(value)?)
                            .map_err(|_| "Table cell is not UTF-8".into())
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                blocks.push(PrintBlock::TableRow(cells));
            }
            ["I", width, encoded] => {
                let width_percent: f32 = width.parse().map_err(|_| "Invalid image width")?;
                let bytes = base64_decode(encoded)?;
                blocks.push(PrintBlock::Image {
                    bytes,
                    width_percent,
                });
            }
            ["D", width, height, encoded] => {
                blocks.push(PrintBlock::SizedImage {
                    bytes: base64_decode(encoded)?,
                    width_pt: width.parse().map_err(|_| "Invalid sized image width")?,
                    height_pt: height.parse().map_err(|_| "Invalid sized image height")?,
                });
            }
            _ => return Err("Invalid print block record".into()),
        }
    }
    validate_blocks(&blocks)?;
    Ok((
        PageSettings {
            landscape,
            margin_mm,
            image_scale,
            print_page_numbers,
        },
        blocks,
    ))
}

#[cfg(windows)]
fn encode_powershell_command(source: &str) -> String {
    let utf16 = source
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    base64(&utf16)
}

#[cfg(windows)]
pub(crate) struct PrintJobFiles {
    directory: std::path::PathBuf,
    assets: Vec<std::path::PathBuf>,
}

#[cfg(windows)]
impl PrintJobFiles {
    pub(crate) fn new(manifest: &str, helper: &str) -> Result<Self, String> {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("liteoffice-print-{}-{seed}", std::process::id()));
        std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
        let files = Self {
            directory,
            assets: Vec::new(),
        };
        for (name, data) in [("job.txt", manifest), ("helper.ps1", helper)] {
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(files.directory.join(name))
                .map_err(|error| error.to_string())?;
            file.write_all(data.as_bytes())
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
        }
        Ok(files)
    }

    pub(crate) fn add_asset(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        if !name.starts_with("page-") || !name.ends_with(".png") || name.contains(['/', '\\', ':'])
        {
            return Err("Unsafe print asset name".into());
        }
        let path = self.directory.join(name);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|e| e.to_string())?;
        self.assets.push(path);
        file.write_all(bytes).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub(crate) fn command(
        &self,
        pdf_test_output: Option<&std::path::Path>,
    ) -> std::process::Command {
        use std::os::windows::process::CommandExt;
        // Keep the Windows command line independent of helper/content length.
        // As before, execute trusted embedded helper code in memory. Document
        // paths/content are passed as environment data, never script syntax.
        const BOOTSTRAP: &str = "& ([ScriptBlock]::Create([IO.File]::ReadAllText($env:LITEOFFICE_PRINT_HELPER,[Text.Encoding]::UTF8))) -PdfTestOutput $env:LITEOFFICE_PDF_TEST_OUTPUT";
        let mut command = std::process::Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-NoLogo",
                "-STA",
                "-EncodedCommand",
                &encode_powershell_command(BOOTSTRAP),
            ])
            .env("LITEOFFICE_PRINT_INPUT", self.directory.join("job.txt"))
            .env("LITEOFFICE_PRINT_HELPER", self.directory.join("helper.ps1"))
            .env(
                "LITEOFFICE_PDF_TEST_OUTPUT",
                pdf_test_output
                    .map(std::path::Path::as_os_str)
                    .unwrap_or_default(),
            )
            .creation_flags(0x08000000);
        command
    }
}

#[cfg(windows)]
impl Drop for PrintJobFiles {
    fn drop(&mut self) {
        for path in &self.assets {
            let _ = std::fs::remove_file(path);
        }
        // Remove only the two files and directory exclusively created by us.
        for name in ["job.txt", "helper.ps1"] {
            let _ = std::fs::remove_file(self.directory.join(name));
        }
        let _ = std::fs::remove_dir(&self.directory);
    }
}

/// Internal Windows renderer entry point shared by the GUI and PDF regressions.
/// `None` opens the native dialogs; `Some` selects the existing PDF test mode.
#[cfg(windows)]
#[doc(hidden)]
pub fn run_windows_print_helper(
    manifest: &str,
    pdf_test_output: Option<&std::path::Path>,
) -> Result<PrintOutcome, String> {
    let files = PrintJobFiles::new(manifest, include_str!("print_windows.ps1"))?;
    let result = files.command(pdf_test_output).output();
    match result {
        Ok(output) if output.status.success() => {
            print_outcome_from_helper_output(&String::from_utf8_lossy(&output.stdout))
        }
        Ok(output) => Err(format!(
            "Print preview failed: {} {}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        )),
        Err(error) => Err(format!("Cannot open Windows print preview: {error}")),
    }
}

pub fn print_blocks(
    blocks: Vec<PrintBlock>,
    settings: PageSettings,
) -> Result<PrintOutcome, String> {
    let manifest = spool_manifest(&blocks, settings)?;
    #[cfg(not(windows))]
    {
        let _ = manifest;
        Err("Printing requires Windows".into())
    }
    #[cfg(windows)]
    {
        run_windows_print_helper(&manifest, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_preserves_utf8_and_order() {
        let blocks = vec![
            PrintBlock::Text("Русский✓".into()),
            PrintBlock::Image {
                bytes: vec![1, 2, 3],
                width_percent: 47.0,
            },
            PrintBlock::Text("end".into()),
        ];
        let lines = spool_manifest(&blocks, PageSettings::default()).unwrap();
        let list: Vec<_> = lines.split('\n').collect();
        assert_eq!(list.len(), 4);
        assert_eq!(list[0], "S|P|16.50|percent|1");
        assert_eq!(list[1], format!("T|{}", base64("Русский✓".as_bytes())));
        assert_eq!(list[2], "I|47.0|AQID");
        assert_eq!(list[3], "T|ZW5k");
    }

    #[test]
    fn manifest_preserves_explicit_page_breaks() {
        let manifest = spool_manifest(
            &[
                PrintBlock::Text("first".into()),
                PrintBlock::PageBreak,
                PrintBlock::Text("second".into()),
            ],
            PageSettings::default(),
        )
        .unwrap();

        assert_eq!(
            manifest.lines().collect::<Vec<_>>(),
            ["S|P|16.50|percent|1", "T|Zmlyc3Q=", "P", "T|c2Vjb25k"]
        );
    }

    #[test]
    fn manifest_roundtrip_preserves_order_unicode_images_and_page_numbers() {
        let settings = PageSettings {
            landscape: true,
            margin_mm: 9.5,
            image_scale: ImageScale::FitToWidth,
            print_page_numbers: true,
        };
        let blocks = vec![
            PrintBlock::Text("Первая страница".into()),
            PrintBlock::TableRow(vec!["Лево".into(), "Право".into()]),
            PrintBlock::SizedImage {
                bytes: vec![137, 80, 78, 71],
                width_pt: 144.0,
                height_pt: 72.0,
            },
            PrintBlock::Image {
                bytes: vec![0, 1, 2, 3],
                width_percent: 100.0,
            },
            PrintBlock::PageBreak,
            PrintBlock::Text("第二页 ✓".into()),
        ];
        let manifest = spool_manifest(&blocks, settings).unwrap();
        assert_eq!(parse_manifest(&manifest).unwrap(), (settings, blocks));
    }

    #[test]
    fn manifest_rejects_ambiguous_page_boundaries_and_malformed_records() {
        for blocks in [
            vec![PrintBlock::PageBreak, PrintBlock::Text("later".into())],
            vec![PrintBlock::Text("first".into()), PrintBlock::PageBreak],
            vec![
                PrintBlock::Text("first".into()),
                PrintBlock::PageBreak,
                PrintBlock::PageBreak,
                PrintBlock::Text("third".into()),
            ],
        ] {
            assert!(spool_manifest(&blocks, PageSettings::default()).is_err());
        }
        assert!(parse_manifest("S|P|16.50|percent|1\nX|bad").is_err());
        assert!(parse_manifest("S|P|16.50|percent|1\nT|not-base64!").is_err());
    }

    #[test]
    fn empty_page_and_large_multipage_job_have_deterministic_contract() {
        let empty_page = vec![PrintBlock::Text(String::new())];
        let manifest = spool_manifest(&empty_page, PageSettings::default()).unwrap();
        assert_eq!(parse_manifest(&manifest).unwrap().1, empty_page);

        let mut blocks = Vec::new();
        for page in 0..40 {
            if page > 0 {
                blocks.push(PrintBlock::PageBreak);
            }
            for line in 0..75 {
                blocks.push(PrintBlock::Text(format!("строка {page}:{line}")));
            }
        }
        let manifest = spool_manifest(&blocks, PageSettings::default()).unwrap();
        let (_, parsed) = parse_manifest(&manifest).unwrap();
        assert_eq!(parsed, blocks);
        assert_eq!(
            parsed
                .iter()
                .filter(|block| matches!(block, PrintBlock::PageBreak))
                .count(),
            39
        );
    }

    #[test]
    fn print_helper_result_distinguishes_cancel_from_submission() {
        assert_eq!(
            print_outcome_from_helper_output("LITEOFFICE_PRINT_RESULT:printed\n").unwrap(),
            PrintOutcome::Printed
        );
        assert_eq!(
            print_outcome_from_helper_output("noise\nLITEOFFICE_PRINT_RESULT:cancelled").unwrap(),
            PrintOutcome::Cancelled
        );
        assert!(print_outcome_from_helper_output("no marker").is_err());
    }

    #[test]
    fn reject_oversized_images() {
        let blocks = vec![PrintBlock::Image {
            bytes: vec![1u8; 12 * 1024 * 1024 + 1],
            width_percent: 70.0,
        }];
        assert!(spool_manifest(&blocks, PageSettings::default()).is_err());
    }
    #[test]
    fn landscape_and_margins_are_consistent() {
        let s = PageSettings {
            landscape: true,
            margin_mm: 20.0,
            ..PageSettings::default()
        };
        assert_eq!(s.page_size_mm(), (297.0, 210.0));
        assert_eq!(s.printable_width_mm(), 257.0);
        assert_eq!(s.printable_height_mm(), 170.0);
        assert!(spool_manifest(&[PrintBlock::Text("ok".into())], s)
            .unwrap()
            .starts_with("S|L|20.00|percent|1\n"));
        assert_eq!(
            PageSettings {
                landscape: false,
                margin_mm: f32::NAN,
                ..PageSettings::default()
            }
            .sanitized()
            .margin_mm,
            16.5
        );
    }
    #[test]
    fn print_image_scaling_mode_and_page_numbers_are_manifested() {
        for (mode, token) in [
            (ImageScale::OriginalSize, "original"),
            (ImageScale::FitToWidth, "fit"),
            (ImageScale::CustomPercent, "percent"),
        ] {
            let settings = PageSettings {
                image_scale: mode,
                print_page_numbers: true,
                ..PageSettings::default()
            };
            let lines = spool_manifest(&[PrintBlock::Text("ok".into())], settings).unwrap();
            assert!(lines.starts_with(&format!("S|P|16.50|{token}|1\n")));
        }
    }

    #[test]
    fn sized_image_contract_rejects_nonfinite_or_unbounded_geometry() {
        for (width_pt, height_pt) in [
            (0.0, 1.0),
            (1.0, -1.0),
            (f32::NAN, 1.0),
            (1.0, f32::INFINITY),
            (843.0, 1.0),
        ] {
            let blocks = [PrintBlock::SizedImage {
                bytes: vec![1],
                width_pt,
                height_pt,
            }];
            assert!(spool_manifest(&blocks, PageSettings::default()).is_err());
        }
        assert!(parse_manifest("S|P|25.00|fit|1\nR|2|YQ==").is_err());
    }
    #[cfg(windows)]
    #[test]
    fn powershell_encoding_is_utf16le() {
        assert_eq!(encode_powershell_command("A"), "QQA=");
    }

    #[cfg(windows)]
    #[test]
    fn windows_launcher_runs_large_unicode_helper_with_a_short_command_and_cleans_up() {
        let helper = format!(
            "param([string]$PdfTestOutput)\n{}\nif ([IO.File]::ReadAllText($env:LITEOFFICE_PRINT_INPUT,[Text.Encoding]::UTF8) -ne 'Кириллица | $data') {{ throw 'manifest changed' }}\nWrite-Output 'LITEOFFICE_PRINT_RESULT:cancelled'",
            "# helper growth regression\n".repeat(5000)
        );
        assert!(encode_powershell_command(&helper).len() > 32767);
        let files = PrintJobFiles::new("Кириллица | $data", &helper).unwrap();
        let directory = files.directory.clone();
        let mut command = files.command(None);
        let command_units: usize = command
            .get_args()
            .map(|arg| arg.to_string_lossy().encode_utf16().count() + 3)
            .sum();
        assert!(
            command_units < 2048,
            "helper length leaked into the Windows command line"
        );
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            print_outcome_from_helper_output(&String::from_utf8_lossy(&output.stdout)).unwrap(),
            PrintOutcome::Cancelled
        );
        drop(files);
        assert!(!directory.exists(), "private helper files were left behind");
    }

    #[cfg(windows)]
    #[test]
    fn failed_windows_helper_leaves_no_owned_temporary_files() {
        let files = PrintJobFiles::new("T|data", "throw 'controlled failure'").unwrap();
        let directory = files.directory.clone();
        assert!(!files.command(None).output().unwrap().status.success());
        drop(files);
        assert!(!directory.exists());
    }
}
