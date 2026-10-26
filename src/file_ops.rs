//! Shared document-path classification for dialogs, drag-and-drop, and CLI.
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    Doc,
    Docx,
    Xls,
    Xlsx,
    Ods,
    Xlsb,
    Csv,
    Pdf,
    Txt,
    Markdown,
    Rtf,
}

pub fn document_kind(path: &Path) -> Result<DocumentKind, String> {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or_else(|| "The selected file has no supported extension.".to_owned())?;
    match extension.to_ascii_lowercase().as_str() {
        "doc" => Ok(DocumentKind::Doc),
        "docx" => Ok(DocumentKind::Docx),
        "xls" => Ok(DocumentKind::Xls),
        "xlsx" => Ok(DocumentKind::Xlsx),
        "ods" => Ok(DocumentKind::Ods),
        "xlsb" => Ok(DocumentKind::Xlsb),
        "csv" => Ok(DocumentKind::Csv),
        "tsv" => Ok(DocumentKind::Csv),
        "pdf" => Ok(DocumentKind::Pdf),
        "txt" => Ok(DocumentKind::Txt),
        "md" => Ok(DocumentKind::Markdown),
        "rtf" => Ok(DocumentKind::Rtf),
        _ => Err("Only DOCX, DOC, XLSX, XLS, XLSB, ODS, CSV, TSV, PDF, TXT, MD and RTF files are supported; scripts and macro-enabled extensions are not accepted.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{document_kind, DocumentKind};
    use std::path::Path;

    #[test]
    fn recognizes_supported_extensions_case_insensitively_on_unicode_paths() {
        assert_eq!(
            document_kind(Path::new(r"C:\Документы\Отчёт.XlSX")).unwrap(),
            DocumentKind::Xlsx
        );
    }

    #[test]
    fn script_macro_enabled_and_double_executable_extensions_are_not_openable() {
        for name in [
            "test.docm",
            "test.dotm",
            "test.xlsm",
            "test.xlam",
            "test.js",
            "test.vbs",
            "test.ps1",
            "test.hta",
            "test.html",
            "test.cmd",
            "test.pdf.js",
            "test.docx.exe",
        ] {
            assert!(document_kind(Path::new(name)).is_err(), "{name}");
        }
    }

    #[test]
    fn rejects_unsupported_and_extensionless_paths_with_actionable_errors() {
        assert_eq!(
            document_kind(Path::new("letter.PDF")),
            Ok(DocumentKind::Pdf)
        );
        assert!(document_kind(Path::new("letter.docm"))
            .unwrap_err()
            .contains("DOCX"));
        assert!(document_kind(Path::new("no-extension"))
            .unwrap_err()
            .contains("extension"));
    }
}
