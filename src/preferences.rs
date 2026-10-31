//! Portable view and print preference serialization.
use crate::print::{ImageScale, PageSettings};
use crate::theme::Theme;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const MAX_SETTINGS_BYTES: u64 = 64 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub const MIN_VIEW_ZOOM: f32 = 0.5;
pub const MAX_VIEW_ZOOM: f32 = 4.0;
pub const DEFAULT_VIEW_ZOOM: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecimalSeparator {
    #[default]
    Comma,
    Dot,
}

impl DecimalSeparator {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Comma => "comma",
            Self::Dot => "dot",
        }
    }

    pub fn from_slug(value: &str) -> Self {
        match value {
            "dot" => Self::Dot,
            _ => Self::Comma,
        }
    }
}

pub fn sanitize_zoom(zoom: f32) -> f32 {
    if zoom.is_finite() {
        zoom.clamp(MIN_VIEW_ZOOM, MAX_VIEW_ZOOM)
    } else {
        DEFAULT_VIEW_ZOOM
    }
}

pub fn parse(contents: &str) -> (PageSettings, f32, Theme, DecimalSeparator) {
    let mut settings = PageSettings::default();
    let mut zoom = DEFAULT_VIEW_ZOOM;
    let mut theme = Theme::Light;
    let mut decimal_separator = DecimalSeparator::Comma;
    for line in contents.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "landscape" => settings.landscape = value.trim() == "true",
                "margin_mm" => {
                    if let Ok(value) = value.parse::<f32>() {
                        settings.margin_mm = value;
                    }
                }
                "image_scale" => {
                    settings.image_scale = match value.trim() {
                        "original" => ImageScale::OriginalSize,
                        "fit" => ImageScale::FitToWidth,
                        _ => ImageScale::CustomPercent,
                    }
                }
                "page_numbers" => settings.print_page_numbers = value.trim() == "true",
                "zoom" => {
                    if let Ok(value) = value.parse::<f32>() {
                        zoom = value;
                    }
                }
                "theme" => theme = Theme::from_slug(value.trim()),
                "decimal_separator" => {
                    decimal_separator = DecimalSeparator::from_slug(value.trim())
                }
                _ => {}
            }
        }
    }
    (
        settings.sanitized(),
        sanitize_zoom(zoom),
        theme,
        decimal_separator,
    )
}

pub fn load(path: &Path) -> (PageSettings, f32, Theme, DecimalSeparator) {
    read_current(path)
        .map(|contents| parse(&contents))
        .unwrap_or_else(|_| {
            (
                PageSettings::default(),
                DEFAULT_VIEW_ZOOM,
                Theme::Light,
                DecimalSeparator::Comma,
            )
        })
}

pub fn save(
    path: &Path,
    settings: PageSettings,
    zoom: f32,
    theme: Theme,
    decimal_separator: DecimalSeparator,
) -> std::io::Result<()> {
    update_keys(
        path,
        &serialized_values(settings, zoom, theme, decimal_separator),
    )
}

fn serialized_values(
    settings: PageSettings,
    zoom: f32,
    theme: Theme,
    decimal_separator: DecimalSeparator,
) -> [(&'static str, String); 7] {
    let settings = settings.sanitized();
    let scale = match settings.image_scale {
        ImageScale::OriginalSize => "original",
        ImageScale::FitToWidth => "fit",
        ImageScale::CustomPercent => "percent",
    };
    [
        ("landscape", settings.landscape.to_string()),
        ("margin_mm", format!("{:.2}", settings.margin_mm)),
        ("image_scale", scale.to_owned()),
        ("page_numbers", settings.print_page_numbers.to_string()),
        ("zoom", format!("{:.3}", sanitize_zoom(zoom))),
        ("theme", theme.slug().to_owned()),
        ("decimal_separator", decimal_separator.slug().to_owned()),
    ]
}

/// Persist only fields changed in this window. Unchanged local fields may be
/// stale: re-read and merge under an OS lock instead of writing their snapshot.
pub fn save_changes(
    path: &Path,
    before: (PageSettings, f32, Theme, DecimalSeparator),
    after: (PageSettings, f32, Theme, DecimalSeparator),
) -> std::io::Result<()> {
    let before = serialized_values(before.0, before.1, before.2, before.3);
    let after = serialized_values(after.0, after.1, after.2, after.3);
    let changes: Vec<_> = after
        .into_iter()
        .zip(before)
        .filter_map(|(after, before)| (after.1 != before.1).then_some(after))
        .collect();
    if changes.is_empty() {
        return Ok(());
    }
    update_keys(path, &changes)
}

fn read_current(path: &Path) -> io::Result<String> {
    let mut bytes = Vec::new();
    match File::open(path) {
        Ok(file) => {
            file.take(MAX_SETTINGS_BYTES + 1).read_to_end(&mut bytes)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(String::new()),
        Err(error) => return Err(error),
    }
    if bytes.len() as u64 > MAX_SETTINGS_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "settings exceed 64 KiB",
        ));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn update_keys(path: &Path, changes: &[(&str, String)]) -> io::Result<()> {
    let mut lock_name = path.as_os_str().to_owned();
    lock_name.push(".lock");
    let lock = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(Path::new(&lock_name))?;
    let started = Instant::now();
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if started.elapsed() < Duration::from_millis(500) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(TryLockError::WouldBlock) => {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "settings are busy in another window; retry",
                ))
            }
            Err(TryLockError::Error(error)) => return Err(error),
        }
    }
    // Keep this separate lock file: unlinking it could split writer ownership.
    let current = read_current(path)?;
    let mut written = vec![false; changes.len()];
    let mut output = String::new();
    for line in current.lines() {
        let update = line.split_once('=').and_then(|(key, _)| {
            changes
                .iter()
                .position(|(changed, _)| key.trim() == *changed)
        });
        if let Some(index) = update {
            if written[index] {
                continue;
            }
            written[index] = true;
            let (key, value) = &changes[index];
            output.push_str(&format!("{key}={value}\n"));
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    for (index, (key, value)) in changes.iter().enumerate() {
        if !written[index] {
            output.push_str(&format!("{key}={value}\n"));
        }
    }
    if output.len() as u64 > MAX_SETTINGS_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "settings exceed 64 KiB",
        ));
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let mut name = path
        .file_name()
        .ok_or_else(|| io::Error::other("settings path has no filename"))?
        .to_os_string();
    name.push(format!(
        ".{}.{stamp}.{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = path.with_file_name(name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(output.as_bytes())?;
        file.sync_all()?;
        drop(file);
        replace_settings(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(windows))]
fn replace_settings(temporary: &Path, path: &Path) -> io::Result<()> {
    fs::rename(temporary, path)
}

#[cfg(windows)]
fn replace_settings(temporary: &Path, path: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load, parse, save, save_changes, DecimalSeparator};
    use crate::theme::Theme;

    #[test]
    fn stale_window_zoom_change_does_not_reset_new_theme_or_page_settings() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.txt");
        let before = (
            crate::print::PageSettings::default(),
            1.0,
            Theme::Light,
            DecimalSeparator::Comma,
        );
        save(&path, before.0, before.1, before.2, before.3).unwrap();
        let mut first = before;
        first.0.landscape = true;
        first.2 = Theme::DarkBlue;
        save_changes(&path, before, first).unwrap();
        let mut stale = before;
        stale.1 = 2.0;
        save_changes(&path, before, stale).unwrap();
        let actual = load(&path);
        assert_eq!(actual.2, Theme::DarkBlue);
        assert!(actual.0.landscape);
        assert_eq!(actual.1, 2.0);
    }

    #[test]
    fn each_changed_page_field_merges_without_overwriting_other_windows() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.txt");
        let before = (
            crate::print::PageSettings::default(),
            1.0,
            Theme::Light,
            DecimalSeparator::Comma,
        );
        let mut first = before;
        first.0.landscape = true;
        first.0.image_scale = crate::print::ImageScale::FitToWidth;
        save_changes(&path, before, first).unwrap();
        let mut stale = before;
        stale.0.margin_mm = 23.0;
        stale.0.print_page_numbers = false;
        save_changes(&path, before, stale).unwrap();
        let mut theme_only = before;
        theme_only.2 = Theme::Sage;
        save_changes(&path, before, theme_only).unwrap();
        let (settings, zoom, theme, _separator) = load(&path);
        assert!(settings.landscape);
        assert_eq!(settings.image_scale, crate::print::ImageScale::FitToWidth);
        assert_eq!(settings.margin_mm, 23.0);
        assert!(!settings.print_page_numbers);
        assert_eq!((zoom, theme), (1.0, Theme::Sage));
    }

    #[test]
    fn unknown_settings_survive_and_unchanged_windows_do_not_write() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.txt");
        let before = (
            crate::print::PageSettings::default(),
            1.0,
            Theme::Light,
            DecimalSeparator::Comma,
        );
        let contents = "# user comment\nfuture_key=keep me\ntheme=light\ntheme=light\n";
        std::fs::write(&path, contents).unwrap();
        save_changes(&path, before, before).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), contents);
        save_changes(
            &path,
            before,
            (before.0, before.1, Theme::DarkBlue, before.3),
        )
        .unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("# user comment\nfuture_key=keep me\n"));
        assert_eq!(contents.matches("theme=").count(), 1);
        assert_eq!(load(&path).2, Theme::DarkBlue);
        assert!(!folder.path().read_dir().unwrap().any(|entry| entry
            .unwrap()
            .path()
            .extension()
            .is_some_and(|extension| extension == "tmp")));
    }

    #[test]
    fn busy_lock_and_invalid_settings_leave_previous_file_untouched() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.txt");
        std::fs::write(&path, "theme=dark-blue\n").unwrap();
        let lock = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(folder.path().join("settings.txt.lock"))
            .unwrap();
        lock.try_lock().unwrap();
        let before = (
            crate::print::PageSettings::default(),
            1.0,
            Theme::Light,
            DecimalSeparator::Comma,
        );
        let after = (before.0, 2.0, before.2, before.3);
        assert_eq!(
            save_changes(&path, before, after).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "theme=dark-blue\n");
        drop(lock);
        save_changes(&path, before, after).unwrap();
        assert_eq!(load(&path).2, Theme::DarkBlue);
        for invalid in [vec![0xff], vec![b'x'; 65537]] {
            std::fs::write(&path, &invalid).unwrap();
            assert!(save_changes(&path, before, after).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), invalid);
        }
    }

    #[test]
    fn preference_process_worker() {
        let Some(directory) = std::env::var_os("NOTOFFICE_PREFERENCE_TEST_DIR") else {
            return;
        };
        let directory = std::path::PathBuf::from(directory);
        let field = std::env::var("NOTOFFICE_PREFERENCE_TEST_FIELD").unwrap();
        let before = (
            crate::print::PageSettings::default(),
            1.0,
            Theme::Light,
            DecimalSeparator::Comma,
        );
        let mut after = before;
        match field.as_str() {
            "theme" => after.2 = Theme::DarkBlue,
            "zoom" => after.1 = 2.5,
            "margin" => after.0.margin_mm = 22.0,
            "orientation" => after.0.landscape = true,
            _ => panic!("unknown test field"),
        }
        std::fs::write(directory.join(format!("ready-{field}")), b"").unwrap();
        let started = std::time::Instant::now();
        while !directory.join("go").exists() {
            assert!(started.elapsed() < std::time::Duration::from_secs(20));
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        save_changes(&directory.join("settings.txt"), before, after).unwrap();
    }

    #[test]
    fn four_independent_processes_merge_their_changes() {
        let folder = tempfile::tempdir().unwrap();
        let fields = ["theme", "zoom", "margin", "orientation"];
        let mut children: Vec<_> = fields
            .iter()
            .map(|field| {
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "preferences::tests::preference_process_worker"])
                    .env("NOTOFFICE_PREFERENCE_TEST_DIR", folder.path())
                    .env("NOTOFFICE_PREFERENCE_TEST_FIELD", field)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::piped())
                    .spawn()
                    .unwrap()
            })
            .collect();
        let started = std::time::Instant::now();
        while !fields
            .iter()
            .all(|field| folder.path().join(format!("ready-{field}")).exists())
        {
            if started.elapsed() > std::time::Duration::from_secs(15) {
                for child in &mut children {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                panic!("settings test workers did not start");
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::fs::write(folder.path().join("go"), b"").unwrap();
        for child in children {
            let output = child.wait_with_output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let (settings, zoom, theme, _separator) = load(&folder.path().join("settings.txt"));
        assert_eq!(theme, Theme::DarkBlue);
        assert_eq!(zoom, 2.5);
        assert_eq!(settings.margin_mm, 22.0);
        assert!(settings.landscape);
    }

    #[test]
    fn decimal_separator_defaults_to_comma_and_roundtrips_dot() {
        let (_, _, _, default_separator) = parse("theme=light\n");
        assert_eq!(default_separator, DecimalSeparator::Comma);
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.txt");
        save(
            &path,
            crate::print::PageSettings::default(),
            1.0,
            Theme::Light,
            DecimalSeparator::Dot,
        )
        .unwrap();
        assert_eq!(load(&path).3, DecimalSeparator::Dot);
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("decimal_separator=dot"));
    }

    #[test]
    fn reads_existing_settings_format_and_ignores_unknown_keys() {
        let (settings, zoom, theme, _separator) = parse(
            "landscape=true\nmargin_mm=20\nimage_scale=fit\npage_numbers=true\nzoom=2.5\nfuture_key=value\n",
        );

        assert!(settings.landscape);
        assert_eq!(settings.margin_mm, 20.0);
        assert_eq!(settings.image_scale, crate::print::ImageScale::FitToWidth);
        assert!(settings.print_page_numbers);
        assert_eq!(zoom, 2.5);
        assert_eq!(theme, Theme::Light);
    }

    #[test]
    fn every_theme_roundtrips_without_changing_other_settings() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("settings.txt");
        let settings = crate::print::PageSettings {
            landscape: true,
            margin_mm: 21.0,
            print_page_numbers: true,
            ..Default::default()
        };
        for theme in Theme::ALL {
            save(&path, settings, 1.75, theme, DecimalSeparator::Comma).unwrap();
            let (actual_settings, zoom, actual_theme, _separator) = load(&path);
            assert_eq!(actual_settings, settings.sanitized());
            assert_eq!(zoom, 1.75);
            assert_eq!(actual_theme, theme);
            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.contains("theme="));
            assert!(contents.contains("margin_mm=21.00"));
        }
    }

    #[test]
    fn damaged_or_unknown_theme_uses_light_and_keeps_other_values() {
        let (settings, zoom, theme, _separator) = parse(
            "landscape=true\nmargin_mm=18\nimage_scale=fit\npage_numbers=true\nzoom=1.4\ntheme=no-such-theme\n",
        );
        assert!(settings.landscape);
        assert_eq!(settings.margin_mm, 18.0);
        assert_eq!(zoom, 1.4);
        assert_eq!(theme, Theme::Light);

        let (_, _, missing, _separator) = parse("landscape=false\nzoom=1.0\n");
        assert_eq!(missing, Theme::Light);
    }

    #[test]
    fn invalid_values_fall_back_to_sanitized_defaults() {
        let (settings, zoom, theme, _separator) =
            parse("margin_mm=not-a-number\nzoom=not-a-number\n");

        assert_eq!(
            settings.margin_mm,
            crate::print::PageSettings::default().margin_mm
        );
        assert_eq!(zoom, 1.0);
        assert_eq!(theme, Theme::Light);
    }
}
