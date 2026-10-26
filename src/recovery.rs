//! Portable DOCX checkpoints. Each window owns a locked session directory.
//! Only fully validated, immutable generations are offered after a crash.
use crate::docx::DocxPackage;
use crate::text_document::{TextDocument, TextKind};
use crate::workbook_edit::{CellFormat, CellKey};
use crate::xlsx::{SpreadsheetCell, SpreadsheetModel};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug)]
pub struct Session {
    kind: SessionKind,
    directory: PathBuf,
    // The OS releases this lock even on an aborted/killed process. PID reuse
    // cannot turn a crashed session into a permanently "live" one.
    _lease: File,
    generation: u64,
    pub origin: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SessionKind {
    Docx,
    Text,
    Workbook,
}
fn snapshot_kind(name: &str) -> Option<SessionKind> {
    generation(name)?;
    Some(if name.ends_with(".lotext") {
        SessionKind::Text
    } else if name.ends_with(".loxlsx") {
        SessionKind::Workbook
    } else {
        SessionKind::Docx
    })
}
const TEXT_MAGIC: &[u8] = b"LITEOFFICE-TEXT-1\0";
const TEXT_LIMIT: usize = 32 * 1024 * 1024;
fn text_checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, b| {
        (hash ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn text_payload(document: &TextDocument) -> io::Result<Vec<u8>> {
    if document.text.len() > TEXT_LIMIT {
        return Err(io::Error::other("recovery text exceeds 32 MiB"));
    }
    let mut body = Vec::with_capacity(document.text.len() + 1);
    body.push(match document.kind {
        TextKind::Txt => 0,
        TextKind::Markdown => 1,
        TextKind::Rtf => 2,
    });
    body.extend(document.text.as_bytes());
    let mut bytes = TEXT_MAGIC.to_vec();
    bytes.extend((body.len() as u64).to_le_bytes());
    bytes.extend(text_checksum(&body).to_le_bytes());
    bytes.extend(body);
    Ok(bytes)
}
fn read_text_payload(path: &Path) -> io::Result<(TextKind, String)> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((TEXT_LIMIT + TEXT_MAGIC.len() + 18) as u64)
        .read_to_end(&mut bytes)?;
    let invalid = || io::Error::other("damaged or unsupported text checkpoint");
    let header = TEXT_MAGIC.len() + 16;
    if !bytes.starts_with(TEXT_MAGIC)
        || bytes.len() < header + 1
        || bytes.len() > header + TEXT_LIMIT + 1
    {
        return Err(invalid());
    }
    let length = u64::from_le_bytes(
        bytes[TEXT_MAGIC.len()..TEXT_MAGIC.len() + 8]
            .try_into()
            .map_err(|_| invalid())?,
    );
    let checksum = u64::from_le_bytes(
        bytes[TEXT_MAGIC.len() + 8..header]
            .try_into()
            .map_err(|_| invalid())?,
    );
    let body = &bytes[header..];
    if length != body.len() as u64 || checksum != text_checksum(body) {
        return Err(invalid());
    }
    let kind = match body[0] {
        0 => TextKind::Txt,
        1 => TextKind::Markdown,
        2 => TextKind::Rtf,
        _ => return Err(invalid()),
    };
    let text = std::str::from_utf8(&body[1..])
        .map_err(|_| invalid())?
        .to_owned();
    Ok((kind, text))
}

fn plain_directory(path: &Path) -> io::Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Ok(false); // no junction/reparse-point traversal
        }
    }
    Ok(metadata.is_dir() && !metadata.file_type().is_symlink())
}

fn generation(name: &str) -> Option<u64> {
    let name = name.strip_prefix("snapshot-")?;
    let digits = name
        .strip_suffix(".docx")
        .or_else(|| name.strip_suffix(".lotext"))
        .or_else(|| name.strip_suffix(".loxlsx"))?;
    (digits.len() == 20 && digits.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| digits.parse().ok())?
}

fn validated_package(path: &Path) -> Result<DocxPackage, crate::docx::DocxError> {
    let package = DocxPackage::open(path)?;
    package.verify_all_parts()?;
    Ok(package)
}

const WORKBOOK_MAGIC: &[u8] = b"LITEOFFICE-XLSX-1\0";
const WORKBOOK_LIMIT: usize = 32 * 1024 * 1024;

fn workbook_payload(
    sheets: &[SpreadsheetModel],
    formats: &BTreeMap<CellKey, CellFormat>,
) -> io::Result<Vec<u8>> {
    let cell_count = sheets.iter().map(|sheet| sheet.cells.len()).sum::<usize>();
    if sheets.is_empty() || sheets.len() > 64 || cell_count > 250_000 {
        return Err(io::Error::other("too many sheets or cells for recovery"));
    }
    let mut body = Vec::new();
    body.extend((sheets.len() as u32).to_le_bytes());
    for sheet in sheets {
        let name = sheet.sheet_name.as_bytes();
        if name.len() > 4096 {
            return Err(io::Error::other("sheet name too long for recovery"));
        }
        body.extend((name.len() as u32).to_le_bytes());
        body.extend(name);
        body.extend((sheet.row_count as u32).to_le_bytes());
        body.extend((sheet.column_count as u32).to_le_bytes());
        body.extend((sheet.cells.len() as u32).to_le_bytes());
        for cell in &sheet.cells {
            let value = cell.value.as_bytes();
            body.extend((cell.row as u32).to_le_bytes());
            body.extend((cell.column as u32).to_le_bytes());
            body.extend((value.len() as u32).to_le_bytes());
            body.extend(value);
        }
    }
    body.extend((formats.len() as u32).to_le_bytes());
    for ((sheet, row, column), format) in formats {
        body.extend((*sheet as u32).to_le_bytes());
        body.extend((*row as u32).to_le_bytes());
        body.extend((*column as u32).to_le_bytes());
        body.extend(format.size.to_le_bytes());
        body.push(u8::from(format.bold));
    }
    if body.len() > WORKBOOK_LIMIT {
        return Err(io::Error::other("recovery workbook exceeds 32 MiB"));
    }
    let mut bytes = WORKBOOK_MAGIC.to_vec();
    bytes.extend((body.len() as u64).to_le_bytes());
    bytes.extend(text_checksum(&body).to_le_bytes());
    bytes.extend(body);
    Ok(bytes)
}

struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| io::Error::other("damaged workbook checkpoint"))?;
        if end > self.bytes.len() {
            return Err(io::Error::other("damaged workbook checkpoint"));
        }
        let slice = &self.bytes[self.position..end];
        self.position = end;
        Ok(slice)
    }
    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().map_err(
            |_| io::Error::other("damaged workbook checkpoint"),
        )?))
    }
    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().map_err(
            |_| io::Error::other("damaged workbook checkpoint"),
        )?))
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn string(&mut self) -> io::Result<String> {
        let length = self.u32()? as usize;
        if length > WORKBOOK_LIMIT {
            return Err(io::Error::other("damaged workbook checkpoint"));
        }
        String::from_utf8(self.take(length)?.to_vec())
            .map_err(|_| io::Error::other("damaged workbook checkpoint"))
    }
}

fn read_workbook_payload(
    path: &Path,
) -> io::Result<(Vec<SpreadsheetModel>, BTreeMap<CellKey, CellFormat>)> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take((WORKBOOK_LIMIT + WORKBOOK_MAGIC.len() + 18) as u64)
        .read_to_end(&mut bytes)?;
    let invalid = || io::Error::other("damaged or unsupported workbook checkpoint");
    let header = WORKBOOK_MAGIC.len() + 16;
    if !bytes.starts_with(WORKBOOK_MAGIC) || bytes.len() < header {
        return Err(invalid());
    }
    let length = u64::from_le_bytes(
        bytes[WORKBOOK_MAGIC.len()..WORKBOOK_MAGIC.len() + 8]
            .try_into()
            .map_err(|_| invalid())?,
    );
    let checksum = u64::from_le_bytes(
        bytes[WORKBOOK_MAGIC.len() + 8..header]
            .try_into()
            .map_err(|_| invalid())?,
    );
    let body = &bytes[header..];
    if length != body.len() as u64 || checksum != text_checksum(body) {
        return Err(invalid());
    }
    let mut cursor = Cursor {
        bytes: body,
        position: 0,
    };
    let sheet_count = cursor.u32()? as usize;
    if sheet_count == 0 || sheet_count > 64 {
        return Err(invalid());
    }
    let mut sheets = Vec::with_capacity(sheet_count);
    let mut total_cells = 0usize;
    for _ in 0..sheet_count {
        let sheet_name = cursor.string()?;
        let row_count = cursor.u32()? as usize;
        let column_count = cursor.u32()? as usize;
        let cell_count = cursor.u32()? as usize;
        total_cells = total_cells.saturating_add(cell_count);
        if total_cells > 250_000 || row_count > 1_048_576 || column_count > 16_384 {
            return Err(invalid());
        }
        let mut cells = Vec::with_capacity(cell_count);
        for _ in 0..cell_count {
            let row = cursor.u32()? as usize;
            let column = cursor.u32()? as usize;
            let value = cursor.string()?;
            if row >= 1_048_576 || column >= 16_384 {
                return Err(invalid());
            }
            cells.push(SpreadsheetCell { row, column, value });
        }
        sheets.push(SpreadsheetModel {
            sheet_name,
            cells,
            row_count,
            column_count,
        });
    }
    let format_count = cursor.u32()? as usize;
    if format_count > 250_000 {
        return Err(invalid());
    }
    let mut formats = BTreeMap::new();
    for _ in 0..format_count {
        let sheet = cursor.u32()? as usize;
        let row = cursor.u32()? as usize;
        let column = cursor.u32()? as usize;
        let size = cursor.u16()?;
        let bold = cursor.u8()? != 0;
        if sheet >= sheets.len() || row >= 1_048_576 || column >= 16_384 {
            return Err(invalid());
        }
        formats.insert((sheet, row, column), CellFormat { size, bold });
    }
    if cursor.position != body.len() {
        return Err(invalid());
    }
    Ok((sheets, formats))
}

impl Session {
    pub fn create(root: &Path, origin: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        if !plain_directory(root)? {
            return Err(io::Error::other("recovery root is not a plain directory"));
        }
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let directory = root.join(format!("session-{}-{nonce}", std::process::id()));
        fs::create_dir(&directory)?;
        let lease = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(directory.join("lease"))?;
        lease.try_lock().map_err(io::Error::other)?;
        // This is a display label, never an instruction/path used for restoring
        // or overwriting the original. Non-Unicode names are display-lossy only.
        let origin = origin.to_string_lossy().into_owned();
        let mut label = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("origin.txt"))?;
        label.write_all(origin.as_bytes())?;
        label.sync_all()?;
        Ok(Self {
            kind: SessionKind::Docx,
            directory,
            _lease: lease,
            generation: 0,
            origin,
        })
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn create_text(root: &Path, origin: Option<&Path>) -> io::Result<Self> {
        let origin = origin.map(std::path::absolute).transpose()?;
        let mut session = Self::create(
            root,
            origin
                .as_deref()
                .unwrap_or_else(|| Path::new("Untitled.txt")),
        )?;
        session.kind = SessionKind::Text;
        Ok(session)
    }
    pub fn is_text(&self) -> bool {
        self.kind == SessionKind::Text
    }

    pub fn checkpoint_text(&mut self, document: &TextDocument) -> io::Result<()> {
        if !self.is_text() {
            return Err(io::Error::other("not a text recovery session"));
        }
        let bytes = text_payload(document)?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| io::Error::other("too many checkpoints"))?;
        let pending = self
            .directory
            .join(format!("pending-{:020}.tmp", self.generation));
        let committed = self
            .directory
            .join(format!("snapshot-{:020}.lotext", self.generation));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        let (kind, text) = read_text_payload(&pending)?;
        if kind != document.kind || text != document.text {
            return Err(io::Error::other("text checkpoint round-trip mismatch"));
        }
        fs::rename(&pending, &committed)?;
        let snapshots = self.snapshots()?;
        let mut kept = false;
        for (_, path) in snapshots.iter().rev().skip(1) {
            if !kept && read_text_payload(path).is_ok() {
                kept = true;
                continue;
            }
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    pub fn restore_text(&self) -> io::Result<(TextDocument, bool)> {
        if !self.is_text() {
            return Err(io::Error::other("not a text recovery session"));
        }
        let mut damaged = false;
        for (_, path) in self.snapshots()?.iter().rev() {
            match read_text_payload(path) {
                Ok((kind, text)) => {
                    let origin = PathBuf::from(&self.origin);
                    return Ok((
                        TextDocument::recovered_copy(
                            kind,
                            text,
                            origin.is_absolute().then_some(origin),
                        ),
                        damaged,
                    ));
                }
                Err(_) => damaged = true,
            }
        }
        Err(io::Error::other(
            "No valid text checkpoint; recovery files kept",
        ))
    }

    pub fn create_workbook(root: &Path, origin: &Path) -> io::Result<Self> {
        let origin = std::path::absolute(origin)?;
        let mut session = Self::create(root, &origin)?;
        session.kind = SessionKind::Workbook;
        Ok(session)
    }

    pub fn is_workbook(&self) -> bool {
        self.kind == SessionKind::Workbook
    }

    pub fn checkpoint_workbook(
        &mut self,
        sheets: &[SpreadsheetModel],
        formats: &BTreeMap<CellKey, CellFormat>,
    ) -> io::Result<()> {
        if !self.is_workbook() {
            return Err(io::Error::other("not a workbook recovery session"));
        }
        let bytes = workbook_payload(sheets, formats)?;
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| io::Error::other("too many checkpoints"))?;
        let pending = self
            .directory
            .join(format!("pending-{:020}.tmp", self.generation));
        let committed = self
            .directory
            .join(format!("snapshot-{:020}.loxlsx", self.generation));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        let (restored_sheets, restored_formats) = read_workbook_payload(&pending)?;
        if restored_sheets != sheets || restored_formats != *formats {
            return Err(io::Error::other("workbook checkpoint round-trip mismatch"));
        }
        fs::rename(&pending, &committed)?;
        let snapshots = self.snapshots()?;
        let mut kept_predecessor = false;
        for (_, path) in snapshots.iter().rev().skip(1) {
            if !kept_predecessor && read_workbook_payload(path).is_ok() {
                kept_predecessor = true;
                continue;
            }
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    pub fn restore_workbook(
        &self,
    ) -> io::Result<(Vec<SpreadsheetModel>, BTreeMap<CellKey, CellFormat>, bool)> {
        if !self.is_workbook() {
            return Err(io::Error::other("not a workbook recovery session"));
        }
        let mut damaged = false;
        for (_, path) in self.snapshots()?.iter().rev() {
            match read_workbook_payload(path) {
                Ok((sheets, formats)) => return Ok((sheets, formats, damaged)),
                Err(_) => damaged = true,
            }
        }
        Err(io::Error::other(
            "No valid workbook checkpoint; recovery files kept",
        ))
    }

    fn snapshots(&self) -> io::Result<Vec<(u64, PathBuf)>> {
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                if let Some(number) = entry
                    .file_name()
                    .to_str()
                    .filter(|n| snapshot_kind(n) == Some(self.kind))
                    .and_then(generation)
                {
                    files.push((number, entry.path()));
                }
            }
        }
        files.sort_by_key(|(number, _)| *number);
        Ok(files)
    }

    /// Does not adopt the destination or change the source fingerprint/model.
    /// A failed/interrupted write never replaces a previously committed file.
    pub fn checkpoint(&mut self, package: &DocxPackage) -> io::Result<()> {
        if self.kind != SessionKind::Docx {
            return Err(io::Error::other("not a DOCX recovery session"));
        }
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| io::Error::other("too many checkpoints"))?;
        let pending = self
            .directory
            .join(format!("pending-{:020}.tmp", self.generation));
        let committed = self
            .directory
            .join(format!("snapshot-{:020}.docx", self.generation));
        package.save_as(&pending).map_err(io::Error::other)?;
        let reopened = validated_package(&pending).map_err(io::Error::other)?;
        if reopened.model != package.model {
            return Err(io::Error::other(
                "recovery validation changed the document model",
            ));
        }
        // save_as synced the complete ZIP. Rename publishes it only after the
        // full DOCX loader and model round-trip have succeeded.
        fs::rename(&pending, &committed)?;
        let snapshots = self.snapshots()?;
        // Keep one VALID predecessor as protection against a damaged latest file.
        // Never rotate away the last valid checkpoint before its replacement.
        let mut kept_predecessor = false;
        for (_, path) in snapshots.iter().rev().skip(1) {
            if !kept_predecessor && validated_package(path).is_ok() {
                kept_predecessor = true;
                continue;
            }
            // Rotation is maintenance after a committed checkpoint, not a
            // failed write. Leave an undeletable predecessor intact.
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    /// Invalid newest generation falls back to its predecessor. Never opens a
    /// partial/temp file, and never removes malformed evidence automatically.
    pub fn restore(&self) -> io::Result<(DocxPackage, bool)> {
        if self.kind != SessionKind::Docx {
            return Err(io::Error::other("not a DOCX recovery session"));
        }
        let mut errors = Vec::new();
        for (_, path) in self.snapshots()?.iter().rev() {
            match validated_package(path) {
                Ok(package) => return Ok((package, !errors.is_empty())),
                Err(error) => errors.push(format!("{}: {error}", path.display())),
            }
        }
        Err(io::Error::other(format!(
            "No valid recovery DOCX; files kept. {}",
            errors.join("; ")
        )))
    }

    /// Call only after a confirmed save or an explicit user discard. No Drop
    /// cleanup: even an ordinary exit must not erase an unaccepted recovery.
    pub fn discard(&self) -> io::Result<()> {
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let owned_payload = generation(&name).is_some()
                || name == "origin.txt"
                || (name.starts_with("pending-") && name.ends_with(".tmp"))
                || (name.starts_with(".")
                    && name.contains(".liteoffice-")
                    && name.ends_with(".tmp"));
            if owned_payload && entry.file_type()?.is_file() {
                fs::remove_file(entry.path())?;
            }
        }
        // The tiny lease-only directory is intentionally retained: removing the
        // lock pathname while another process scans it can split ownership.
        Ok(())
    }
}

/// Claims crashed sessions exclusively. Active windows and candidates already
/// offered by another window are skipped. Unreadable/malformed files stay put.
pub fn discover(root: &Path) -> io::Result<(Vec<Session>, Vec<String>)> {
    if !root.try_exists()? {
        return Ok((Vec::new(), Vec::new()));
    }
    if !plain_directory(root)? {
        return Err(io::Error::other("recovery root is not a plain directory"));
    }
    let mut sessions = Vec::new();
    let mut warnings = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with("session-") {
            continue;
        }
        match plain_directory(&entry.path()) {
            Ok(true) => {}
            Ok(false) => continue,
            Err(error) => {
                warnings.push(format!("{}: {error}", entry.path().display()));
                continue;
            }
        }
        let directory = entry.path();
        let lease_path = directory.join("lease");
        if !fs::symlink_metadata(&lease_path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
        {
            continue;
        }
        let lease = match OpenOptions::new().read(true).write(true).open(&lease_path) {
            Ok(file) => file,
            Err(error) => {
                warnings.push(format!("{}: {error}", directory.display()));
                continue;
            }
        };
        match lease.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => continue,
            Err(error) => {
                warnings.push(format!("{}: {error}", directory.display()));
                continue;
            }
        }
        let mut origin = String::new();
        if let Ok(file) = File::open(directory.join("origin.txt")) {
            let _ = file.take(65536).read_to_string(&mut origin);
        }
        let mut kind = None;
        let mut mixed = false;
        for payload in fs::read_dir(&directory)? {
            let payload = payload?;
            if !payload.file_type()?.is_file() {
                continue;
            }
            if let Some(next) = payload.file_name().to_str().and_then(snapshot_kind) {
                if kind.is_some_and(|k| k != next) {
                    mixed = true;
                    break;
                }
                kind = Some(next);
            }
        }
        if mixed {
            warnings.push(format!(
                "{}: mixed recovery types; files kept",
                directory.display()
            ));
            continue;
        }
        let Some(kind) = kind else {
            continue;
        };
        let mut session = Session {
            kind,
            directory,
            _lease: lease,
            generation: 0,
            origin,
        };
        let snapshots = match session.snapshots() {
            Ok(snapshots) => snapshots,
            Err(error) => {
                warnings.push(format!("{}: {error}", session.directory.display()));
                continue;
            }
        };
        if let Some((last, _)) = snapshots.last() {
            session.generation = *last;
            sessions.push(session);
        }
    }
    sessions.sort_by(|left, right| left.directory.cmp(&right.directory));
    Ok((sessions, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    #[test]
    fn text_generations_validate_checksum_fallback_empty_text_and_failed_writes() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("recovery");
        let mut document =
            TextDocument::recovered_copy(TextKind::Markdown, "# Previous".into(), None);
        let mut session = Session::create_text(&root, None).unwrap();
        session.checkpoint_text(&document).unwrap();
        document.text = "# New".into();
        session.checkpoint_text(&document).unwrap();
        let newest = session
            .directory
            .join("snapshot-00000000000000000002.lotext");
        let mut bytes = fs::read(&newest).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&newest, &bytes).unwrap();
        let (restored, fallback) = session.restore_text().unwrap();
        assert!(fallback);
        assert_eq!(restored.text, "# Previous");
        assert_eq!(restored.kind, TextKind::Markdown);
        assert!(restored.requires_save_as());
        document.text.clear();
        session.checkpoint_text(&document).unwrap();
        assert_eq!(session.restore_text().unwrap().0.text, "");
        let blocked = session.directory.join("pending-00000000000000000004.tmp");
        fs::create_dir(&blocked).unwrap();
        document.text = "Keep only in memory".into();
        assert!(session.checkpoint_text(&document).is_err());
        assert_eq!(session.restore_text().unwrap().0.text, "");
        fs::remove_dir(blocked).unwrap();
        fs::write(
            session
                .directory
                .join("snapshot-00000000000000000003.lotext"),
            b"truncated",
        )
        .unwrap();
        assert_eq!(session.restore_text().unwrap().0.text, "# Previous");
        assert!(session.restore().is_err());
        session.discard().unwrap();
        drop(session);
        assert!(discover(&root).unwrap().0.is_empty());
    }

    #[test]
    fn text_sessions_are_isolated_and_restore_literal_rtf_without_interpretation() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("recovery");
        let source = folder.path().join("original.rtf");
        let mut original = TextDocument::recovered_copy(TextKind::Rtf, "Original".into(), None);
        original.save(&source).unwrap();
        let bytes = fs::read(&source).unwrap();
        let mut active = Session::create_text(&root, Some(&source)).unwrap();
        let mut crashed = Session::create_text(&root, Some(&source)).unwrap();
        original.text = "{\\rtf1\\object LITERAL TEXT} Ёжик 🙂".into();
        active.checkpoint_text(&original).unwrap();
        crashed.checkpoint_text(&original).unwrap();
        assert!(discover(&root).unwrap().0.is_empty());
        drop(crashed);
        let (mut candidates, _) = discover(&root).unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(discover(&root).unwrap().0.is_empty());
        let session = candidates.pop().unwrap();
        let mut restored = session.restore_text().unwrap().0;
        assert_eq!(restored.text, original.text);
        assert!(restored.path().is_none());
        assert!(restored.save(&source).is_err());
        assert_eq!(fs::read(&source).unwrap(), bytes);
        let copy = folder.path().join("restored.rtf");
        restored.save(&copy).unwrap();
        assert_eq!(TextDocument::open(&copy).unwrap().text, original.text);
        session.discard().unwrap();
        drop(session);
        drop(active);
        assert_eq!(discover(&root).unwrap().0.len(), 1);
    }

    #[test]
    fn isolated_text_crash_worker() {
        let Some(root) = std::env::var_os("LITEOFFICE_TEXT_RECOVERY_WORKER") else {
            return;
        };
        let root = PathBuf::from(root);
        let mut sessions = Vec::new();
        for (index, kind) in [
            TextKind::Txt,
            TextKind::Txt,
            TextKind::Markdown,
            TextKind::Rtf,
        ]
        .into_iter()
        .enumerate()
        {
            let mut document = TextDocument::recovered_copy(kind, "Original".into(), None);
            if index > 0 {
                document
                    .save(
                        &root
                            .parent()
                            .unwrap()
                            .join(format!("source-{index}.{}", kind.extension())),
                    )
                    .unwrap();
            }
            let mut session = Session::create_text(&root, document.path()).unwrap();
            document.text = format!("Unsaved {index} Ёжик 🙂");
            session.checkpoint_text(&document).unwrap();
            sessions.push(session);
        }
        fs::write(root.join("text-ready"), b"ready").unwrap();
        loop {
            std::hint::black_box(&sessions);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }

    #[test]
    fn forced_text_process_termination_recovers_txt_md_rtf_and_untitled() {
        struct Worker(std::process::Child);
        impl Drop for Worker {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("recovery");
        let mut worker = Worker(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "recovery::tests::isolated_text_crash_worker",
                    "--nocapture",
                ])
                .env("LITEOFFICE_TEXT_RECOVERY_WORKER", &root)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let started = std::time::Instant::now();
        while !root.join("text-ready").exists() {
            assert!(
                started.elapsed() < std::time::Duration::from_secs(10),
                "checkpoint worker timed out"
            );
            assert!(
                worker.0.try_wait().unwrap().is_none(),
                "checkpoint worker exited"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(discover(&root).unwrap().0.is_empty());
        worker.0.kill().unwrap();
        worker.0.wait().unwrap();
        let (sessions, warnings) = discover(&root).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(sessions.len(), 4);
        let mut texts = Vec::new();
        for session in sessions {
            let (mut restored, fallback) = session.restore_text().unwrap();
            assert!(!fallback);
            assert!(restored.requires_save_as());
            assert!(restored.path().is_none());
            texts.push(restored.text.clone());
            if session.origin != "Untitled.txt" {
                let original = Path::new(&session.origin);
                let before = fs::read(original).unwrap();
                assert!(restored.save(original).is_err());
                assert_eq!(fs::read(original).unwrap(), before);
            }
        }
        texts.sort();
        assert_eq!(
            texts,
            (0..4)
                .map(|i| format!("Unsaved {i} Ёжик 🙂"))
                .collect::<Vec<_>>()
        );
    }

    fn fixture(path: &Path) -> DocxPackage {
        let mut archive = zip::ZipWriter::new(File::create(path).unwrap());
        archive
            .start_file("word/document.xml", SimpleFileOptions::default())
            .unwrap();
        archive.write_all(br#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Original</w:t></w:r></w:p></w:body></w:document>"#).unwrap();
        archive
            .start_file(
                "custom/preserved.bin",
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        archive.write_all(b"unknown part preserved").unwrap();
        archive.finish().unwrap();
        DocxPackage::open(path).unwrap()
    }

    #[test]
    fn checkpoint_survives_restart_without_changing_original_or_package_path() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("Исходный файл.docx");
        let root = folder.path().join("recovery");
        let mut package = fixture(&source);
        let original = fs::read(&source).unwrap();
        package.model.fragment_mut(0).unwrap().text = "Несохранённое Ё".into();
        let mut session = Session::create(&root, &source).unwrap();
        session.checkpoint(&package).unwrap();
        assert_eq!(package.source_path(), source);
        assert_eq!(fs::read(&source).unwrap(), original);
        assert!(discover(&root).unwrap().0.is_empty());
        drop(session); // simulate process releasing all handles, not cleanup
        fs::remove_file(&source).unwrap();
        let (sessions, _) = discover(&root).unwrap();
        assert_eq!(sessions.len(), 1);
        let (restored, fallback) = sessions[0].restore().unwrap();
        assert!(!fallback);
        assert_eq!(restored.model, package.model);
        assert!(discover(&root).unwrap().0.is_empty()); // exclusive claimant
        let copy = folder.path().join("Recovered.docx");
        restored.save_as(&copy).unwrap();
        let mut zip = zip::ZipArchive::new(File::open(copy).unwrap()).unwrap();
        let mut unknown = String::new();
        zip.by_name("custom/preserved.bin")
            .unwrap()
            .read_to_string(&mut unknown)
            .unwrap();
        assert_eq!(unknown, "unknown part preserved");
    }

    #[test]
    fn damaged_newest_and_interrupted_temp_fall_back_without_deleting_evidence() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("source.docx");
        let root = folder.path().join("recovery");
        let mut package = fixture(&source);
        let original = fs::read(&source).unwrap();
        let mut session = Session::create(&root, &source).unwrap();
        package.model.fragment_mut(0).unwrap().text = "First checkpoint".into();
        session.checkpoint(&package).unwrap();
        package.model.fragment_mut(0).unwrap().text = "Second checkpoint".into();
        session.checkpoint(&package).unwrap();
        let newest = session.snapshots().unwrap().last().unwrap().1.clone();
        fs::write(&newest, b"broken ZIP").unwrap();
        fs::write(
            session.directory.join("pending-00000000000000000003.tmp"),
            b"partial",
        )
        .unwrap();
        let (restored, fallback) = session.restore().unwrap();
        assert!(fallback);
        assert_eq!(restored.model.fragment(0).unwrap().text, "First checkpoint");
        assert_eq!(fs::read(&newest).unwrap(), b"broken ZIP");
        assert_eq!(fs::read(source).unwrap(), original);
    }

    #[test]
    fn failed_checkpoint_preserves_previous_snapshot_and_explicit_discard_clears_it() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("source.docx");
        let root = folder.path().join("recovery");
        let mut package = fixture(&source);
        let mut session = Session::create(&root, &source).unwrap();
        session.checkpoint(&package).unwrap();
        let blocked = session.directory.join("pending-00000000000000000002.tmp");
        fs::create_dir(&blocked).unwrap();
        package.model.fragment_mut(0).unwrap().text = "New edit".into();
        assert!(session.checkpoint(&package).is_err());
        assert_eq!(
            session.restore().unwrap().0.model.fragment(0).unwrap().text,
            "Original"
        );
        fs::remove_dir(blocked).unwrap();
        session.discard().unwrap();
        drop(session);
        assert!(discover(&root).unwrap().0.is_empty());
    }

    #[test]
    fn independent_windows_keep_two_generations_each_and_live_sessions_are_not_offered() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("source.docx");
        let root = folder.path().join("recovery");
        let mut package = fixture(&source);
        let mut a = Session::create(&root, &source).unwrap();
        let mut b = Session::create(&root, &source).unwrap();
        for text in ["A1", "A2", "A3"] {
            package.model.fragment_mut(0).unwrap().text = text.into();
            a.checkpoint(&package).unwrap();
        }
        package.model.fragment_mut(0).unwrap().text = "B".into();
        b.checkpoint(&package).unwrap();
        assert_eq!(a.snapshots().unwrap().len(), 2);
        assert!(discover(&root).unwrap().0.is_empty());
        drop(a);
        let (mut sessions, _) = discover(&root).unwrap();
        assert_eq!(sessions.len(), 1);
        let a = sessions.pop().unwrap();
        assert_eq!(a.restore().unwrap().0.model.fragment(0).unwrap().text, "A3");
        a.discard().unwrap();
        drop(a);
        drop(b);
        let (sessions, _) = discover(&root).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0]
                .restore()
                .unwrap()
                .0
                .model
                .fragment(0)
                .unwrap()
                .text,
            "B"
        );
    }

    #[test]
    fn malformed_only_session_is_retained_until_explicit_rejection() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("recovery");
        let session = Session::create(&root, Path::new("unknown.docx")).unwrap();
        let broken = session.directory.join("snapshot-00000000000000000001.docx");
        fs::write(&broken, b"malformed").unwrap();
        drop(session);
        let (sessions, _) = discover(&root).unwrap();
        assert_eq!(sessions.len(), 1);
        assert!(sessions[0].restore().is_err());
        assert!(broken.exists());
        sessions[0].discard().unwrap();
        assert!(!broken.exists());
    }

    #[test]
    fn corrupted_unknown_package_part_also_triggers_fallback() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("source.docx");
        let mut package = fixture(&source);
        let mut session = Session::create(&folder.path().join("recovery"), &source).unwrap();
        session.checkpoint(&package).unwrap();
        package.model.fragment_mut(0).unwrap().text = "Second".into();
        session.checkpoint(&package).unwrap();
        let newest = session.snapshots().unwrap().last().unwrap().1.clone();
        let mut bytes = fs::read(&newest).unwrap();
        let offset = bytes
            .windows(b"unknown part preserved".len())
            .position(|chunk| chunk == b"unknown part preserved")
            .unwrap();
        bytes[offset] ^= 1; // XML remains valid; only an unused OPC part is damaged
        fs::write(&newest, bytes).unwrap();
        let (restored, fallback) = session.restore().unwrap();
        assert!(
            fallback,
            "CRC of every preserved part must be checked, not just document.xml"
        );
        assert_eq!(restored.model.fragment(0).unwrap().text, "Original");
    }

    #[test]
    fn checkpoint_after_fallback_keeps_a_valid_predecessor_not_the_broken_generation() {
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("source.docx");
        let mut package = fixture(&source);
        let mut session = Session::create(&folder.path().join("recovery"), &source).unwrap();
        session.checkpoint(&package).unwrap();
        package.model.fragment_mut(0).unwrap().text = "Second".into();
        session.checkpoint(&package).unwrap();
        let newest = session.snapshots().unwrap().last().unwrap().1.clone();
        fs::write(newest, b"broken second generation").unwrap();
        let (mut restored, _) = session.restore().unwrap();
        restored.model.fragment_mut(0).unwrap().text = "Third".into();
        session.checkpoint(&restored).unwrap();
        let newest = session.snapshots().unwrap().last().unwrap().1.clone();
        fs::write(newest, b"broken third generation").unwrap();
        let (fallback, _) = session.restore().unwrap();
        assert_eq!(fallback.model.fragment(0).unwrap().text, "Original");
    }

    // This test is also used as an isolated worker by the kill/restart test.
    // In the normal suite there is no marker environment variable, so no worker
    // is created or parked in the parent test process.
    #[test]
    fn isolated_crash_worker() {
        let Some(root) = std::env::var_os("LITEOFFICE_RECOVERY_TEST_WORKER") else {
            return;
        };
        let root = PathBuf::from(root);
        let source = root.parent().unwrap().join("source.docx");
        let mut package = DocxPackage::open(&source).unwrap();
        package.model.fragment_mut(0).unwrap().text = "Committed before forced termination".into();
        let mut session = Session::create(&root, &source).unwrap();
        session.checkpoint(&package).unwrap();
        fs::write(root.join("worker-ready"), b"ready").unwrap();
        std::thread::park_timeout(std::time::Duration::from_secs(30));
        panic!("crash-test parent did not terminate its worker");
    }

    #[test]
    fn forced_process_termination_releases_lock_and_preserves_checkpoint() {
        struct Worker(std::process::Child);
        impl Drop for Worker {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let folder = tempfile::tempdir().unwrap();
        let source = folder.path().join("source.docx");
        let root = folder.path().join("recovery");
        fixture(&source);
        let original = fs::read(&source).unwrap();
        let mut worker = Worker(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "recovery::tests::isolated_crash_worker",
                    "--nocapture",
                ])
                .env("LITEOFFICE_RECOVERY_TEST_WORKER", &root)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let start = std::time::Instant::now();
        while !root.join("worker-ready").exists() {
            assert!(
                start.elapsed() < std::time::Duration::from_secs(10),
                "worker checkpoint timeout"
            );
            assert!(
                worker.0.try_wait().unwrap().is_none(),
                "worker exited before checkpoint"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            discover(&root).unwrap().0.is_empty(),
            "live process must not be offered"
        );
        worker.0.kill().unwrap();
        worker.0.wait().unwrap();
        let (sessions, warnings) = discover(&root).unwrap();
        assert!(warnings.is_empty());
        assert_eq!(sessions.len(), 1);
        assert_eq!(
            sessions[0]
                .restore()
                .unwrap()
                .0
                .model
                .fragment(0)
                .unwrap()
                .text,
            "Committed before forced termination"
        );
        assert_eq!(fs::read(source).unwrap(), original);
    }
    #[test]
    fn workbook_checkpoint_roundtrips_values_and_formats_after_restart() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("recovery");
        let origin = folder.path().join("book.xlsx");
        std::fs::write(&origin, b"protected original").unwrap();
        let sheets = vec![SpreadsheetModel {
            sheet_name: "Лист 1".into(),
            cells: vec![
                SpreadsheetCell {
                    row: 0,
                    column: 0,
                    value: "Ёжик".into(),
                },
                SpreadsheetCell {
                    row: 2,
                    column: 3,
                    value: "42".into(),
                },
            ],
            row_count: 3,
            column_count: 4,
        }];
        let mut formats = BTreeMap::new();
        formats.insert(
            (0usize, 0usize, 0usize),
            CellFormat {
                size: 18,
                bold: true,
            },
        );
        {
            let mut session = Session::create_workbook(&root, &origin).unwrap();
            session.checkpoint_workbook(&sheets, &formats).unwrap();
        }
        let (sessions, warnings) = discover(&root).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let session = sessions.iter().find(|s| s.is_workbook()).unwrap();
        let (restored, restored_formats, damaged) = session.restore_workbook().unwrap();
        assert!(!damaged);
        assert_eq!(restored, sheets);
        assert_eq!(restored_formats, formats);
        assert_eq!(std::fs::read(&origin).unwrap(), b"protected original");
        session.discard().unwrap();
    }

    #[test]
    fn damaged_workbook_snapshot_falls_back_to_previous_generation() {
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("recovery");
        let origin = folder.path().join("book.xlsx");
        std::fs::write(&origin, b"protected original").unwrap();
        let first = vec![SpreadsheetModel {
            sheet_name: "S".into(),
            cells: vec![SpreadsheetCell {
                row: 0,
                column: 0,
                value: "first".into(),
            }],
            row_count: 1,
            column_count: 1,
        }];
        let second = vec![SpreadsheetModel {
            sheet_name: "S".into(),
            cells: vec![SpreadsheetCell {
                row: 0,
                column: 0,
                value: "second".into(),
            }],
            row_count: 1,
            column_count: 1,
        }];
        let mut session = Session::create_workbook(&root, &origin).unwrap();
        session
            .checkpoint_workbook(&first, &BTreeMap::new())
            .unwrap();
        session
            .checkpoint_workbook(&second, &BTreeMap::new())
            .unwrap();
        let newest = session.snapshots().unwrap().last().unwrap().1.clone();
        std::fs::write(&newest, b"damaged").unwrap();
        let (restored, _, damaged) = session.restore_workbook().unwrap();
        assert!(damaged);
        assert_eq!(restored, first);
        assert_eq!(std::fs::read(&origin).unwrap(), b"protected original");
    }
}
