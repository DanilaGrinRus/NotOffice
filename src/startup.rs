//! Startup argument handling kept independent from the GUI event loop.
use crate::tr;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

/// Returns the sole document path supplied to NotOffice.
///
/// `args_os` is intentionally used by the caller: it preserves Windows paths
/// containing spaces and Unicode without a lossy UTF-8 conversion.  NotOffice
/// opens one document window at a time, so multiple paths are an actionable
/// error rather than silently discarding all but the first.
pub fn document_path_from_args(
    args: impl IntoIterator<Item = OsString>,
) -> Result<Option<PathBuf>, String> {
    let mut args = args.into_iter();
    let first = match args.next() {
        None => return Ok(None),
        Some(argument) if argument == "--" => args.next().ok_or_else(|| {
            tr!(
                "Ожидался путь к документу после --",
                "Expected a document path after --"
            )
            .to_owned()
        })?,
        Some(argument) => argument,
    };
    if args.next().is_some() {
        return Err(tr!(
            "NotOffice открывает один документ за раз; укажите один путь.",
            "NotOffice can open one document at a time; use one document path."
        )
        .into());
    }
    if first.is_empty() {
        return Err(tr!("Путь к документу пуст.", "The document path is empty.").into());
    }
    Ok(Some(PathBuf::from(first)))
}

/// Builds a direct child-process command for one independent NotOffice window.
/// The document path is supplied as one OS-native argument, never interpolated
/// into a shell command, so Windows spaces and Unicode remain intact.
pub fn new_window_command(executable: OsString, document: Option<&std::path::Path>) -> Command {
    let mut command = Command::new(executable);
    if let Some(document) = document {
        command.arg(document.as_os_str());
    }
    command
}

/// Builds a command for a new independent window that starts with an empty
/// text document (Ctrl+N), not the start screen.
pub fn new_text_window_command(executable: OsString) -> Command {
    let mut command = Command::new(executable);
    command.arg("--blank-text");
    command
}

#[cfg(test)]
mod tests {
    use super::{document_path_from_args, new_text_window_command, new_window_command};
    use std::ffi::{OsStr, OsString};
    use std::path::PathBuf;

    #[test]
    fn accepts_a_unicode_path_with_spaces_as_one_argument() {
        let path = PathBuf::from(r"C:\Документы\Отчёт за октябрь.docx");
        assert_eq!(
            document_path_from_args([OsString::from(path.as_os_str())]).unwrap(),
            Some(path)
        );
    }

    #[test]
    fn accepts_a_path_after_the_argument_separator() {
        assert_eq!(
            document_path_from_args([OsString::from("--"), OsString::from("таблица.xlsx")])
                .unwrap(),
            Some(PathBuf::from("таблица.xlsx"))
        );
    }

    #[test]
    fn rejects_multiple_document_paths_instead_of_silently_ignoring_one() {
        let error =
            document_path_from_args([OsString::from("first.docx"), OsString::from("second.docx")])
                .unwrap_err();
        assert!(error.contains("один документ"));
    }

    #[test]
    fn new_window_command_keeps_a_unicode_path_as_one_os_argument() {
        let executable = OsString::from(r"C:\NotOffice\NotOffice.exe");
        let document = PathBuf::from(r"C:\Документы\Отчёт за октябрь.docx");
        let command = new_window_command(executable.clone(), Some(&document));

        assert_eq!(command.get_program(), executable);
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![document.as_os_str()]
        );
    }

    #[test]
    fn new_blank_window_command_has_no_document_argument() {
        let command = new_window_command(OsString::from("NotOffice.exe"), None);
        assert!(command.get_args().next().is_none());
    }

    #[test]
    fn new_text_window_passes_only_the_blank_text_flag() {
        let command = new_text_window_command(OsString::from("NotOffice.exe"));
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            vec![OsStr::new("--blank-text")]
        );
    }

    #[test]
    fn ten_independent_window_commands_have_no_shared_document_arguments() {
        for index in 0..10 {
            let document = PathBuf::from(format!(r"C:\Документы\Файл {index}.docx"));
            let command = new_window_command(OsString::from("NotOffice.exe"), Some(&document));
            assert_eq!(
                command.get_args().collect::<Vec<_>>(),
                vec![document.as_os_str()]
            );
        }
    }
}
