//! Portable recent-document list storage and ordering.
use std::path::{Path, PathBuf};

pub const MAX_RECENT_PATHS: usize = 8;

pub fn load_paths(path: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(path)
        .ok()
        .map(|contents| {
            contents
                .lines()
                .map(PathBuf::from)
                .filter(|path| path.is_file())
                .take(MAX_RECENT_PATHS)
                .collect()
        })
        .unwrap_or_default()
}

pub fn save_paths(path: &Path, paths: &[PathBuf]) -> std::io::Result<()> {
    std::fs::write(
        path,
        paths
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

pub fn remember_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    paths.retain(|recent| recent != &path);
    paths.insert(0, path);
    paths.truncate(MAX_RECENT_PATHS);
}

#[cfg(test)]
mod tests {
    use super::remember_path;
    use std::path::PathBuf;

    #[test]
    fn recent_paths_are_deduplicated_and_keep_the_newest_entry_first() {
        let first = PathBuf::from("C:/Документы/первый файл.docx");
        let second = PathBuf::from("C:/Документы/второй файл.xlsx");
        let mut paths = vec![first.clone(), second.clone()];

        remember_path(&mut paths, first.clone());

        assert_eq!(paths, vec![first, second]);
    }

    #[test]
    fn recent_paths_are_bounded_to_eight_entries() {
        let mut paths = (0..8)
            .map(|index| PathBuf::from(format!("document-{index}.docx")))
            .collect::<Vec<_>>();

        remember_path(&mut paths, PathBuf::from("new.docx"));

        assert_eq!(paths.len(), 8);
        assert_eq!(paths.first(), Some(&PathBuf::from("new.docx")));
        assert!(!paths.contains(&PathBuf::from("document-7.docx")));
    }
}
