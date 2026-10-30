//! Read-only Word 97–2003 importer. Uses the CLX piece table, not a binary-text scan.
//! This extracts ordered main-story text; it does not interpret Word formatting or images.
use encoding_rs::WINDOWS_1252;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const WORD_DOCUMENT_STREAM: &str = "/WordDocument";
const ENCRYPTED_FLAG: u16 = 0x0100;
const WHICH_TABLE_FLAG: u16 = 0x0200;
const MAX_STREAM_BYTES: u64 = 128 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 12_000_000;

#[derive(Debug)]
pub enum DocError {
    Io(std::io::Error),
    MissingWordDocument,
    Encrypted,
    Unsupported(&'static str),
    InvalidStructure(&'static str),
    NoRecoverableText,
}

impl std::fmt::Display for DocError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "DOC file operation failed: {error}"),
            Self::MissingWordDocument => write!(f, "this is not a readable Word 97–2003 document"),
            Self::Encrypted => write!(f, "encrypted DOC files are not supported"),
            Self::Unsupported(message) => write!(f, "unsupported DOC: {message}"),
            Self::InvalidStructure(message) => write!(f, "invalid DOC structure: {message}"),
            Self::NoRecoverableText => write!(f, "no document text was found in this DOC"),
        }
    }
}
impl std::error::Error for DocError {}
impl From<std::io::Error> for DocError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LegacyDocPackage {
    pub paragraphs: Vec<String>,
    pub links: Vec<LegacyDocLink>,
    pub images: Vec<LegacyDocImage>,
    /// Shape anchors read from the original main-story PlcfSpaMom.
    /// These are NOT reliably matched to the recovered raster gallery.
    pub shape_anchors: Vec<LegacyShapeAnchor>,
    /// Only links with matching raster bytes are accepted; never assume scan order.
    pub verified_shape_images: Vec<VerifiedShapeImage>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LegacyShapeAnchor {
    /// Character position (UTF-16 code units) in the main story.
    pub cp: u32,
    /// Best-effort paragraph corresponding to the shape anchor position.
    pub paragraph_index: usize,
    /// OfficeArt shape identifier from SPA; mapping to raster is not yet resolved.
    pub shape_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VerifiedShapeImage {
    pub shape_id: u32,
    pub image_index: usize,
    pub paragraph_index: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LegacyDocImage {
    pub bytes: Vec<u8>,
    pub format: LegacyImageFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum LegacyImageFormat {
    Jpeg,
    Png,
}

/// This is a read-only raster recovery gallery, NOT Word image placement.
/// Only well-framed JPEG / PNG streams are candidates; the UI validates decoding.
/// The byte budget prevents unexpected memory growth on large/untrusted files.
const MAX_DOC_SCAN: usize = 32 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;
const MAX_IMAGES: usize = 64;
const MAX_TOTAL_IMAGE_BYTES: usize = 48 * 1024 * 1024;

fn png_extent(data: &[u8], start: usize) -> Option<usize> {
    if data.get(start..start + 8)? != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let mut at = start + 8;
    let mut first = true;
    loop {
        let len = u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?) as usize;
        if len > MAX_IMAGE_BYTES {
            return None;
        }
        let kind = data.get(at + 4..at + 8)?;
        if first && (kind != b"IHDR" || len != 13) {
            return None;
        }
        first = false;
        let end = at.checked_add(12)?.checked_add(len)?;
        if end > data.len() || end - start > MAX_IMAGE_BYTES {
            return None;
        }
        if kind == b"IEND" {
            if len != 0 {
                return None;
            }
            return Some(end);
        }
        at = end;
    }
}

fn jpeg_extent(data: &[u8], start: usize) -> Option<usize> {
    if data.get(start..start + 3)? != b"\xff\xd8\xff" {
        return None;
    }
    let limit = data.len().min(start.checked_add(MAX_IMAGE_BYTES)?);
    let mut at = start + 3;
    while at + 1 < limit {
        if data[at] == 0xff && data[at + 1] == 0xd9 {
            return Some(at + 2);
        }
        at += 1;
    }
    None
}

fn extract_embedded_rasters(data: &[u8]) -> Vec<LegacyDocImage> {
    use std::collections::HashSet;
    use std::hash::{Hash, Hasher};
    let mut result = Vec::new();
    let mut hashes = HashSet::new();
    let mut total = 0usize;
    let mut i = 0;
    while i + 8 < data.len() && result.len() < MAX_IMAGES && total < MAX_TOTAL_IMAGE_BYTES {
        let found = if data[i] == 0x89 && data.get(i..i + 8) == Some(&b"\x89PNG\r\n\x1a\n"[..]) {
            png_extent(data, i).map(|end| (end, LegacyImageFormat::Png))
        } else if data.get(i..i + 3) == Some(&b"\xff\xd8\xff"[..]) {
            jpeg_extent(data, i).map(|end| (end, LegacyImageFormat::Jpeg))
        } else {
            None
        };
        if let Some((end, format)) = found {
            let bytes = &data[i..end];
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut hasher);
            if hashes.insert(hasher.finish())
                && total.saturating_add(bytes.len()) <= MAX_TOTAL_IMAGE_BYTES
            {
                result.push(LegacyDocImage {
                    bytes: bytes.to_vec(),
                    format,
                });
                total += bytes.len();
            }
            i = end;
        } else {
            i += 1;
        }
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LegacyDocLink {
    pub paragraph_index: usize,
    pub label: String,
    pub url: String,
}

impl LegacyDocPackage {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DocError> {
        let mut compound = cfb::open(path.as_ref())?;
        let word = read_stream(&mut compound, WORD_DOCUMENT_STREAM)?;
        let flags = u16_at(&word, 10).ok_or(DocError::InvalidStructure("truncated FIB"))?;
        if flags & ENCRYPTED_FLAG != 0 {
            return Err(DocError::Encrypted);
        }
        // The fWhichTblStm bit in the FIB selects the correct 0Table or 1Table stream.
        let table_name = if flags & WHICH_TABLE_FLAG != 0 {
            "/1Table"
        } else {
            "/0Table"
        };
        let table = read_stream(&mut compound, table_name)?;
        let text = read_main_story(&word, &table)?;
        let shape_anchors = read_shape_anchors(&word, &table, &text).unwrap_or_default();
        let (filtered, extracted_links) = decode_fields(&text);
        let paragraphs = split_paragraphs(&filtered);
        if paragraphs.iter().all(|p| p.trim().is_empty()) {
            return Err(DocError::NoRecoverableText);
        }
        let links = assign_links_to_paragraphs(&filtered, extracted_links);
        // Best-effort raster recovery from the compound-file bytes. A CFB stream
        // can be fragmented, so this may miss assets; the gallery is intentionally
        // unpositioned. We never claim to render the original Word page layout.
        let mut images = Vec::new();
        if let Ok(meta) = std::fs::metadata(path.as_ref()) {
            if meta.len() <= MAX_DOC_SCAN as u64 {
                if let Ok(bytes) = std::fs::read(path.as_ref()) {
                    images = extract_embedded_rasters(&bytes);
                }
            }
        }
        let verified_shape_images =
            read_verified_shape_images(&word, &table, &shape_anchors, &images);
        Ok(Self {
            paragraphs,
            links,
            images,
            shape_anchors,
            verified_shape_images,
        })
    }
}

fn read_stream<F: std::io::Read + std::io::Seek>(
    compound: &mut cfb::CompoundFile<F>,
    name: &str,
) -> Result<Vec<u8>, DocError> {
    let mut stream = compound.open_stream(name).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            DocError::MissingWordDocument
        } else {
            DocError::Io(err)
        }
    })?;
    let stream_len = stream.seek(SeekFrom::End(0))?;
    if stream_len > MAX_STREAM_BYTES {
        return Err(DocError::Unsupported("stream exceeds 128 MiB"));
    }
    stream.seek(SeekFrom::Start(0))?;
    let mut data = Vec::new();
    stream.read_to_end(&mut data)?;
    Ok(data)
}
fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}
fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// Parse FcPlcSpaMom/LcbPlcSpaMom (FibRgFcLcb97 entry 40).
/// PLC layout: (n+1) CPs, followed by n 26-byte SPA records.
/// Conservative validation: malformed/unsupported data produces no suggestions.
fn read_shape_anchors(
    word: &[u8],
    table: &[u8],
    main_text: &str,
) -> Option<Vec<LegacyShapeAnchor>> {
    if u16_at(word, 0)? != 0xA5EC {
        return None;
    }
    let csw = u16_at(word, 0x20)? as usize;
    let cslw_at = 0x22usize.checked_add(csw.checked_mul(2)?)?;
    let cslw = u16_at(word, cslw_at)? as usize;
    let rglw = cslw_at.checked_add(2)?;
    if cslw < 4 {
        return None;
    }
    let ccp_text = u32_at(word, rglw.checked_add(12)?)?;
    let fc_lcb_at = rglw.checked_add(cslw.checked_mul(4)?)?;
    if u16_at(word, fc_lcb_at)? <= 40 {
        return None;
    }
    let pair = fc_lcb_at.checked_add(2)?.checked_add(40 * 8)?;
    let fc = u32_at(word, pair)? as usize;
    let len = u32_at(word, pair + 4)? as usize;
    if len == 0 {
        return Some(Vec::new());
    }
    if len < 34 || !(len - 4).is_multiple_of(30) {
        return None;
    }
    let n = (len - 4) / 30;
    if n > 1024 {
        return None;
    }
    let plc = table.get(fc..fc.checked_add(len)?)?;
    let spa_start = (n + 1).checked_mul(4)?;
    let mut result = Vec::with_capacity(n);
    let mut previous = None;
    for i in 0..n {
        let cp = u32_at(plc, i * 4)?;
        if cp >= ccp_text || previous.is_some_and(|old| cp <= old) {
            return None;
        }
        previous = Some(cp);
        let shape_id = u32_at(plc, spa_start + i * 26)?;
        // CP is measured in UTF-16 code units, not UTF-8 bytes.
        let mut utf16_position = 0u32;
        let mut paragraph_index = 0usize;
        for ch in main_text.chars() {
            if utf16_position >= cp {
                break;
            }
            utf16_position += ch.len_utf16() as u32;
            if ch == '\r' {
                paragraph_index += 1;
            }
        }
        result.push(LegacyShapeAnchor {
            cp,
            paragraph_index,
            shape_id,
        });
    }
    let last_cp = u32_at(plc, n * 4)?;
    if previous.is_some_and(|last| last_cp <= last) {
        return None;
    }
    Some(result)
}

// Decode the OfficeArt stream only within the FIB's explicitly specified range.
// The relationship is FSP.spid -> FOPT.pib -> BStore slot -> identical recovered
// raster bytes. If any edge is ambiguous or external, do NOT offer placement.
#[derive(Clone, Copy)]
struct ArtRecord<'a> {
    kind: u16,
    instance: u16,
    bytes: &'a [u8],
    is_container: bool,
}

fn art_records(mut bytes: &[u8]) -> Option<Vec<ArtRecord<'_>>> {
    let mut records = Vec::new();
    while !bytes.is_empty() {
        if bytes.len() < 8 || records.len() >= 4096 {
            return None;
        }
        let flags = u16::from_le_bytes(bytes[..2].try_into().ok()?);
        let kind = u16::from_le_bytes(bytes[2..4].try_into().ok()?);
        let length = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
        let end = 8usize.checked_add(length)?;
        if end > bytes.len() {
            return None;
        }
        records.push(ArtRecord {
            kind,
            instance: flags >> 4,
            is_container: (flags & 15) == 15,
            bytes: &bytes[8..end],
        });
        bytes = &bytes[end..];
    }
    Some(records)
}

fn traverse_art<'a>(bytes: &'a [u8], depth: usize, out: &mut Vec<ArtRecord<'a>>) -> Option<()> {
    if depth > 12 || out.len() > 4096 {
        return None;
    }
    for record in art_records(bytes)? {
        if record.is_container {
            traverse_art(record.bytes, depth + 1, out)?;
        }
        out.push(record);
    }
    Some(())
}

fn simple_pib(opt: ArtRecord<'_>) -> Option<u32> {
    if opt.kind != 0xF00B && opt.kind != 0xF121 {
        return None;
    }
    let count = opt.instance as usize;
    let property_size = count.checked_mul(6)?;
    let records = opt.bytes.get(..property_size)?;
    let (properties, _) = records.as_chunks::<6>();
    for prop in properties {
        let opid = u16::from_le_bytes([prop[0], prop[1]]);
        if (opid & 0x3fff) == 0x0104 && (opid & 0x8000) == 0 {
            let pib = u32::from_le_bytes(prop[2..6].try_into().ok()?);
            if pib > 0 {
                return Some(pib);
            }
        }
    }
    None
}

fn officeart_links(content: &[u8], images: &[LegacyDocImage]) -> Vec<(u32, usize)> {
    let mut all = Vec::new();
    if traverse_art(content, 0, &mut all).is_none() {
        return vec![];
    }
    // BLIP store contains one-based BSE slots in record order.
    let mut slots: Vec<Option<usize>> = Vec::new();
    fn collect_bse(
        bytes: &[u8],
        images: &[LegacyDocImage],
        slots: &mut Vec<Option<usize>>,
    ) -> Option<()> {
        for bse in art_records(bytes)? {
            if bse.kind != 0xF007 || slots.len() >= 1024 {
                continue;
            }
            // A BSE may refer to an external stream; only exact embedded JPEG/PNG
            // identity is sufficient evidence to claim a match.
            let embedded = extract_embedded_rasters(bse.bytes);
            let matches = embedded
                .iter()
                .flat_map(|candidate| {
                    images
                        .iter()
                        .enumerate()
                        .filter_map(move |(i, img)| (img.bytes == candidate.bytes).then_some(i))
                })
                .collect::<Vec<_>>();
            slots.push(if matches.len() == 1 {
                Some(matches[0])
            } else {
                None
            });
        }
        Some(())
    }
    fn search_bstore(
        bytes: &[u8],
        depth: usize,
        images: &[LegacyDocImage],
        slots: &mut Vec<Option<usize>>,
    ) -> Option<()> {
        if depth > 12 {
            return None;
        }
        for record in art_records(bytes)? {
            if record.kind == 0xF001 && record.is_container {
                collect_bse(record.bytes, images, slots)?;
            } else if record.is_container {
                search_bstore(record.bytes, depth + 1, images, slots)?;
            }
        }
        Some(())
    }
    if search_bstore(content, 0, images, &mut slots).is_none() {
        return vec![];
    }
    fn search_shapes(
        bytes: &[u8],
        depth: usize,
        slots: &[Option<usize>],
        matches: &mut Vec<(u32, usize)>,
    ) -> Option<()> {
        if depth > 12 {
            return None;
        }
        for record in art_records(bytes)? {
            if record.kind == 0xF004 && record.is_container {
                let children = art_records(record.bytes)?;
                let id = children
                    .iter()
                    .find(|c| c.kind == 0xF00A)
                    .and_then(|r| u32_at(r.bytes, 0));
                let pib = children.iter().filter_map(|r| simple_pib(*r)).next();
                if let (Some(id), Some(index)) = (id, pib) {
                    if let Some(Some(image)) = slots.get((index - 1) as usize) {
                        matches.push((id, *image));
                    }
                }
            } else if record.is_container {
                search_shapes(record.bytes, depth + 1, slots, matches)?;
            }
        }
        Some(())
    }
    let mut matches = Vec::new();
    if search_shapes(content, 0, &slots, &mut matches).is_none() {
        return vec![];
    }
    matches
}

fn read_verified_shape_images(
    word: &[u8],
    table: &[u8],
    anchors: &[LegacyShapeAnchor],
    images: &[LegacyDocImage],
) -> Vec<VerifiedShapeImage> {
    if anchors.is_empty() || images.is_empty() {
        return Vec::new();
    }
    let Some(csw) = u16_at(word, 0x20).map(usize::from) else {
        return Vec::new();
    };
    let Some(cslw_at) = 0x22usize.checked_add(csw.saturating_mul(2)) else {
        return Vec::new();
    };
    let Some(cslw) = u16_at(word, cslw_at).map(usize::from) else {
        return Vec::new();
    };
    let Some(base) = (cslw_at + 2).checked_add(cslw.saturating_mul(4)) else {
        return Vec::new();
    };
    // FibRgFcLcb97 pair 37 contains fcDggInfo / lcbDggInfo.
    if u16_at(word, base).unwrap_or(0) <= 37 {
        return Vec::new();
    }
    let pair = base + 2 + 37 * 8;
    let (Some(offset), Some(len)) = (u32_at(word, pair), u32_at(word, pair + 4)) else {
        return Vec::new();
    };
    if len == 0 || len > 32 * 1024 * 1024 {
        return Vec::new();
    }
    let offset = offset as usize;
    let Some(content) = table.get(offset..offset.saturating_add(len as usize)) else {
        return Vec::new();
    };
    let raw = officeart_links(content, images);
    let mut result = Vec::new();
    let mut used_images = std::collections::HashSet::new();
    for anchor in anchors {
        let options: Vec<_> = raw
            .iter()
            .filter(|(id, _)| *id == anchor.shape_id)
            .collect();
        if options.len() == 1 {
            let image_index = options[0].1;
            if used_images.insert(image_index) {
                result.push(VerifiedShapeImage {
                    shape_id: anchor.shape_id,
                    image_index,
                    paragraph_index: anchor.paragraph_index,
                });
            }
        }
    }
    result
}

fn read_main_story(word: &[u8], table: &[u8]) -> Result<String, DocError> {
    if u16_at(word, 0) != Some(0xA5EC) {
        return Err(DocError::InvalidStructure("invalid FIB signature"));
    }
    let csw = u16_at(word, 0x20).ok_or(DocError::InvalidStructure("missing csw"))? as usize;
    let cslw_at = 0x22usize
        .checked_add(csw * 2)
        .ok_or(DocError::InvalidStructure("invalid FIB"))?;
    let cslw = u16_at(word, cslw_at).ok_or(DocError::InvalidStructure("missing cslw"))? as usize;
    let rglw = cslw_at + 2;
    if cslw < 4 {
        return Err(DocError::Unsupported("FIB has no main-story size"));
    }
    let ccp_text =
        u32_at(word, rglw + 12).ok_or(DocError::InvalidStructure("missing ccpText"))? as usize;
    if ccp_text == 0 {
        return Err(DocError::NoRecoverableText);
    }
    if ccp_text > MAX_TEXT_CHARS {
        return Err(DocError::Unsupported("document text exceeds size limit"));
    }
    // FibRgFcLcb97 entry 33 is (fcClx, lcbClx), each of size 4.
    // Calculate the position from the FIB fields instead of assuming 0x1A2.
    let cb_rg_fc_lcb_at = rglw + cslw * 4;
    let rg_count = u16_at(word, cb_rg_fc_lcb_at)
        .ok_or(DocError::InvalidStructure("missing fc/lcb count"))? as usize;
    if rg_count <= 33 {
        return Err(DocError::Unsupported("FIB has no CLX entry"));
    }
    let clx_pair_at = cb_rg_fc_lcb_at + 2 + 33 * 8;
    let fc_clx =
        u32_at(word, clx_pair_at).ok_or(DocError::InvalidStructure("missing fcClx"))? as usize;
    let lcb_clx =
        u32_at(word, clx_pair_at + 4).ok_or(DocError::InvalidStructure("missing lcbClx"))? as usize;
    if lcb_clx == 0 {
        return Err(DocError::Unsupported("no CLX piece table"));
    }
    let clx = table
        .get(
            fc_clx
                ..fc_clx
                    .checked_add(lcb_clx)
                    .ok_or(DocError::InvalidStructure("CLX offset overflow"))?,
        )
        .ok_or(DocError::InvalidStructure("CLX outside table stream"))?;
    let mut pos = 0;
    // Optional PRCs before the Pcdt record.
    while clx.get(pos) == Some(&0x01) {
        let size =
            u16_at(clx, pos + 1).ok_or(DocError::InvalidStructure("truncated PRC"))? as usize;
        pos = pos
            .checked_add(3 + size)
            .ok_or(DocError::InvalidStructure("PRC overflow"))?;
        if pos > clx.len() {
            return Err(DocError::InvalidStructure("PRC outside CLX"));
        }
    }
    if clx.get(pos) != Some(&0x02) {
        return Err(DocError::InvalidStructure("missing Pcdt"));
    }
    let plc_len =
        u32_at(clx, pos + 1).ok_or(DocError::InvalidStructure("truncated Pcdt"))? as usize;
    pos += 5;
    let plc = clx
        .get(
            pos..pos
                .checked_add(plc_len)
                .ok_or(DocError::InvalidStructure("Pcdt overflow"))?,
        )
        .ok_or(DocError::InvalidStructure("Pcdt outside CLX"))?;
    if plc_len < 16 || !(plc_len - 4).is_multiple_of(12) {
        return Err(DocError::InvalidStructure("invalid PlcPcd length"));
    }
    let count = (plc_len - 4) / 12;
    if count > MAX_TEXT_CHARS {
        return Err(DocError::Unsupported("too many text pieces"));
    }
    let pcd_offset = (count + 1) * 4;
    let mut units: Vec<u16> = Vec::with_capacity(ccp_text.min(65536));
    for i in 0..count {
        let cp0 = u32_at(plc, i * 4).ok_or(DocError::InvalidStructure("missing CP"))? as usize;
        let cp1 =
            u32_at(plc, (i + 1) * 4).ok_or(DocError::InvalidStructure("missing CP"))? as usize;
        if cp1 < cp0 {
            return Err(DocError::InvalidStructure("unordered CPs"));
        }
        if cp0 >= ccp_text {
            break;
        } // exclude footnotes, headers and other stories
        let char_count = cp1.min(ccp_text) - cp0;
        let pcd = pcd_offset + i * 8;
        let fc = u32_at(plc, pcd + 2).ok_or(DocError::InvalidStructure("missing PCD fc"))?;
        let compressed = fc & 0x4000_0000 != 0;
        let offset = (fc & 0x3fff_ffff) as usize;
        let start = if compressed { offset / 2 } else { offset };
        let len = char_count
            .checked_mul(if compressed { 1 } else { 2 })
            .ok_or(DocError::InvalidStructure("piece length overflow"))?;
        let bytes = word
            .get(
                start
                    ..start
                        .checked_add(len)
                        .ok_or(DocError::InvalidStructure("piece offset overflow"))?,
            )
            .ok_or(DocError::InvalidStructure(
                "text piece outside WordDocument",
            ))?;
        if compressed {
            // Eight-bit Unicode / Windows-1252 special punctuation mapping, per MS-DOC.
            let (decoded, _, _) = WINDOWS_1252.decode(bytes);
            units.extend(decoded.encode_utf16());
        } else {
            units.extend(
                bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|b| u16::from_le_bytes(*b)),
            );
        }
    }
    Ok(String::from_utf16_lossy(&units))
}

/// Visible text and byte-relative link spans after evaluating legacy Word fields.
/// The three field control characters are 0x13 (begin), 0x14 (separator), 0x15 (end).
/// Only the result of a well-formed field is displayed. Unmatched field markers
/// are handled conservatively: plain visible text outside fields is retained.
#[derive(Default)]
struct FieldOutput {
    text: String,
    links: Vec<(usize, usize, String)>,
}

impl FieldOutput {
    fn append(&mut self, other: Self) {
        let offset = self.text.len();
        self.text.push_str(&other.text);
        self.links.extend(
            other
                .links
                .into_iter()
                .map(|(start, end, url)| (start + offset, end + offset, url)),
        );
    }
}

#[derive(Default)]
struct FieldFrame {
    instruction: String,
    result: FieldOutput,
    in_result: bool,
}

/// Extract an HTTP(S) URL from a Word HYPERLINK field instruction, such as
/// `HYPERLINK "https://example.com/path" \\t "_blank"`.
/// This is intentionally narrow: unsupported field types remain visible via
/// their field result, but never become clickable links.
fn hyperlink_target(instruction: &str) -> Option<String> {
    let trimmed = instruction.trim_start();
    let head = trimmed.get(..9)?;
    if !head.eq_ignore_ascii_case("HYPERLINK") {
        return None;
    }
    let rest = trimmed.get(9..)?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim_start();
    let url = if let Some(quoted) = rest.strip_prefix('"') {
        quoted.split('"').next()?
    } else {
        rest.split_whitespace().next()?
    };
    let url = url.trim();
    if url.len() <= 2048 && (url.starts_with("https://") || url.starts_with("http://")) {
        Some(url.to_owned())
    } else {
        None
    }
}

fn decode_fields(input: &str) -> (String, Vec<(usize, usize, String)>) {
    let mut root = FieldOutput::default();
    let mut stack: Vec<FieldFrame> = Vec::new();
    for ch in input.chars() {
        match ch {
            '\u{0013}' => stack.push(FieldFrame::default()),
            '\u{0014}' => {
                if let Some(frame) = stack.last_mut() {
                    frame.in_result = true;
                }
            }
            '\u{0015}' => {
                if let Some(frame) = stack.pop() {
                    // A field without its separator has no display result.
                    // Preserve nothing rather than leaking instruction text.
                    let mut visible = frame.result;
                    if frame.in_result {
                        if let Some(url) = hyperlink_target(&frame.instruction) {
                            if !visible.text.is_empty() {
                                visible.links.push((0, visible.text.len(), url));
                            }
                        }
                    }
                    if let Some(parent) = stack.last_mut() {
                        if parent.in_result {
                            parent.result.append(visible);
                        } else {
                            parent.instruction.push_str(&visible.text);
                        }
                    } else {
                        root.append(visible);
                    }
                }
            }
            _ => {
                if let Some(frame) = stack.last_mut() {
                    if frame.in_result {
                        frame.result.text.push(ch);
                    } else {
                        frame.instruction.push(ch);
                    }
                } else {
                    root.text.push(ch);
                }
            }
        }
    }
    // Truncated field: retain the visible result if one was encountered.
    while let Some(frame) = stack.pop() {
        if frame.in_result {
            if let Some(parent) = stack.last_mut() {
                if parent.in_result {
                    parent.result.append(frame.result);
                }
            } else {
                root.append(frame.result);
            }
        }
    }
    (root.text, root.links)
}

fn assign_links_to_paragraphs(
    filtered: &str,
    spans: Vec<(usize, usize, String)>,
) -> Vec<LegacyDocLink> {
    let mut result = Vec::new();
    for (start, end, url) in spans {
        if start >= end
            || end > filtered.len()
            || !filtered.is_char_boundary(start)
            || !filtered.is_char_boundary(end)
        {
            continue;
        }
        let label = filtered[start..end].trim().to_owned();
        if label.is_empty() {
            continue;
        }
        // Markup uses Word CR and cell-end 0x07 as paragraph separators.
        let paragraph_index = filtered[..start]
            .chars()
            .filter(|c| *c == '\r' || *c == '\u{0007}')
            .count();
        result.push(LegacyDocLink {
            paragraph_index,
            label,
            url,
        });
    }
    result
}

fn split_paragraphs(text: &str) -> Vec<String> {
    // Word 97 paragraph marks are CR; LF and vertical-tab are line breaks.
    // Form feed marks a page break, which is represented as a blank paragraph.
    let mut result = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        match ch {
            '\r' | '\x07' => {
                // 0x07 ends table cells in legacy Word documents
                result.push(std::mem::take(&mut current));
            }
            '\n' | '\x0b' => current.push('\n'),
            '\x0c' => {
                result.push(std::mem::take(&mut current));
                result.push(String::new());
            }
            '\t' => current.push('\t'),
            '\u{0001}'..='\u{0006}' | '\u{0008}' | '\u{000e}'..='\u{001f}' => {}
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    while result.last().is_some_and(String::is_empty) {
        result.pop();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removes_field_instruction_but_keeps_visible_hyperlink_and_url() {
        let input =
            "Текст\r\u{0013} HYPERLINK \"https://example.com/x\" \u{0014}Ссылка\u{0015}\rПосле\r";
        let (visible, spans) = decode_fields(input);
        assert_eq!(visible, "Текст\rСсылка\rПосле\r");
        let links = assign_links_to_paragraphs(&visible, spans);
        assert_eq!(
            links,
            vec![LegacyDocLink {
                paragraph_index: 1,
                label: "Ссылка".into(),
                url: "https://example.com/x".into(),
            }]
        );
    }
    #[test]
    fn ignores_non_url_fields_without_losing_results() {
        let (visible, links) = decode_fields("A\u{0013} PAGE \u{0014}12\u{0015}B");
        assert_eq!(visible, "A12B");
        assert!(links.is_empty());
    }
    #[test]
    fn nested_field_results_are_retained() {
        let (visible, _) = decode_fields("\u{0013} HYPERLINK \"https://a.test\" \u{0014}Name \u{0013} PAGE \u{0014}7\u{0015}\u{0015}");
        assert_eq!(visible, "Name 7");
    }
    #[test]
    fn non_http_hyperlinks_are_not_clickable() {
        assert!(hyperlink_target(r#"HYPERLINK "file:///c:/secret""#).is_none());
        assert!(hyperlink_target(r#"HYPERLINK "javascript:alert(1)""#).is_none());
    }
    #[test]
    fn splits_paragraphs_preserving_order() {
        assert_eq!(
            split_paragraphs("Заголовок\rСтрока 1\rСтрока 2\r"),
            vec!["Заголовок", "Строка 1", "Строка 2"]
        );
    }
    #[test]
    fn reads_piece_table_from_fib() {
        let mut word = vec![0u8; 1024];
        word[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes());
        word[0x20..0x22].copy_from_slice(&14u16.to_le_bytes());
        word[0x3e..0x40].copy_from_slice(&22u16.to_le_bytes());
        word[0x4c..0x50].copy_from_slice(&5u32.to_le_bytes());
        word[0x98..0x9a].copy_from_slice(&93u16.to_le_bytes());
        word[0x1a2..0x1a6].copy_from_slice(&0u32.to_le_bytes());
        word[0x1a6..0x1aa].copy_from_slice(&21u32.to_le_bytes());
        let text = "Тест\r";
        let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        word[512..512 + utf16.len()].copy_from_slice(&utf16);
        let mut table = vec![2u8];
        table.extend(16u32.to_le_bytes());
        table.extend(0u32.to_le_bytes());
        table.extend(5u32.to_le_bytes());
        table.extend([0u8; 2]);
        table.extend(512u32.to_le_bytes());
        table.extend([0u8; 2]);
        assert_eq!(read_main_story(&word, &table).unwrap(), "Тест\r");
    }
    #[test]
    fn extracts_only_complete_png_and_deduplicates() {
        let png: [u8; 45] = [
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0, 31, 21, 196, 137, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];
        let mut blob = Vec::from(&png[..]);
        blob.extend_from_slice(b"noise");
        blob.extend_from_slice(&png);
        assert_eq!(extract_embedded_rasters(&blob).len(), 1);
        assert!(extract_embedded_rasters(&png[..30]).is_empty());
    }
    #[test]
    fn rejects_incomplete_jpeg() {
        assert!(extract_embedded_rasters(b"\xff\xd8\xffsome bytes").is_empty());
    }
    #[test]
    fn parses_plcfspa_main_story_anchor_positions() {
        let mut word = vec![0u8; 1024];
        word[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes());
        word[0x20..0x22].copy_from_slice(&14u16.to_le_bytes());
        word[0x3e..0x40].copy_from_slice(&22u16.to_le_bytes());
        word[0x4c..0x50].copy_from_slice(&10u32.to_le_bytes());
        word[0x98..0x9a].copy_from_slice(&93u16.to_le_bytes());
        let offset = 0x98 + 2 + 40 * 8;
        word[offset..offset + 4].copy_from_slice(&0u32.to_le_bytes());
        word[offset + 4..offset + 8].copy_from_slice(&34u32.to_le_bytes());
        let mut table = vec![0u8; 34];
        table[0..4].copy_from_slice(&2u32.to_le_bytes());
        table[4..8].copy_from_slice(&10u32.to_le_bytes());
        table[8..12].copy_from_slice(&1234u32.to_le_bytes());
        let result = read_shape_anchors(&word, &table, "A\rB\r123456").unwrap();
        assert_eq!(
            result,
            vec![LegacyShapeAnchor {
                cp: 2,
                paragraph_index: 1,
                shape_id: 1234
            }]
        );
    }
    #[test]
    fn ignores_corrupted_plcfspa() {
        let mut word = vec![0u8; 1024];
        word[0..2].copy_from_slice(&0xA5ECu16.to_le_bytes());
        word[0x20..0x22].copy_from_slice(&14u16.to_le_bytes());
        word[0x3e..0x40].copy_from_slice(&22u16.to_le_bytes());
        word[0x4c..0x50].copy_from_slice(&10u32.to_le_bytes());
        word[0x98..0x9a].copy_from_slice(&93u16.to_le_bytes());
        let offset = 0x98 + 2 + 40 * 8;
        word[offset + 4..offset + 8].copy_from_slice(&35u32.to_le_bytes());
        assert!(read_shape_anchors(&word, &[0; 35], "1234567890").is_none());
    }
}

#[cfg(test)]
mod officeart_tests {
    use super::*;
    fn rec(kind: u16, flags: u16, bytes: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&flags.to_le_bytes());
        v.extend_from_slice(&kind.to_le_bytes());
        v.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        v.extend_from_slice(bytes);
        v
    }
    #[test]
    fn links_pib_to_embedded_blip_when_bytes_match() {
        // Single 1x1 PNG: the same bytes occur in BSE and recovered gallery.
        let img = include_bytes!("officeart_test.png").to_vec();
        let bse = rec(0xF007, 0, &img);
        let bstore = rec(0xF001, 0x000f, &bse);
        let fsp = rec(0xF00A, 0, &1026u32.to_le_bytes());
        let mut opt = Vec::new();
        opt.extend_from_slice(&0x0104u16.to_le_bytes());
        opt.extend_from_slice(&1u32.to_le_bytes());
        let fopt = rec(0xF00B, 0x0010, &opt);
        let shape = rec(0xF004, 0x000f, &[fsp, fopt].concat());
        let drawing = rec(0xF000, 0x000f, &[bstore, shape].concat());
        let assets = vec![LegacyDocImage {
            bytes: img,
            format: LegacyImageFormat::Png,
        }];
        assert_eq!(officeart_links(&drawing, &assets), vec![(1026, 0)]);
    }
    #[test]
    fn malformed_escher_does_not_claim_matches() {
        assert!(officeart_links(&[0xff, 0, 1], &[]).is_empty());
    }
}
