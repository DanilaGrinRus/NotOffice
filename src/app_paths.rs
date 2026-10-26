//! Where NotOffice keeps settings, recent files, language and recovery.
//!
//! Windows keeps the portable layout next to the EXE. macOS and Linux use the
//! platform data directory so an installed package does not need write access
//! to `/usr/bin` or `/Applications`.
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    if let Some(explicit) =
        std::env::var_os("NOTOFFICE_DATA_DIR").or_else(|| std::env::var_os("LITEOFFICE_DATA_DIR"))
    {
        return PathBuf::from(explicit);
    }
    #[cfg(windows)]
    {
        return std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("."));
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        return home
            .join("Library")
            .join("Application Support")
            .join("NotOffice");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("notoffice");
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        return home.join(".local").join("share").join("notoffice");
    }
    #[allow(unreachable_code)]
    PathBuf::from(".")
}

/// Creates the data directory and returns it.
pub fn ensure_data_dir() -> std::io::Result<PathBuf> {
    let directory = data_dir();
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

/// Path to a per-user file such as `NotOffice-recent.txt`.
pub fn file(name: &str) -> PathBuf {
    let directory = data_dir();
    let _ = std::fs::create_dir_all(&directory);
    directory.join(name)
}

#[cfg(not(windows))]
fn legacy_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        return home
            .join("Library")
            .join("Application Support")
            .join("LiteOffice");
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return PathBuf::from(xdg).join("liteoffice");
        }
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        return home.join(".local").join("share").join("liteoffice");
    }
    #[allow(unreachable_code)]
    data_dir()
}

/// Moves data written by the old LiteOffice name to NotOffice paths.
pub fn migrate_legacy_data() {
    #[cfg(windows)]
    {
        let directory = data_dir();
        for (old, new) in [
            ("LiteOffice-settings.txt", "NotOffice-settings.txt"),
            ("LiteOffice-recent.txt", "NotOffice-recent.txt"),
            ("LiteOffice-language.txt", "NotOffice-language.txt"),
        ] {
            let old = directory.join(old);
            let new = directory.join(new);
            if !new.exists() && old.is_file() {
                let _ = std::fs::rename(old, new);
            }
        }
        let old = directory.join("LiteOffice-recovery");
        let new = directory.join("NotOffice-recovery");
        if !new.exists() && old.is_dir() {
            let _ = std::fs::rename(old, new);
        }
    }
    #[cfg(not(windows))]
    {
        let new = data_dir();
        let old = legacy_data_dir();
        if !new.exists() && old.exists() {
            if let Some(parent) = new.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::rename(old, new);
        }
    }
}
