//! Passive plain text, Markdown source and basic RTF text. No markup execution,
//! RichEdit/Office automation, embedded objects, URLs or document actions.
use crate::docx::{
    create_temporary_peer, paths_refer_to_same_file, replace_destination, source_fingerprint,
    DocxError, SourceFingerprint,
};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const LIMIT: usize = 8 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TextKind {
    Txt,
    Markdown,
    Rtf,
}
impl TextKind {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Markdown => "md",
            Self::Rtf => "rtf",
        }
    }
    fn of(path: &Path) -> Result<Self, DocxError> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "txt" => Ok(Self::Txt),
            "md" => Ok(Self::Markdown),
            "rtf" => Ok(Self::Rtf),
            _ => Err(DocxError::UnsafeSave("choose TXT, MD or basic RTF".into())),
        }
    }
}
pub struct TextDocument {
    pub text: String,
    pub kind: TextKind,
    path: Option<PathBuf>,
    protected_origin: Option<PathBuf>,
    fingerprint: Option<SourceFingerprint>,
    saved_text: String,
}
impl Default for TextDocument {
    fn default() -> Self {
        Self {
            text: String::new(),
            kind: TextKind::Txt,
            path: None,
            protected_origin: None,
            fingerprint: None,
            saved_text: String::new(),
        }
    }
}
impl TextDocument {
    /// Recovery restores the passive text model directly, never reparses its
    /// contents as RTF/Markdown actions and never adopts the original path.
    pub fn recovered_copy(kind: TextKind, text: String, origin: Option<PathBuf>) -> Self {
        Self {
            text,
            kind,
            path: None,
            protected_origin: origin,
            fingerprint: None,
            saved_text: String::new(),
        }
    }
    pub fn from_isolated(kind: TextKind, text: String, path: &Path) -> Result<Self, DocxError> {
        let fingerprint = source_fingerprint(path)?;
        Ok(Self {
            saved_text: text.clone(),
            text,
            kind,
            path: Some(path.to_path_buf()),
            protected_origin: (kind == TextKind::Rtf).then(|| path.to_path_buf()),
            fingerprint: Some(fingerprint),
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    pub fn requires_save_as(&self) -> bool {
        self.path.is_none() || self.protected_origin.is_some()
    }
    pub fn is_dirty(&self) -> bool {
        self.text != self.saved_text
    }
    pub fn open(path: &Path) -> Result<Self, DocxError> {
        let before = source_fingerprint(path)?;
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take((LIMIT + 1) as u64)
            .read_to_end(&mut bytes)?;
        let kind = TextKind::of(path)?;
        let text = decode(kind, &bytes)?;
        if source_fingerprint(path)? != before {
            return Err(DocxError::SourceChanged);
        }
        Ok(Self {
            saved_text: text.clone(),
            text,
            kind,
            path: Some(path.to_path_buf()),
            protected_origin: (kind == TextKind::Rtf).then(|| path.to_path_buf()),
            fingerprint: Some(before),
        })
    }
    pub fn save(&mut self, path: &Path) -> Result<(), DocxError> {
        let kind = TextKind::of(path)?;
        if let Some(origin) = &self.protected_origin {
            if paths_refer_to_same_file(origin, path)? {
                return Err(DocxError::UnsafeSave(
                    "original RTF is protected; save a separate basic RTF/TXT/MD copy".into(),
                ));
            }
        }
        if self.text.len() > LIMIT {
            return Err(DocxError::ResourceLimit("text exceeds 8 MiB".into()));
        }
        let replaces = self
            .path
            .as_deref()
            .map(|p| paths_refer_to_same_file(p, path))
            .transpose()?
            .unwrap_or(false);
        let check = || -> Result<(), DocxError> {
            if replaces && self.fingerprint.as_ref() != Some(&source_fingerprint(path)?) {
                return Err(DocxError::SourceChanged);
            }
            Ok(())
        };
        check()?;
        let bytes = if kind == TextKind::Rtf {
            encode_rtf(&self.text).into_bytes()
        } else {
            self.text.as_bytes().to_vec()
        };
        if bytes.len() > LIMIT {
            return Err(DocxError::ResourceLimit(
                "encoded text exceeds 8 MiB".into(),
            ));
        }
        if decode(kind, &bytes)? != self.text {
            return Err(DocxError::UnsafeSave("text did not round-trip".into()));
        }
        let (temp, mut file) = create_temporary_peer(path)?;
        let result = (|| -> Result<(), DocxError> {
            file.write_all(&bytes)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            if std::fs::read(&temp)? != bytes {
                return Err(DocxError::UnsafeSave(
                    "temporary text verification failed".into(),
                ));
            }
            check()?;
            replace_destination(&temp, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result?;
        self.path = Some(path.to_path_buf());
        self.kind = kind;
        self.protected_origin = None;
        self.fingerprint = Some(source_fingerprint(path)?);
        self.saved_text = self.text.clone();
        Ok(())
    }
}
fn decode(kind: TextKind, bytes: &[u8]) -> Result<String, DocxError> {
    if bytes.len() > LIMIT {
        return Err(DocxError::ResourceLimit("text exceeds 8 MiB".into()));
    }
    if kind == TextKind::Rtf {
        return decode_rtf(bytes);
    }
    let (encoding, start) = if bytes.starts_with(&[0xff, 0xfe]) {
        (Some(encoding_rs::UTF_16LE), 2)
    } else if bytes.starts_with(&[0xfe, 0xff]) {
        (Some(encoding_rs::UTF_16BE), 2)
    } else {
        (None, 0)
    };
    let text = if let Some(encoding) = encoding {
        let (text, _, errors) = encoding.decode(&bytes[start..]);
        if errors {
            return Err(DocxError::InvalidPackage("invalid UTF-16 text".into()));
        }
        text.into_owned()
    } else {
        std::str::from_utf8(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))
            .map_err(|_| DocxError::InvalidPackage("text must be UTF-8 or UTF-16 with BOM".into()))?
            .to_owned()
    };
    if text.contains('\0') {
        return Err(DocxError::InvalidPackage("binary NUL in text".into()));
    }
    Ok(text)
}

#[derive(Clone, Copy)]
struct RtfState {
    skip: bool,
    uc: usize,
    page: u16,
}
fn decode_rtf(bytes: &[u8]) -> Result<String, DocxError> {
    let invalid = || DocxError::InvalidPackage("invalid or unsupported basic RTF".into());
    if !bytes.starts_with(b"{\\rtf") {
        return Err(invalid());
    }
    let mut state = RtfState {
        skip: false,
        uc: 1,
        page: 1252,
    };
    let mut stack = Vec::new();
    let mut out = Vec::<u16>::new();
    let mut at = 0;
    let mut fallback = 0;
    fn emit(out: &mut Vec<u16>, state: RtfState, fallback: &mut usize, text: &str) {
        if *fallback > 0 {
            *fallback -= 1;
        } else if !state.skip {
            out.extend(text.encode_utf16());
        }
    }
    while at < bytes.len() {
        let ch = bytes[at];
        at += 1;
        match ch {
            b'{' => {
                if stack.len() >= 128 {
                    return Err(invalid());
                }
                stack.push(state);
            }
            b'}' => {
                state = stack.pop().ok_or_else(invalid)?;
                if stack.is_empty() && bytes[at..].iter().any(|b| !b.is_ascii_whitespace()) {
                    return Err(invalid());
                }
            }
            b'\r' | b'\n' => {}
            b'\\' => {
                let next = *bytes.get(at).ok_or_else(invalid)?;
                at += 1;
                if next == b'*' {
                    state.skip = true;
                    continue;
                }
                if next == b'\'' {
                    let hex = bytes.get(at..at + 2).ok_or_else(invalid)?;
                    let s = std::str::from_utf8(hex).map_err(|_| invalid())?;
                    let value = u8::from_str_radix(s, 16).map_err(|_| invalid())?;
                    at += 2;
                    let encoding = if state.page == 1251 {
                        encoding_rs::WINDOWS_1251
                    } else {
                        encoding_rs::WINDOWS_1252
                    };
                    let data = [value];
                    let (s, _, _) = encoding.decode(&data);
                    emit(&mut out, state, &mut fallback, &s);
                    continue;
                }
                if !next.is_ascii_alphabetic() {
                    let value = match next {
                        b'\\' => "\\",
                        b'{' => "{",
                        b'}' => "}",
                        b'~' => "\u{a0}",
                        b'_' => "\u{2011}",
                        b'-' => "\u{ad}",
                        _ => "",
                    };
                    if !value.is_empty() {
                        emit(&mut out, state, &mut fallback, value);
                    }
                    continue;
                }
                let start = at - 1;
                while at < bytes.len() && bytes[at].is_ascii_alphabetic() {
                    at += 1;
                }
                let word = std::str::from_utf8(&bytes[start..at]).map_err(|_| invalid())?;
                let number_start = at;
                if bytes.get(at) == Some(&b'-') {
                    at += 1;
                }
                while at < bytes.len() && bytes[at].is_ascii_digit() {
                    at += 1;
                }
                let number = if number_start != at {
                    Some(
                        std::str::from_utf8(&bytes[number_start..at])
                            .map_err(|_| invalid())?
                            .parse::<i32>()
                            .map_err(|_| invalid())?,
                    )
                } else {
                    None
                };
                if bytes.get(at) == Some(&b' ') {
                    at += 1;
                }
                match word {
                    "fonttbl" | "colortbl" | "stylesheet" | "info" | "pict" | "object"
                    | "objdata" | "datastore" | "themedata" | "fldinst" | "header" | "footer"
                    | "filetbl" | "revtbl" | "listtable" | "listoverridetable" | "generator" => {
                        state.skip = true
                    }
                    "ansicpg" => {
                        let n = number.ok_or_else(invalid)?;
                        if !matches!(n, 1251 | 1252) {
                            return Err(invalid());
                        }
                        state.page = n as u16;
                    }
                    "uc" => {
                        let n = number.ok_or_else(invalid)?;
                        if !(0..=16).contains(&n) {
                            return Err(invalid());
                        }
                        state.uc = n as usize;
                    }
                    "u" => {
                        let n = number.ok_or_else(invalid)?;
                        if !(-32768..=65535).contains(&n) {
                            return Err(invalid());
                        }
                        if !state.skip {
                            out.push(n as u16);
                        }
                        fallback = state.uc;
                    }
                    "bin" => {
                        let n =
                            usize::try_from(number.ok_or_else(invalid)?).map_err(|_| invalid())?;
                        at = at
                            .checked_add(n)
                            .filter(|v| *v <= bytes.len())
                            .ok_or_else(invalid)?;
                    }
                    "par" | "line" => emit(&mut out, state, &mut fallback, "\n"),
                    "tab" => emit(&mut out, state, &mut fallback, "\t"),
                    _ => {}
                }
            }
            value => {
                if stack.is_empty() {
                    if !value.is_ascii_whitespace() {
                        return Err(invalid());
                    }
                    continue;
                }
                let data = [value];
                let encoding = if state.page == 1251 {
                    encoding_rs::WINDOWS_1251
                } else {
                    encoding_rs::WINDOWS_1252
                };
                let (s, _, _) = encoding.decode(&data);
                emit(&mut out, state, &mut fallback, &s);
            }
        }
        if out.len() > LIMIT {
            return Err(invalid());
        }
    }
    if !stack.is_empty() {
        return Err(invalid());
    }
    Ok(String::from_utf16_lossy(&out))
}
fn encode_rtf(text: &str) -> String {
    let mut out = String::from("{\\rtf1\\ansi\\ansicpg1252\\uc1 ");
    for unit in text.encode_utf16() {
        match unit {
            92 | 123 | 125 => {
                out.push('\\');
                out.push(char::from_u32(unit as u32).unwrap());
            }
            10 => out.push_str("\\par\n"),
            9 => out.push_str("\\tab "),
            32..=126 => out.push(char::from_u32(unit as u32).unwrap()),
            _ => out.push_str(&format!("\\u{}?", unit as i16)),
        }
    }
    out.push('}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic_rtf_roundtrips_cyrillic_surrogates_tabs_and_literal_controls() {
        let text = "Ёжик 🙂\nText {braces} \\ \t русский\r\n";
        assert_eq!(decode_rtf(encode_rtf(text).as_bytes()).unwrap(), text);
        assert_eq!(
            decode_rtf(br"{\rtf1\ansi\ansicpg1251 \'c5\'e6\'e8\'ea}").unwrap(),
            "Ежик"
        );
        assert_eq!(
            decode_rtf(br"{\rtf1\uc1 \u1046?\u1080?\u1074?}").unwrap(),
            "Жив"
        );
    }
    #[test]
    fn rtf_ignores_objects_instructions_and_unknown_destinations_without_running_them() {
        let text=decode_rtf(br"{\rtf1 Visible{\object\objclass Package{\*\objdata 001122 powershell.exe}}{\field{\*\fldinst DDEAUTO cmd.exe}{\fldrslt Result}}{\*\unknown scripts} End}").unwrap();
        assert_eq!(text, "VisibleResult End");
        for bytes in [
            b"not rtf".as_slice(),
            br"{\rtf1 unfinished",
            br"{\rtf1\bin999 data}",
            br"{\rtf1\uc99 bad}",
            br"{\rtf1\ansicpg999 bad}",
        ] {
            assert!(decode_rtf(bytes).is_err());
        }
        let deep = format!("{{\\rtf1 {}{} }}", "{".repeat(128), "}".repeat(128));
        assert!(decode_rtf(deep.as_bytes()).is_err());
    }
    #[test]
    fn plain_utf8_utf16_markdown_are_literal_and_invalid_binary_is_rejected() {
        let md = "# Заголовок\n<script>do_not_run()</script>\n[link](javascript:do_not_run())";
        assert_eq!(decode(TextKind::Markdown, md.as_bytes()).unwrap(), md);
        let mut utf16 = vec![0xff, 0xfe];
        utf16.extend("Ёжик".encode_utf16().flat_map(u16::to_le_bytes));
        assert_eq!(decode(TextKind::Txt, &utf16).unwrap(), "Ёжик");
        assert!(decode(TextKind::Txt, b"binary\0data").is_err());
        assert!(decode(TextKind::Txt, &[0xff, 0x42]).is_err());
    }
    #[test]
    fn text_save_as_backups_and_conflict_keep_unsaved_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        let mut text = TextDocument {
            text: "Первое".into(),
            ..Default::default()
        };
        text.save(&path).unwrap();
        assert!(!text.is_dirty());
        assert!(!text.requires_save_as());
        let mut second = TextDocument::open(&path).unwrap();
        text.text = "Внешнее изменение".into();
        text.save(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(path.with_extension("txt.bak")).unwrap(),
            "Первое"
        );
        second.text = "Не терять".into();
        assert!(matches!(second.save(&path), Err(DocxError::SourceChanged)));
        assert!(second.is_dirty());
        assert_eq!(second.text, "Не терять");
        let copy = dir.path().join("copy.md");
        second.save(&copy).unwrap();
        assert_eq!(TextDocument::open(&copy).unwrap().text, "Не терять");
        assert!(second.save(&dir.path().join("missing/bad.txt")).is_err());
    }
    #[test]
    fn rtf_original_and_alias_are_protected_and_basic_copy_is_valid() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("source.rtf");
        let bytes = br"{\rtf1{\b Bold} Text{\object hidden}}";
        std::fs::write(&origin, bytes).unwrap();
        let mut doc = TextDocument::open(&origin).unwrap();
        assert_eq!(doc.text, "Bold Text");
        doc.text.push_str(" Ёжик");
        assert!(doc.requires_save_as());
        assert!(doc.save(&origin).is_err());
        let alias = dir.path().join("alias.rtf");
        std::fs::hard_link(&origin, &alias).unwrap();
        assert!(doc.save(&alias).is_err());
        let copy = dir.path().join("copy.rtf");
        doc.save(&copy).unwrap();
        assert_eq!(TextDocument::open(&copy).unwrap().text, doc.text);
        assert_eq!(std::fs::read(origin).unwrap(), bytes);
        assert!(!String::from_utf8(std::fs::read(copy).unwrap())
            .unwrap()
            .contains("object"));
    }
    #[test]
    #[ignore = "explicit export of new TXT/MD/RTF manual fixtures"]
    fn export_manual_text_fixtures() {
        let folder =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("manual_phase4_artifacts/text-rc11");
        std::fs::create_dir(&folder).unwrap();
        let text = "Ёжик: русский текст.\nSecond line\n";
        std::fs::write(folder.join("text-test.txt"), text).unwrap();
        std::fs::write(folder.join("markdown-test.md"),"# Заголовок\n\n**Текст** и [ссылка](https://example.invalid).\n<script>literal_only()</script>\n").unwrap();
        std::fs::write(folder.join("rtf-test.rtf"), encode_rtf(text)).unwrap();
        println!("Created {}", folder.display());
    }
}
