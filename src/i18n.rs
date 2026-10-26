//! Tiny runtime language switch shared by the library and the binary.
//!
//! The default is Russian. The choice is persisted next to the executable in
//! `NotOffice-language.txt` and can be changed from the View menu.
//!
//! Translations are ordinary pairs of string literals at the call site
//! (`tr!("русский", "English")`) or paired format templates
//! (`tfm!("Открыто: {}", "Open: {}", value)`). No extra crates, no maps, no
//! allocation until a message actually needs formatting.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, Ordering};

const RU: u8 = 0;
const EN: u8 = 1;

static LANGUAGE: AtomicU8 = AtomicU8::new(RU);

#[inline]
pub fn is_ru() -> bool {
    LANGUAGE.load(Ordering::Relaxed) == RU
}

pub fn set_english(english: bool) {
    LANGUAGE.store(if english { EN } else { RU }, Ordering::Relaxed);
}

/// Returns the literal matching the current interface language.
#[inline]
pub fn pick<'a>(ru: &'a str, en: &'a str) -> &'a str {
    if is_ru() {
        ru
    } else {
        en
    }
}

pub fn language_file() -> Option<PathBuf> {
    Some(crate::app_paths::file("NotOffice-language.txt"))
}

/// Reads the saved choice; a missing or unreadable file keeps Russian.
pub fn load() {
    let Some(path) = language_file() else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    set_english(text.trim().eq_ignore_ascii_case("en"));
}

pub fn save() -> std::io::Result<()> {
    let Some(path) = language_file() else {
        return Ok(());
    };
    std::fs::write(path, if is_ru() { "ru\n" } else { "en\n" })
}

#[macro_export]
macro_rules! tr {
    ($ru:literal, $en:literal $(,)?) => {
        $crate::i18n::pick($ru, $en)
    };
}

#[macro_export]
macro_rules! tfm {
    ($ru:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_ru() {
            format!($ru $(, $arg)*)
        } else {
            format!($en $(, $arg)*)
        }
    };
}
