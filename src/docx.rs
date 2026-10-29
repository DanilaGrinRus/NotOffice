use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Cursor, Read, Seek, Write};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;
use zip::read::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const DOCUMENT_XML: &str = "word/document.xml";
const MAX_DOCX_PACKAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_DOCX_PART_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DOCX_UNCOMPRESSED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_DOCX_PARTS: usize = 4_096;
const TEMP_CREATE_ATTEMPTS: u32 = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextFragment {
    /// Position in document.xml's ordered sequence of w:t nodes.
    pub id: usize,
    pub text: String,
    /// OOXML run containing this text fragment.
    pub run_id: usize,
    /// Explicit w:sz value in half-points; None inherits document styles.
    pub font_size_half_points: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Paragraph {
    pub fragments: Vec<TextFragment>,
    /// A Word page break that follows this paragraph.
    pub page_break_after: bool,
    pub alignment: ParagraphAlignment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParagraphAlignment {
    #[default]
    Left,
    Center,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentBlock {
    Paragraph(usize),
    Table {
        rows: Vec<TableRow>,
        /// Preferred column widths in twentieths of a point, from `w:tblGrid`.
        grid_widths: Vec<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TableRow {
    /// Paragraph indexes for each table cell, in document order.
    pub cells: Vec<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocumentModel {
    pub paragraphs: Vec<Paragraph>,
    /// Top-level paragraphs and table rows preserved for approximate display.
    pub blocks: Vec<DocumentBlock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxImage {
    pub bytes: Vec<u8>,
    /// Paragraph containing this drawing reference (including table paragraphs).
    pub paragraph_index: usize,
    /// OOXML drawing size in English Metric Units, when available.
    pub extent_emu: Option<(u64, u64)>,
}

impl DocumentModel {
    pub fn set_run_font_size(&mut self, fragment_id: usize, half_points: u16) -> bool {
        if !(12..=144).contains(&half_points) {
            return false;
        }
        let Some(run_id) = self
            .paragraphs
            .iter()
            .flat_map(|p| &p.fragments)
            .find(|f| f.id == fragment_id)
            .map(|f| f.run_id)
        else {
            return false;
        };
        for fragment in self.paragraphs.iter_mut().flat_map(|p| &mut p.fragments) {
            if fragment.run_id == run_id {
                fragment.font_size_half_points = Some(half_points);
            }
        }
        true
    }

    /// Apply one size to every run, including runs in table cells.
    /// Returns false if there are no fragments or the value is unchanged.
    pub fn set_all_run_font_sizes(&mut self, half_points: u16) -> bool {
        if !(12..=144).contains(&half_points) {
            return false;
        }
        let mut changed = false;
        for fragment in self
            .paragraphs
            .iter_mut()
            .flat_map(|p| p.fragments.iter_mut())
        {
            if fragment.font_size_half_points != Some(half_points) {
                fragment.font_size_half_points = Some(half_points);
                changed = true;
            }
        }
        changed
    }

    /// Common explicit size of all fragments, or None when mixed/unspecified.
    pub fn common_font_size(&self) -> Option<u16> {
        let mut fragments = self.paragraphs.iter().flat_map(|p| p.fragments.iter());
        let first = fragments.next()?.font_size_half_points?;
        fragments
            .all(|f| f.font_size_half_points == Some(first))
            .then_some(first)
    }

    pub fn fragment(&self, id: usize) -> Option<&TextFragment> {
        self.paragraphs
            .iter()
            .flat_map(|p| &p.fragments)
            .find(|f| f.id == id)
    }

    pub fn fragment_mut(&mut self, id: usize) -> Option<&mut TextFragment> {
        self.paragraphs
            .iter_mut()
            .flat_map(|paragraph| paragraph.fragments.iter_mut())
            .find(|fragment| fragment.id == id)
    }

    fn texts_in_xml_order(&self) -> Vec<&str> {
        let mut fragments: Vec<&TextFragment> = self
            .paragraphs
            .iter()
            .flat_map(|paragraph| paragraph.fragments.iter())
            .collect();
        fragments.sort_by_key(|fragment| fragment.id);
        fragments
            .into_iter()
            .map(|fragment| fragment.text.as_str())
            .collect()
    }
}

#[derive(Debug)]
pub enum DocxError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Xml(quick_xml::Error),
    MissingDocumentXml,
    InvalidDocumentXml(String),
    InvalidPackage(String),
    ResourceLimit(String),
    SourceChanged,
    UnsafeSave(String),
}

impl std::fmt::Display for DocxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "file operation failed: {error}"),
            Self::Zip(error) => write!(f, "DOCX ZIP operation failed: {error}"),
            Self::Xml(error) => write!(f, "DOCX XML operation failed: {error}"),
            Self::MissingDocumentXml => write!(f, "DOCX does not contain word/document.xml"),
            Self::InvalidDocumentXml(reason) => write!(f, "invalid word/document.xml: {reason}"),
            Self::InvalidPackage(reason) => write!(f, "unsafe or invalid DOCX package: {reason}"),
            Self::ResourceLimit(reason) => write!(f, "DOCX exceeds a safety limit: {reason}"),
            Self::SourceChanged => write!(
                f,
                "source document changed outside NotOffice; save was cancelled to avoid overwriting it"
            ),
            Self::UnsafeSave(reason) => write!(f, "document was not replaced: {reason}"),
        }
    }
}

impl std::error::Error for DocxError {}
impl From<std::io::Error> for DocxError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
impl From<zip::result::ZipError> for DocxError {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Zip(value)
    }
}
impl From<quick_xml::Error> for DocxError {
    fn from(value: quick_xml::Error) -> Self {
        Self::Xml(value)
    }
}

/// A DOCX package retains the original package bytes and only transforms
/// word/document.xml during save.  Relationships, styles, images and unknown
/// package parts are copied through untouched.
pub struct DocxPackage {
    source: PathBuf,
    /// A converted legacy document must be saved to a separate DOCX destination.
    imported_legacy: bool,
    source_fingerprint: SourceFingerprint,
    original_document_xml: Vec<u8>,
    /// Immutable package captured at open, so Save Copy cannot combine stale
    /// edits with externally replaced relationships, media or unknown parts.
    original_package: Vec<u8>,
    pub model: DocumentModel,
    /// Supported embedded Word media, retained for preview/print output.
    pub images: Vec<DocxImage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceFingerprint {
    byte_len: u64,
    modified: Option<SystemTime>,
    content_hash: u64,
}

impl DocxPackage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DocxError> {
        let source = path.as_ref().to_path_buf();
        let source_fingerprint = source_fingerprint(&source)?;
        let mut original_package = Vec::new();
        File::open(&source)?
            .take(MAX_DOCX_PACKAGE_BYTES + 1)
            .read_to_end(&mut original_package)?;
        if original_package.len() as u64 > MAX_DOCX_PACKAGE_BYTES {
            return Err(DocxError::ResourceLimit(
                "package grew beyond the input limit while opening".into(),
            ));
        }
        if source_fingerprint != self::source_fingerprint(&source)?
            || source_fingerprint.content_hash != content_hash(&original_package)
        {
            return Err(DocxError::SourceChanged);
        }
        let mut archive = ZipArchive::new(Cursor::new(original_package.as_slice()))?;
        validate_package(&mut archive)?;
        let mut document_xml = Vec::new();
        archive
            .by_name(DOCUMENT_XML)
            .map_err(|error| match error {
                zip::result::ZipError::FileNotFound => DocxError::MissingDocumentXml,
                other => DocxError::Zip(other),
            })?
            .read_to_end(&mut document_xml)?;
        let model = parse_document_xml(&document_xml)?;
        let images = embedded_images(&mut archive, &document_xml)?;
        Ok(Self {
            source,
            imported_legacy: false,
            source_fingerprint,
            original_document_xml: document_xml,
            original_package,
            model,
            images,
        })
    }

    pub fn source_path(&self) -> &Path {
        &self.source
    }

    pub fn requires_save_as(&self) -> bool {
        self.imported_legacy
    }

    /// Convert only the recovered passive content. Binary DOC styles, table
    /// structure, fields and executable streams are deliberately not copied.
    pub fn from_legacy(
        source: &Path,
        legacy: &crate::doc::LegacyDocPackage,
        anchors: &[Option<usize>],
        widths: &[f32],
    ) -> Result<Self, DocxError> {
        let fingerprint = source_fingerprint(source)?;
        let parts = crate::doc_export::build_package_parts(legacy, anchors, widths)?;
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in parts {
            zip.start_file(name, SimpleFileOptions::default())?;
            zip.write_all(&bytes)?;
        }
        let original_package = zip.finish()?.into_inner();
        if original_package.len() as u64 > MAX_DOCX_PACKAGE_BYTES {
            return Err(DocxError::ResourceLimit(
                "converted package is too large".into(),
            ));
        }
        let mut archive = ZipArchive::new(Cursor::new(original_package.as_slice()))?;
        validate_package(&mut archive)?;
        let mut xml = Vec::new();
        archive.by_name(DOCUMENT_XML)?.read_to_end(&mut xml)?;
        let model = parse_document_xml(&xml)?;
        let images = embedded_images(&mut archive, &xml)?;
        Ok(Self {
            source: source.to_path_buf(),
            imported_legacy: true,
            source_fingerprint: fingerprint,
            original_document_xml: xml,
            model,
            images,
            original_package,
        })
    }

    /// Recovery must validate even OPC parts that the current renderer does not
    /// read. Stream the bounded immutable snapshot to check every entry's CRC;
    /// otherwise damaged unknown parts could silently poison the next Save As.
    pub(crate) fn verify_all_parts(&self) -> Result<(), DocxError> {
        let mut archive = ZipArchive::new(Cursor::new(self.original_package.as_slice()))?;
        validate_package(&mut archive)?;
        for index in 0..archive.len() {
            let mut entry = archive.by_index(index)?;
            let expected = entry.size();
            let actual = std::io::copy(
                &mut entry.by_ref().take(MAX_DOCX_PART_BYTES + 1),
                &mut std::io::sink(),
            )?;
            if actual != expected || actual > MAX_DOCX_PART_BYTES {
                return Err(DocxError::InvalidPackage(
                    "part size differs from ZIP metadata".into(),
                ));
            }
        }
        Ok(())
    }

    /// Save and adopt the destination as the session's new baseline. Failed
    /// saves leave the opened package/model untouched. Save As therefore changes
    /// where the next normal Save writes and refreshes conflict detection.
    pub fn save(&mut self, destination: impl AsRef<Path>) -> Result<(), DocxError> {
        self.save_as(destination.as_ref())?;
        let saved = Self::open(destination)?;
        if saved.model != self.model {
            return Err(DocxError::SourceChanged);
        }
        *self = saved;
        Ok(())
    }

    /// Rebuild the package beside the destination, validate its edited XML,
    /// then replace the destination.  If an existing destination is replaced,
    /// its `.bak` copy is created first.
    pub fn save_as(&self, destination: impl AsRef<Path>) -> Result<(), DocxError> {
        let destination = destination.as_ref();
        let replaces_source = paths_refer_to_same_file(&self.source, destination)?;
        if self.imported_legacy && replaces_source {
            return Err(DocxError::UnsafeSave(
                "the original DOC is protected; choose a separate DOCX file".into(),
            ));
        }
        if replaces_source && source_fingerprint(&self.source)? != self.source_fingerprint {
            return Err(DocxError::SourceChanged);
        }
        let replacement_xml = rewrite_document_xml(&self.original_document_xml, &self.model)?;
        // Parsing the new XML before creating a ZIP avoids replacing a valid file
        // with a package that this model itself cannot reopen.
        parse_document_xml(&replacement_xml)?;

        let (temp_path, temp_file) = create_temporary_peer(destination)?;
        let copy_result = self.write_rebuilt_package(temp_file, &replacement_xml);
        if let Err(error) = copy_result {
            let _ = fs::remove_file(&temp_path);
            return Err(error);
        }

        if replaces_source && source_fingerprint(&self.source)? != self.source_fingerprint {
            let _ = fs::remove_file(&temp_path);
            return Err(DocxError::SourceChanged);
        }

        if let Err(error) = verify_rebuilt_package(&temp_path, &replacement_xml) {
            return Err(DocxError::UnsafeSave(format!(
                "temporary package kept at {} after verification failed: {error}",
                temp_path.display()
            )));
        }

        replace_destination(&temp_path, destination).map_err(|error| {
            DocxError::UnsafeSave(format!(
                "temporary package kept at {}: {error}",
                temp_path.display()
            ))
        })
    }

    fn write_rebuilt_package(
        &self,
        temp_file: File,
        replacement_xml: &[u8],
    ) -> Result<(), DocxError> {
        let mut source_zip = ZipArchive::new(Cursor::new(self.original_package.as_slice()))?;
        validate_package(&mut source_zip)?;
        let mut output_zip = ZipWriter::new(temp_file);

        for index in 0..source_zip.len() {
            let mut entry = source_zip.by_index(index)?;
            let name = entry.name().to_owned();
            let options = SimpleFileOptions::default().compression_method(if entry.is_dir() {
                CompressionMethod::Stored
            } else {
                entry.compression()
            });
            if entry.is_dir() {
                output_zip.add_directory(name, options)?;
                continue;
            }
            output_zip.start_file(name.clone(), options)?;
            if name == DOCUMENT_XML {
                output_zip.write_all(replacement_xml)?;
            } else {
                std::io::copy(&mut entry, &mut output_zip)?;
            }
        }
        let mut file = output_zip.finish()?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    }
}

fn xml_attribute(event: &BytesStart<'_>, name: &[u8]) -> Option<String> {
    event.attributes().flatten().find_map(|attribute| {
        (attribute.key.as_ref() == name)
            .then(|| {
                attribute
                    .unescape_value()
                    .ok()
                    .map(|value| value.into_owned())
            })
            .flatten()
    })
}

fn image_part_name(target: &str) -> Option<String> {
    let name = if let Some(absolute) = target.strip_prefix('/') {
        absolute.to_owned()
    } else {
        format!("word/{target}")
    };
    let lower = name.to_ascii_lowercase();
    (is_safe_package_part(&name)
        && lower.starts_with("word/media/")
        && [".png", ".jpg", ".jpeg"]
            .iter()
            .any(|extension| lower.ends_with(extension)))
    .then_some(name)
}

fn embedded_images<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    document_xml: &[u8],
) -> Result<Vec<DocxImage>, DocxError> {
    // Missing/malformed relationships must not prevent the safe text-only
    // roundtrip of packages that older NotOffice versions could already open.
    let mut relationships = HashMap::new();
    if let Ok(mut entry) = archive.by_name("word/_rels/document.xml.rels") {
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        let mut reader = Reader::from_reader(bytes.as_slice());
        loop {
            match reader.read_event() {
                Ok(Event::Empty(event) | Event::Start(event))
                    if event.local_name().as_ref() == b"Relationship" =>
                {
                    if xml_attribute(&event, b"TargetMode").as_deref() == Some("External") {
                        continue;
                    }
                    if !xml_attribute(&event, b"Type").is_some_and(|kind| kind.ends_with("/image"))
                    {
                        continue;
                    }
                    if let (Some(id), Some(name)) = (
                        xml_attribute(&event, b"Id"),
                        xml_attribute(&event, b"Target")
                            .and_then(|target| image_part_name(&target)),
                    ) {
                        relationships.insert(id, name);
                    }
                }
                Ok(Event::Eof) | Err(_) => break,
                _ => {}
            }
        }
    }
    let mut references = Vec::new();
    let mut reader = Reader::from_reader(document_xml);
    let mut paragraph_index = 0;
    let mut next_paragraph = 0;
    let mut in_drawing = false;
    let mut extent = None;
    let mut relationship_id = None;
    loop {
        match reader.read_event()? {
            Event::Start(event) if event.name().as_ref() == b"w:p" => {
                paragraph_index = next_paragraph;
                next_paragraph += 1;
            }
            Event::Start(event) if event.name().as_ref() == b"w:drawing" => {
                in_drawing = true;
                extent = None;
                relationship_id = None;
            }
            Event::Empty(event) | Event::Start(event) if in_drawing => {
                if event.name().as_ref() == b"wp:extent" {
                    extent = xml_attribute(&event, b"cx")
                        .and_then(|width| width.parse::<u64>().ok())
                        .zip(
                            xml_attribute(&event, b"cy")
                                .and_then(|height| height.parse::<u64>().ok()),
                        )
                        .filter(|(width, height)| *width > 0 && *height > 0);
                } else if event.name().as_ref() == b"a:blip" {
                    relationship_id = xml_attribute(&event, b"r:embed");
                }
            }
            Event::End(event) if event.name().as_ref() == b"w:drawing" => {
                if let Some(name) = relationship_id
                    .take()
                    .and_then(|id| relationships.get(&id).cloned())
                {
                    references.push((name, paragraph_index, extent));
                }
                in_drawing = false;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut images = Vec::with_capacity(references.len());
    let mut total_bytes = 0;
    for (name, paragraph_index, extent_emu) in references {
        let Ok(mut entry) = archive.by_name(&name) else {
            continue;
        };
        if entry.size() > 12 * 1024 * 1024 {
            return Err(DocxError::ResourceLimit(format!(
                "embedded image {name} exceeds 12 MiB"
            )));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        total_bytes += bytes.len();
        if total_bytes > 48 * 1024 * 1024 {
            return Err(DocxError::ResourceLimit(
                "referenced image budget exceeds 48 MiB".into(),
            ));
        }
        if !bytes.is_empty() {
            images.push(DocxImage {
                bytes,
                paragraph_index,
                extent_emu,
            });
        }
    }
    Ok(images)
}

pub(crate) fn source_fingerprint(path: &Path) -> Result<SourceFingerprint, DocxError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_DOCX_PACKAGE_BYTES {
        return Err(DocxError::ResourceLimit(format!(
            "package is {} bytes; the maximum is {MAX_DOCX_PACKAGE_BYTES}",
            metadata.len()
        )));
    }
    let mut file = File::open(path)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut bytes_read = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes_read += count as u64;
        if bytes_read > MAX_DOCX_PACKAGE_BYTES {
            return Err(DocxError::ResourceLimit(
                "package grew beyond the fingerprint limit".into(),
            ));
        }
        for byte in &buffer[..count] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    if bytes_read != metadata.len() {
        return Err(DocxError::SourceChanged);
    }
    Ok(SourceFingerprint {
        byte_len: metadata.len(),
        modified: metadata.modified().ok(),
        content_hash: hash,
    })
}

fn content_hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

pub(crate) fn paths_refer_to_same_file(left: &Path, right: &Path) -> Result<bool, DocxError> {
    if left == right {
        return Ok(true);
    }
    if !left.exists() || !right.exists() {
        return Ok(false);
    }
    if fs::canonicalize(left)? == fs::canonicalize(right)? {
        return Ok(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        fn identity(path: &Path) -> std::io::Result<(u32, u32, u32)> {
            let file = File::open(path)?;
            let mut info = std::mem::MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::uninit();
            // SAFETY: the file owns a live handle and info points to writable
            // storage of the required type. It is read only after API success.
            if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
                return Err(std::io::Error::last_os_error());
            }
            let info = unsafe { info.assume_init() };
            Ok((
                info.dwVolumeSerialNumber,
                info.nFileIndexHigh,
                info.nFileIndexLow,
            ))
        }
        Ok(identity(left)? == identity(right)?)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let left = fs::metadata(left)?;
        let right = fs::metadata(right)?;
        Ok(left.dev() == right.dev() && left.ino() == right.ino())
    }
    #[cfg(not(any(windows, unix)))]
    Ok(false)
}

fn is_safe_package_part(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('\\')
        && Path::new(name)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn validate_package<R: Read + Seek>(archive: &mut ZipArchive<R>) -> Result<(), DocxError> {
    if archive.len() > MAX_DOCX_PARTS {
        return Err(DocxError::ResourceLimit(format!(
            "package contains more than {MAX_DOCX_PARTS} parts"
        )));
    }
    let mut names = HashSet::with_capacity(archive.len());
    let mut total_uncompressed = 0_u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let name = entry.name();
        if !is_safe_package_part(name) {
            return Err(DocxError::InvalidPackage(format!(
                "unsafe part name {name:?}"
            )));
        }
        if !names.insert(name.to_owned()) {
            return Err(DocxError::InvalidPackage(format!(
                "duplicate part name {name:?}"
            )));
        }
        if entry.size() > MAX_DOCX_PART_BYTES {
            return Err(DocxError::ResourceLimit(format!(
                "part {name:?} exceeds {MAX_DOCX_PART_BYTES} bytes"
            )));
        }
        total_uncompressed = total_uncompressed
            .checked_add(entry.size())
            .ok_or_else(|| DocxError::ResourceLimit("total uncompressed size overflowed".into()))?;
        if total_uncompressed > MAX_DOCX_UNCOMPRESSED_BYTES {
            return Err(DocxError::ResourceLimit(format!(
                "total uncompressed size exceeds {MAX_DOCX_UNCOMPRESSED_BYTES} bytes"
            )));
        }
    }
    if !names.contains(DOCUMENT_XML) {
        return Err(DocxError::MissingDocumentXml);
    }
    Ok(())
}

fn verify_rebuilt_package(temp_path: &Path, expected_xml: &[u8]) -> Result<(), DocxError> {
    let file = File::open(temp_path)?;
    let mut archive = ZipArchive::new(file)?;
    validate_package(&mut archive)?;
    let mut document_xml = Vec::new();
    archive
        .by_name(DOCUMENT_XML)?
        .read_to_end(&mut document_xml)?;
    if document_xml != expected_xml {
        return Err(DocxError::UnsafeSave(
            "rebuilt word/document.xml differs from the validated replacement".into(),
        ));
    }
    parse_document_xml(&document_xml)?;
    Ok(())
}

pub(crate) fn create_temporary_peer(destination: &Path) -> Result<(PathBuf, File), DocxError> {
    let file_name = destination
        .file_name()
        .unwrap_or_else(|| OsStr::new("document.docx"));
    for attempt in 0..TEMP_CREATE_ATTEMPTS {
        let mut temporary_name = OsString::from(".");
        temporary_name.push(file_name);
        temporary_name.push(format!(
            ".liteoffice-v071-{}-{attempt}.tmp",
            std::process::id()
        ));
        let temporary_path = destination.with_file_name(temporary_name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
        {
            Ok(file) => return Ok((temporary_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(DocxError::Io(error)),
        }
    }
    Err(DocxError::UnsafeSave(format!(
        "could not create a unique temporary file beside {}",
        destination.display()
    )))
}

fn available_backup_path(destination: &Path) -> std::io::Result<PathBuf> {
    let extension = destination
        .extension()
        .unwrap_or_else(|| OsStr::new("docx"))
        .to_string_lossy();
    let base = destination.with_extension(format!("{extension}.bak"));
    if !base.exists() {
        return Ok(base);
    }
    for suffix in 1_u32..=1_000 {
        let mut file_name = base
            .file_name()
            .unwrap_or_else(|| OsStr::new("document.docx.bak"))
            .to_os_string();
        file_name.push(format!(".{suffix}"));
        let candidate = base.with_file_name(file_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "too many existing NotOffice backup files",
    ))
}

#[cfg(not(windows))]
pub(crate) fn replace_destination(temp_path: &Path, destination: &Path) -> std::io::Result<()> {
    if destination.exists() {
        fs::copy(destination, available_backup_path(destination)?)?;
    }
    fs::rename(temp_path, destination)
}

/// Windows does not let `std::fs::rename` replace an existing file.  ReplaceFileW
/// performs that operation as one filesystem action and leaves the former file as
/// the requested backup, so there is never a point where a successful save has
/// deleted the old document before the new package is in place.
#[cfg(windows)]
pub(crate) fn replace_destination(temp_path: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    if !destination.exists() {
        return fs::rename(temp_path, destination);
    }
    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
    let replaced = wide(destination);
    let replacement = wide(temp_path);
    let backup_path = available_backup_path(destination)?;
    let backup = wide(&backup_path);
    let result = unsafe {
        ReplaceFileW(
            replaced.as_ptr(),
            replacement.as_ptr(),
            backup.as_ptr(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn is_name(event_name: &[u8], expected: &[u8]) -> bool {
    event_name == expected
}

fn twips_width(event: &BytesStart<'_>) -> Option<u32> {
    event.attributes().flatten().find_map(|attribute| {
        (attribute.key.as_ref() == b"w:w")
            .then(|| {
                std::str::from_utf8(attribute.value.as_ref())
                    .ok()?
                    .parse()
                    .ok()
            })
            .flatten()
    })
}

fn is_page_break(event: &BytesStart<'_>) -> bool {
    if is_name(event.name().as_ref(), b"w:lastRenderedPageBreak") {
        return true;
    }
    is_name(event.name().as_ref(), b"w:br")
        && event.attributes().flatten().any(|attribute| {
            attribute.key.as_ref() == b"w:type" && attribute.value.as_ref() == b"page"
        })
}

fn paragraph_alignment(event: &BytesStart<'_>) -> Option<ParagraphAlignment> {
    if !is_name(event.name().as_ref(), b"w:jc") {
        return None;
    }
    event.attributes().flatten().find_map(|attribute| {
        (attribute.key.as_ref() == b"w:val").then(|| {
            if attribute.value.as_ref() == b"center" {
                ParagraphAlignment::Center
            } else {
                ParagraphAlignment::Left
            }
        })
    })
}

fn font_size_attr(event: &BytesStart<'_>) -> Option<u16> {
    event.attributes().flatten().find_map(|a| {
        (a.key.as_ref() == b"w:val")
            .then(|| {
                std::str::from_utf8(a.value.as_ref())
                    .ok()?
                    .parse::<u16>()
                    .ok()
            })
            .flatten()
    })
}

pub fn parse_document_xml(xml: &[u8]) -> Result<DocumentModel, DocxError> {
    let mut reader = Reader::from_reader(Cursor::new(xml));
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut paragraphs = Vec::new();
    let mut blocks = Vec::new();
    let mut current_paragraph: Option<Paragraph> = None;
    let mut table_depth: usize = 0;
    let mut table_rows = Vec::new();
    let mut table_grid_widths = Vec::new();
    let mut current_table_row: Option<TableRow> = None;
    let mut current_table_cell: Option<Vec<usize>> = None;
    let mut in_text = false;
    let mut next_id = 0;
    let mut next_run_id = 0;
    let mut current_run_id = 0;
    let mut current_run_size: Option<u16> = None;
    let mut run_fragment_ids: Vec<usize> = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if is_name(event.name().as_ref(), b"w:tbl") => {
                if table_depth == 0 {
                    table_rows.clear();
                    table_grid_widths.clear();
                }
                table_depth += 1;
            }
            Event::Empty(event)
                if table_depth > 0 && is_name(event.name().as_ref(), b"w:gridCol") =>
            {
                if let Some(width) = twips_width(&event) {
                    table_grid_widths.push(width);
                }
            }
            Event::Empty(event) if is_name(event.name().as_ref(), b"w:t") => {
                let paragraph = current_paragraph
                    .as_mut()
                    .ok_or_else(|| DocxError::InvalidDocumentXml("w:t outside w:p".into()))?;
                paragraph.fragments.push(TextFragment {
                    id: next_id,
                    text: String::new(),
                    run_id: current_run_id,
                    font_size_half_points: current_run_size,
                });
                run_fragment_ids.push(next_id);
                next_id += 1;
            }
            Event::Empty(event) if is_name(event.name().as_ref(), b"w:br") => {
                if is_page_break(&event) {
                    if let Some(paragraph) = current_paragraph.as_mut() {
                        paragraph.page_break_after = true;
                    }
                } else if let Some(paragraph) = current_paragraph.as_mut() {
                    if let Some(fragment) = paragraph.fragments.last_mut() {
                        fragment.text.push('\n');
                    } else {
                        paragraph.fragments.push(TextFragment {
                            id: next_id,
                            text: "\n".into(),
                            run_id: current_run_id,
                            font_size_half_points: current_run_size,
                        });
                        run_fragment_ids.push(next_id);
                        next_id += 1;
                    }
                }
            }
            Event::Empty(event) if is_name(event.name().as_ref(), b"w:sz") => {
                current_run_size = font_size_attr(&event).or(current_run_size);
            }
            Event::Empty(event) => {
                if let (Some(paragraph), Some(alignment)) =
                    (current_paragraph.as_mut(), paragraph_alignment(&event))
                {
                    paragraph.alignment = alignment;
                }
            }
            Event::Start(event) if is_name(event.name().as_ref(), b"w:r") => {
                current_run_id = next_run_id;
                next_run_id += 1;
                current_run_size = None;
                run_fragment_ids.clear();
            }
            Event::Start(event) if is_name(event.name().as_ref(), b"w:sz") => {
                current_run_size = font_size_attr(&event).or(current_run_size);
            }
            Event::End(event) if is_name(event.name().as_ref(), b"w:r") => {
                for fragment_id in run_fragment_ids.drain(..) {
                    if let Some(p) = current_paragraph.as_mut() {
                        if let Some(f) = p.fragments.iter_mut().find(|f| f.id == fragment_id) {
                            f.font_size_half_points = current_run_size;
                        }
                    }
                }
                current_run_size = None;
            }
            Event::Start(event) if table_depth > 0 && is_name(event.name().as_ref(), b"w:tr") => {
                current_table_row = Some(TableRow::default());
            }
            Event::Start(event) if table_depth > 0 && is_name(event.name().as_ref(), b"w:tc") => {
                current_table_cell = Some(Vec::new());
            }
            Event::Start(event) if is_name(event.name().as_ref(), b"w:p") => {
                current_paragraph = Some(Paragraph::default())
            }
            Event::End(event) if is_name(event.name().as_ref(), b"w:p") => {
                if let Some(paragraph) = current_paragraph.take() {
                    let index = paragraphs.len();
                    paragraphs.push(paragraph);
                    if let Some(cell) = current_table_cell.as_mut() {
                        cell.push(index);
                    } else {
                        blocks.push(DocumentBlock::Paragraph(index));
                    }
                }
            }
            Event::End(event) if table_depth > 0 && is_name(event.name().as_ref(), b"w:tc") => {
                if let (Some(row), Some(cell)) =
                    (current_table_row.as_mut(), current_table_cell.take())
                {
                    row.cells.push(cell);
                }
            }
            Event::End(event) if table_depth > 0 && is_name(event.name().as_ref(), b"w:tr") => {
                if let Some(row) = current_table_row.take() {
                    table_rows.push(row);
                }
            }
            Event::End(event) if is_name(event.name().as_ref(), b"w:tbl") => {
                table_depth = table_depth.saturating_sub(1);
                if table_depth == 0 {
                    blocks.push(DocumentBlock::Table {
                        rows: std::mem::take(&mut table_rows),
                        grid_widths: std::mem::take(&mut table_grid_widths),
                    });
                }
            }
            Event::Start(event) if is_name(event.name().as_ref(), b"w:t") => {
                let paragraph = current_paragraph
                    .as_mut()
                    .ok_or_else(|| DocxError::InvalidDocumentXml("w:t outside w:p".into()))?;
                paragraph.fragments.push(TextFragment {
                    id: next_id,
                    text: String::new(),
                    run_id: current_run_id,
                    font_size_half_points: current_run_size,
                });
                run_fragment_ids.push(next_id);
                next_id += 1;
                in_text = true;
            }
            Event::End(event) if is_name(event.name().as_ref(), b"w:t") => in_text = false,
            Event::Text(text) if in_text => {
                let paragraph = current_paragraph
                    .as_mut()
                    .ok_or_else(|| DocxError::InvalidDocumentXml("w:t outside w:p".into()))?;
                let value = text
                    .unescape()
                    .map_err(|error| DocxError::InvalidDocumentXml(error.to_string()))?
                    .into_owned();
                if let Some(fragment) = paragraph.fragments.last_mut() {
                    fragment.text.push_str(&value);
                }
            }
            Event::CData(text) if in_text => {
                let value = std::str::from_utf8(text.as_ref())
                    .map_err(|error| DocxError::InvalidDocumentXml(error.to_string()))?;
                if let Some(fragment) = current_paragraph
                    .as_mut()
                    .and_then(|paragraph| paragraph.fragments.last_mut())
                {
                    fragment.text.push_str(value);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if paragraphs.is_empty() && next_id == 0 {
        return Err(DocxError::InvalidDocumentXml(
            "no editable paragraphs or text fragments found".into(),
        ));
    }
    Ok(DocumentModel { paragraphs, blocks })
}

pub fn rewrite_text_nodes(
    original_xml: &[u8],
    model: &DocumentModel,
) -> Result<Vec<u8>, DocxError> {
    let replacements = model.texts_in_xml_order();
    let mut reader = Reader::from_reader(Cursor::new(original_xml));
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::new());
    let mut buffer = Vec::new();
    let mut in_text = false;
    let mut skip_end = false;
    let mut replacement_index = 0;

    loop {
        let event = reader.read_event_into(&mut buffer)?;
        match event {
            Event::Start(ref start) if is_name(start.name().as_ref(), b"w:t") => {
                in_text = true;
                skip_end = true;
                let replacement = replacements.get(replacement_index).ok_or_else(|| {
                    DocxError::InvalidDocumentXml("model contains fewer fragments than XML".into())
                })?;
                write_text_with_breaks(&mut writer, start, replacement)?;
                replacement_index += 1;
            }
            Event::Empty(ref start) if is_name(start.name().as_ref(), b"w:t") => {
                let replacement = replacements.get(replacement_index).ok_or_else(|| {
                    DocxError::InvalidDocumentXml("model contains fewer fragments than XML".into())
                })?;
                write_text_with_breaks(&mut writer, start, replacement)?;
                replacement_index += 1;
            }
            Event::End(ref end) if is_name(end.name().as_ref(), b"w:t") => {
                in_text = false;
                if skip_end {
                    skip_end = false;
                } else {
                    writer.write_event(event)?;
                }
            }
            Event::Empty(ref start)
                if is_name(start.name().as_ref(), b"w:br") && !is_page_break(start) => {}
            Event::Text(_) | Event::CData(_) if in_text => {}
            Event::Eof => break,
            other => writer.write_event(other)?,
        }
        buffer.clear();
    }
    if replacement_index != replacements.len() {
        return Err(DocxError::InvalidDocumentXml(
            "model contains more fragments than XML".into(),
        ));
    }
    Ok(writer.into_inner())
}

fn write_text_with_breaks(
    writer: &mut Writer<Vec<u8>>,
    start: &BytesStart<'_>,
    replacement: &str,
) -> Result<(), DocxError> {
    let mut parts = replacement.split('\n').peekable();
    let first = parts.next().unwrap_or_default();
    let node = text_node_start(start, first)?;
    writer.write_event(Event::Start(node))?;
    writer.write_event(Event::Text(BytesText::new(first)))?;
    writer.write_event(Event::End(BytesStart::new("w:t").to_end()))?;
    for part in parts {
        writer.write_event(Event::Empty(BytesStart::new("w:br")))?;
        if part.is_empty() {
            continue;
        }
        let node = text_node_start(start, part)?;
        writer.write_event(Event::Start(node))?;
        writer.write_event(Event::Text(BytesText::new(part)))?;
        writer.write_event(Event::End(BytesStart::new("w:t").to_end()))?;
    }
    Ok(())
}

fn text_node_start<'a>(
    start: &BytesStart<'a>,
    replacement: &str,
) -> Result<BytesStart<'static>, DocxError> {
    let mut updated = start.to_owned();
    // Word may otherwise strip spaces newly introduced by an edit/replacement.
    if replacement.starts_with(char::is_whitespace) || replacement.ends_with(char::is_whitespace) {
        updated.clear_attributes();
        for attribute in start.attributes() {
            let attribute =
                attribute.map_err(|error| DocxError::InvalidDocumentXml(error.to_string()))?;
            if attribute.key.as_ref() != b"xml:space" {
                updated.push_attribute(attribute);
            }
        }
        updated.push_attribute(("xml:space", "preserve"));
    }
    Ok(updated)
}

/// Rewrites text and only those run properties explicitly changed by the editor.
/// A run may contain several w:t elements. The whole run receives one size.
pub fn rewrite_document_xml(
    original_xml: &[u8],
    model: &DocumentModel,
) -> Result<Vec<u8>, DocxError> {
    let text_xml = rewrite_text_nodes(original_xml, model)?;
    let original_model = parse_document_xml(original_xml)?;
    rewrite_run_font_sizes(&text_xml, model, &original_model)
}

fn rewrite_run_font_sizes(
    xml: &[u8],
    model: &DocumentModel,
    original_model: &DocumentModel,
) -> Result<Vec<u8>, DocxError> {
    let mut target_sizes = std::collections::HashMap::new();
    let originals: std::collections::HashMap<usize, Option<u16>> = original_model
        .paragraphs
        .iter()
        .flat_map(|p| &p.fragments)
        .map(|fragment| (fragment.run_id, fragment.font_size_half_points))
        .collect();
    for fragment in model.paragraphs.iter().flat_map(|p| &p.fragments) {
        if let Some(size) = fragment.font_size_half_points {
            if originals.get(&fragment.run_id) == Some(&Some(size)) {
                continue;
            }
            if !(12..=144).contains(&size) {
                return Err(DocxError::UnsafeSave("font size must be 6–72 pt".into()));
            }
            if let Some(previous) = target_sizes.insert(fragment.run_id, size) {
                if previous != size {
                    return Err(DocxError::UnsafeSave(
                        "conflicting font sizes within one run".into(),
                    ));
                }
            }
        }
    }
    let mut reader = Reader::from_reader(Cursor::new(xml));
    reader.config_mut().trim_text(false);
    let mut writer = Writer::new(Vec::new());
    let mut buf = Vec::new();
    let mut run_index = 0usize;
    loop {
        let event = reader.read_event_into(&mut buf)?;
        match event {
            Event::Start(ref start) if is_name(start.name().as_ref(), b"w:r") => {
                let target = target_sizes.get(&run_index).copied();
                run_index += 1;
                writer.write_event(event)?;
                if let Some(size) = target {
                    // Collect all contents until the run's closing tag; OOXML runs do not nest.
                    let mut contents = Vec::new();
                    let mut depth = 1usize;
                    loop {
                        let item = reader.read_event_into(&mut buf)?.into_owned();
                        match &item {
                            Event::Start(e) if is_name(e.name().as_ref(), b"w:r") => depth += 1,
                            Event::End(e) if is_name(e.name().as_ref(), b"w:r") => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            Event::Eof => {
                                return Err(DocxError::InvalidDocumentXml(
                                    "unterminated w:r".into(),
                                ))
                            }
                            _ => {}
                        }
                        contents.push(item);
                        buf.clear();
                    }
                    write_sized_run(&mut writer, contents, size)?;
                    writer.write_event(Event::End(quick_xml::events::BytesEnd::new("w:r")))?;
                }
            }
            Event::Eof => break,
            other => writer.write_event(other)?,
        }
        buf.clear();
    }
    Ok(writer.into_inner())
}

fn write_sized_run(
    writer: &mut Writer<Vec<u8>>,
    events: Vec<Event<'static>>,
    size: u16,
) -> Result<(), DocxError> {
    let mut has_rpr = false;
    let mut inside_rpr = false;
    let mut has_sz = false;
    let value = size.to_string();
    for event in events.iter() {
        match event {
            Event::Start(e) if is_name(e.name().as_ref(), b"w:rPr") => has_rpr = true,
            Event::Empty(e) if is_name(e.name().as_ref(), b"w:rPr") => has_rpr = true,
            _ => {}
        }
    }
    if !has_rpr {
        writer.write_event(Event::Start(BytesStart::new("w:rPr")))?;
        let mut sz = BytesStart::new("w:sz");
        sz.push_attribute(("w:val", value.as_str()));
        writer.write_event(Event::Empty(sz))?;
        writer.write_event(Event::End(quick_xml::events::BytesEnd::new("w:rPr")))?;
    }
    for event in events {
        match event {
            Event::Start(ref e) if is_name(e.name().as_ref(), b"w:rPr") => {
                inside_rpr = true;
                has_sz = false;
                writer.write_event(event)?;
            }
            Event::Empty(ref e) if is_name(e.name().as_ref(), b"w:rPr") => {
                writer.write_event(Event::Start(BytesStart::new("w:rPr")))?;
                let mut sz = BytesStart::new("w:sz");
                sz.push_attribute(("w:val", value.as_str()));
                writer.write_event(Event::Empty(sz))?;
                writer.write_event(Event::End(quick_xml::events::BytesEnd::new("w:rPr")))?;
            }
            Event::Empty(ref e) if inside_rpr && is_name(e.name().as_ref(), b"w:sz") => {
                let mut updated = e.to_owned();
                updated.clear_attributes();
                for a in e.attributes().flatten() {
                    if a.key.as_ref() != b"w:val" {
                        updated.push_attribute((a.key.as_ref(), a.value.as_ref()));
                    }
                }
                updated.push_attribute(("w:val", value.as_str()));
                writer.write_event(Event::Empty(updated))?;
                has_sz = true;
            }
            Event::Start(ref e) if inside_rpr && is_name(e.name().as_ref(), b"w:sz") => {
                return Err(DocxError::UnsafeSave(
                    "non-empty w:sz element is unsupported".into(),
                ));
            }
            Event::End(ref e) if is_name(e.name().as_ref(), b"w:rPr") => {
                if !has_sz {
                    let mut sz = BytesStart::new("w:sz");
                    sz.push_attribute(("w:val", value.as_str()));
                    writer.write_event(Event::Empty(sz))?;
                }
                inside_rpr = false;
                writer.write_event(event)?;
            }
            other => writer.write_event(other)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t xml:space="preserve">Hello </w:t></w:r><w:r><w:t>world</w:t></w:r></w:p></w:body></w:document>"#;

    fn fixture(path: &Path) {
        fixture_with_parts(path, XML.as_bytes(), &[]);
    }

    fn fixture_with_parts(path: &Path, document_xml: &[u8], extra_parts: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = ZipWriter::new(file);
        zip.start_file("[Content_Types].xml", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"<Types/>").unwrap();
        zip.start_file(DOCUMENT_XML, SimpleFileOptions::default())
            .unwrap();
        zip.write_all(document_xml).unwrap();
        zip.start_file("word/media/keep.bin", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&[1, 2, 3, 4]).unwrap();
        for (name, bytes) in extra_parts {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn parses_fragments_in_xml_order() {
        let model = parse_document_xml(XML.as_bytes()).unwrap();
        assert_eq!(model.paragraphs[0].fragments[0].text, "Hello ");
        assert_eq!(model.paragraphs[0].fragments[1].text, "world");
        assert_eq!(model.blocks, vec![DocumentBlock::Paragraph(0)]);
    }

    #[test]
    fn keeps_table_cell_boundaries_for_display() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:p><w:r><w:t>Left</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Right</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
        let model = parse_document_xml(xml.as_bytes()).unwrap();
        assert_eq!(
            model.blocks,
            vec![DocumentBlock::Table {
                rows: vec![TableRow {
                    cells: vec![vec![0], vec![1]]
                }],
                grid_widths: vec![],
            }]
        );
    }

    #[test]
    fn reads_table_grid_column_widths() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>Left</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Right</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
        let model = parse_document_xml(xml.as_bytes()).unwrap();
        assert!(matches!(
            &model.blocks[0],
            DocumentBlock::Table { grid_widths, .. } if grid_widths == &[2000, 4000]
        ));
    }

    #[test]
    fn keeps_explicit_page_breaks_for_display() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>First</w:t><w:br w:type="page"/></w:r></w:p><w:p><w:r><w:t>Second</w:t></w:r></w:p></w:body></w:document>"#;
        let model = parse_document_xml(xml.as_bytes()).unwrap();
        assert!(model.paragraphs[0].page_break_after);
        assert!(!model.paragraphs[1].page_break_after);
    }

    #[test]
    fn keeps_centered_paragraph_alignment() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:jc w:val="center"/></w:pPr><w:r><w:t>Title</w:t></w:r></w:p></w:body></w:document>"#;
        let model = parse_document_xml(xml.as_bytes()).unwrap();
        assert_eq!(model.paragraphs[0].alignment, ParagraphAlignment::Center);
    }

    #[test]
    fn rewrites_text_without_damaging_attributes() {
        let mut model = parse_document_xml(XML.as_bytes()).unwrap();
        model.fragment_mut(1).unwrap().text = "XML & ZIP".into();
        let rewritten = rewrite_text_nodes(XML.as_bytes(), &model).unwrap();
        let text = String::from_utf8(rewritten).unwrap();
        assert!(text.contains("xml:space=\"preserve\""));
        assert!(text.contains("XML &amp; ZIP"));
        assert_eq!(parse_document_xml(text.as_bytes()).unwrap(), model);
    }

    #[test]
    fn deleted_text_nodes_keep_their_identity_after_save_and_reopen() {
        let mut model = parse_document_xml(XML.as_bytes()).unwrap();
        model.fragment_mut(0).unwrap().text.clear();
        let rewritten = rewrite_document_xml(XML.as_bytes(), &model).unwrap();
        assert_eq!(parse_document_xml(&rewritten).unwrap(), model);
    }

    #[test]
    fn empty_text_nodes_can_be_edited_without_shifting_later_fragment_ids() {
        let xml = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t/><w:t></w:t><w:t>Third</w:t></w:r></w:p></w:body></w:document>"#;
        let mut model = parse_document_xml(xml).unwrap();
        assert_eq!(model.paragraphs[0].fragments.len(), 3);
        model.fragment_mut(0).unwrap().text = " First ".into();
        model.fragment_mut(1).unwrap().text = "Second".into();
        let rewritten = rewrite_document_xml(xml, &model).unwrap();
        assert_eq!(parse_document_xml(&rewritten).unwrap(), model);
        assert!(String::from_utf8(rewritten)
            .unwrap()
            .contains("xml:space=\"preserve\""));
    }

    #[test]
    fn newly_inserted_edge_spaces_are_preserved_for_word_readers() {
        let xml = XML.replace(" xml:space=\"preserve\"", "");
        let mut model = parse_document_xml(xml.as_bytes()).unwrap();
        model.fragment_mut(1).unwrap().text = " replacement ".into();
        let rewritten = rewrite_document_xml(xml.as_bytes(), &model).unwrap();
        let output = String::from_utf8(rewritten).unwrap();
        assert!(output.contains("<w:t xml:space=\"preserve\"> replacement </w:t>"));
        assert_eq!(parse_document_xml(output.as_bytes()).unwrap(), model);
    }

    #[test]
    fn reads_explicit_run_size_and_updates_it() {
        let xml = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:rPr><w:b/><w:sz w:val="24"/></w:rPr><w:t>Hello</w:t><w:t>world</w:t></w:r></w:p></w:body></w:document>"#;
        let mut model = parse_document_xml(xml).unwrap();
        assert_eq!(
            model.paragraphs[0].fragments[0].font_size_half_points,
            Some(24)
        );
        assert_eq!(
            model.paragraphs[0].fragments[1].font_size_half_points,
            Some(24)
        );
        assert!(model.set_run_font_size(0, 36));
        assert_eq!(
            model.paragraphs[0].fragments[1].font_size_half_points,
            Some(36)
        );
        let rewritten = rewrite_document_xml(xml, &model).unwrap();
        let output = String::from_utf8(rewritten.clone()).unwrap();
        assert!(output.contains("<w:b/>") || output.contains("<w:b />"));
        assert!(output.contains("w:sz w:val=\"36\""));
        let roundtrip = parse_document_xml(&rewritten).unwrap();
        assert_eq!(
            roundtrip.paragraphs[0].fragments[0].font_size_half_points,
            Some(36)
        );
    }

    #[test]
    fn inserts_missing_run_properties_without_touching_unicode() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Русский текст ✓</w:t></w:r></w:p></w:body></w:document>"#;
        let mut model = parse_document_xml(xml.as_bytes()).unwrap();
        assert!(model.set_run_font_size(0, 28));
        let rewritten = rewrite_document_xml(xml.as_bytes(), &model).unwrap();
        let roundtrip = parse_document_xml(&rewritten).unwrap();
        assert_eq!(roundtrip.paragraphs[0].fragments[0].text, "Русский текст ✓");
        assert_eq!(
            roundtrip.paragraphs[0].fragments[0].font_size_half_points,
            Some(28)
        );
    }

    #[test]
    fn safe_save_keeps_package_parts_and_writes_backup() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("input.docx");
        let output = folder.path().join("output.docx");
        fixture(&input);
        fixture(&output);
        let mut package = DocxPackage::open(&input).unwrap();
        package.model.fragment_mut(0).unwrap().text = "Updated ".into();
        package.save_as(&output).unwrap();
        let reloaded = DocxPackage::open(&output).unwrap();
        assert_eq!(reloaded.model.paragraphs[0].fragments[0].text, "Updated ");
        let mut archive = ZipArchive::new(File::open(&output).unwrap()).unwrap();
        let mut media = Vec::new();
        archive
            .by_name("word/media/keep.bin")
            .unwrap()
            .read_to_end(&mut media)
            .unwrap();
        assert_eq!(media, [1, 2, 3, 4]);
        assert!(output.with_extension("docx.bak").exists());
    }

    #[test]
    fn opens_supported_embedded_raster_for_printing() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("image.docx");
        let png = include_bytes!("officeart_test.png");
        let drawing = r#"<w:p><w:r><w:drawing><wp:inline><wp:extent cx="914400" cy="457200"/><a:graphic><a:blip r:embed="image"/></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
        let xml = format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><w:body><w:p><w:r><w:t>Before</w:t></w:r></w:p>{drawing}{drawing}</w:body></w:document>"#
        );
        let rels = br#"<Relationships><Relationship Id="image" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#;
        fixture_with_parts(
            &input,
            xml.as_bytes(),
            &[
                ("word/media/image1.png", png),
                ("word/media/unused.png", png),
                ("word/_rels/document.xml.rels", rels),
            ],
        );

        let package = DocxPackage::open(&input).unwrap();
        assert_eq!(
            package.images,
            vec![
                DocxImage {
                    bytes: png.to_vec(),
                    paragraph_index: 1,
                    extent_emu: Some((914400, 457200))
                },
                DocxImage {
                    bytes: png.to_vec(),
                    paragraph_index: 2,
                    extent_emu: Some((914400, 457200))
                }
            ]
        );
    }

    #[test]
    fn image_relationships_cannot_escape_the_docx_package() {
        assert_eq!(
            image_part_name("media/picture.png").as_deref(),
            Some("word/media/picture.png")
        );
        assert_eq!(
            image_part_name("/word/media/picture.jpg").as_deref(),
            Some("word/media/picture.jpg")
        );
        for path in [
            "../../private.png",
            "media/../../../private.png",
            "https://example.test/image.png",
            "C:\\private.png",
            "media/drawing.svg",
        ] {
            assert!(
                image_part_name(path).is_none(),
                "accepted unsafe/unsupported path {path}"
            );
        }
    }

    #[test]
    fn roundtrip_edit_preserves_unknown_parts_and_malformed_relationships() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("input.docx");
        let output = folder.path().join("output.docx");
        let relationship = b"this is deliberately not XML";
        let unknown_part = b"opaque custom payload";
        fixture_with_parts(
            &input,
            XML.as_bytes(),
            &[
                ("word/_rels/document.xml.rels", relationship.as_slice()),
                ("word/custom/unknown.bin", unknown_part.as_slice()),
            ],
        );

        let mut package = DocxPackage::open(&input).unwrap();
        package.model.fragment_mut(1).unwrap().text = "changed ✓".into();
        package.save_as(&output).unwrap();

        let reopened = DocxPackage::open(&output).unwrap();
        assert_eq!(reopened.model.fragment(1).unwrap().text, "changed ✓");
        let mut archive = ZipArchive::new(File::open(&output).unwrap()).unwrap();
        for (name, expected) in [
            ("word/_rels/document.xml.rels", relationship.as_slice()),
            ("word/custom/unknown.bin", unknown_part.as_slice()),
        ] {
            let mut actual = Vec::new();
            archive
                .by_name(name)
                .unwrap()
                .read_to_end(&mut actual)
                .unwrap();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn missing_relationships_do_not_block_a_safe_roundtrip() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("input.docx");
        let output = folder.path().join("output.docx");
        fixture(&input);

        let mut package = DocxPackage::open(&input).unwrap();
        package.model.fragment_mut(0).unwrap().text = "Updated ".into();
        package.save_as(&output).unwrap();

        assert_eq!(
            DocxPackage::open(&output)
                .unwrap()
                .model
                .fragment(0)
                .unwrap()
                .text,
            "Updated "
        );
    }

    #[test]
    fn external_change_cancels_overwrite_and_keeps_original_bytes() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("input.docx");
        fixture(&input);
        let package = DocxPackage::open(&input).unwrap();
        let external_xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>External change</w:t></w:r></w:p></w:body></w:document>"#;
        fixture_with_parts(&input, external_xml.as_bytes(), &[]);
        let externally_changed_bytes = fs::read(&input).unwrap();

        assert!(matches!(
            package.save_as(&input),
            Err(DocxError::SourceChanged)
        ));
        assert_eq!(fs::read(&input).unwrap(), externally_changed_bytes);
        assert!(!input.with_extension("docx.bak").exists());
    }

    #[test]
    fn repeated_saves_of_our_own_changes_do_not_look_like_an_external_conflict() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("input.docx");
        fixture(&input);
        let mut package = DocxPackage::open(&input).unwrap();
        package.model.fragment_mut(0).unwrap().text = "First save ".into();
        package.save(&input).unwrap();
        package.model.fragment_mut(0).unwrap().text = "Second save ".into();
        package.save(&input).unwrap();
        assert_eq!(
            DocxPackage::open(&input)
                .unwrap()
                .model
                .fragment(0)
                .unwrap()
                .text,
            "Second save "
        );
    }

    #[test]
    fn saving_a_copy_after_an_external_change_preserves_the_opened_package_parts() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("input.docx");
        let output = folder.path().join("my-copy.docx");
        fixture_with_parts(
            &input,
            XML.as_bytes(),
            &[("word/custom/state.bin", b"opened state")],
        );
        let mut package = DocxPackage::open(&input).unwrap();
        package.model.fragment_mut(0).unwrap().text = "My unsaved edit ".into();
        fixture_with_parts(
            &input,
            XML.as_bytes(),
            &[("word/custom/state.bin", b"external state")],
        );
        let external = fs::read(&input).unwrap();
        package.save_as(&output).unwrap();
        assert_eq!(fs::read(&input).unwrap(), external);
        let mut archive = ZipArchive::new(File::open(&output).unwrap()).unwrap();
        let mut content = Vec::new();
        archive
            .by_name("word/custom/state.bin")
            .unwrap()
            .read_to_end(&mut content)
            .unwrap();
        assert_eq!(content, b"opened state");
    }

    #[test]
    fn save_copy_still_works_after_the_original_file_is_removed() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("removed.docx");
        let copy = folder.path().join("copy.docx");
        fixture(&input);
        fixture(&copy);
        let mut package = DocxPackage::open(&input).unwrap();
        package.model.fragment_mut(0).unwrap().text = "Still available in memory ".into();
        fs::remove_file(&input).unwrap();
        package.save(&copy).unwrap();
        assert!(!input.exists());
        assert_eq!(package.source_path(), copy);
        assert_eq!(
            DocxPackage::open(&copy)
                .unwrap()
                .model
                .fragment(0)
                .unwrap()
                .text,
            "Still available in memory "
        );
        assert!(copy.with_extension("docx.bak").exists());
    }

    #[test]
    fn write_error_leaves_the_source_document_untouched() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("input.docx");
        fixture(&input);
        let original = fs::read(&input).unwrap();
        let package = DocxPackage::open(&input).unwrap();
        let output = folder.path().join("missing-parent").join("output.docx");

        assert!(package.save_as(&output).is_err());
        assert_eq!(fs::read(&input).unwrap(), original);
    }

    #[test]
    fn save_does_not_overwrite_an_existing_backup() {
        let folder = tempfile::tempdir().unwrap();
        let input = folder.path().join("input.docx");
        let output = folder.path().join("output.docx");
        fixture(&input);
        fixture(&output);
        let backup = output.with_extension("docx.bak");
        fs::write(&backup, b"older backup remains intact").unwrap();

        let mut package = DocxPackage::open(&input).unwrap();
        package.model.fragment_mut(0).unwrap().text = "Updated ".into();
        package.save_as(&output).unwrap();

        assert_eq!(fs::read(&backup).unwrap(), b"older backup remains intact");
        assert!(backup.with_file_name("output.docx.bak.1").exists());
    }

    #[test]
    fn corrupt_zip_and_invalid_document_xml_are_rejected() {
        let folder = tempfile::tempdir().unwrap();
        let corrupt = folder.path().join("corrupt.docx");
        fs::write(&corrupt, b"not a ZIP").unwrap();
        assert!(DocxPackage::open(&corrupt).is_err());

        let invalid_xml = folder.path().join("invalid-xml.docx");
        fixture_with_parts(&invalid_xml, b"<w:document><w:p>", &[]);
        let error = match DocxPackage::open(&invalid_xml) {
            Ok(_) => panic!("invalid XML unexpectedly opened"),
            Err(error) => error,
        };
        assert!(
            matches!(error, DocxError::Xml(_) | DocxError::InvalidDocumentXml(_)),
            "unexpected error: {error:?}"
        );
    }

    #[test]
    fn unsafe_package_paths_are_rejected_before_copying() {
        assert!(!is_safe_package_part("../outside.xml"));
        assert!(!is_safe_package_part("word\\document.xml"));
        assert!(!is_safe_package_part("/absolute.xml"));
        assert!(is_safe_package_part("word/document.xml"));
    }

    #[test]
    fn package_file_size_limit_is_checked_before_zip_parsing() {
        let folder = tempfile::tempdir().unwrap();
        let oversized = folder.path().join("oversized.docx");
        let file = File::create(&oversized).unwrap();
        file.set_len(MAX_DOCX_PACKAGE_BYTES + 1).unwrap();
        drop(file);

        assert!(matches!(
            DocxPackage::open(&oversized),
            Err(DocxError::ResourceLimit(_))
        ));
    }

    #[test]
    fn bulk_font_change_including_table_runs_roundtrips() {
        let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Русский ✓</w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:rPr><w:sz w:val="20"/></w:rPr><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
        let mut model = parse_document_xml(xml.as_bytes()).unwrap();
        assert!(model.set_all_run_font_sizes(36));
        assert!(!model.set_all_run_font_sizes(36));
        let rewritten = rewrite_document_xml(xml.as_bytes(), &model).unwrap();
        let reparsed = parse_document_xml(&rewritten).unwrap();
        assert_eq!(reparsed.paragraphs[0].fragments[0].text, "Русский ✓");
        assert!(reparsed
            .paragraphs
            .iter()
            .flat_map(|p| p.fragments.iter())
            .all(|f| f.font_size_half_points == Some(36)));
        assert_eq!(reparsed.common_font_size(), Some(36));
    }
    #[test]
    fn line_breaks_are_parsed_rewritten_and_round_trip_as_text() {
        let xml = br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>One</w:t><w:br/><w:t>Two</w:t><w:br/><w:t>Three</w:t></w:r></w:p></w:body></w:document>"#;
        let model = parse_document_xml(xml).unwrap();
        let text = model
            .paragraphs
            .iter()
            .flat_map(|paragraph| paragraph.fragments.iter())
            .map(|fragment| fragment.text.as_str())
            .collect::<String>();
        assert_eq!(text, "One\nTwo\nThree");

        let rewritten = rewrite_document_xml(xml, &model).unwrap();
        let rewritten_text = String::from_utf8_lossy(&rewritten);
        assert!(rewritten_text.contains("<w:br/>"));
        let reparsed = parse_document_xml(&rewritten).unwrap();
        let reparsed_text = reparsed
            .paragraphs
            .iter()
            .flat_map(|paragraph| paragraph.fragments.iter())
            .map(|fragment| fragment.text.as_str())
            .collect::<String>();
        assert_eq!(reparsed_text, "One\nTwo\nThree");
        assert_eq!(reparsed_text.matches('\n').count(), 2);
    }
}
