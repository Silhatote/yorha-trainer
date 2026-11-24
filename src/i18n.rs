//! Portuguese and English. On automatic, the language follows Windows (Portuguese if the UI
//! language or the regional format is Portuguese; otherwise, English).
//!
//! Each text lives where it is used, in both languages: `tr!("Ligado", "On")`, and with
//! variables `trf!("{n} inimigos", "{n} enemies")`.

use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

static ENGLISH: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    /// Follows the Windows language.
    #[default]
    Auto,
    Pt,
    En,
}

/// Is the text in English right now?
pub fn is_english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

/// Applies the language chosen in the settings.
pub fn set_language(language: Language) {
    let en = match language {
        Language::Auto => !windows_in_portuguese(),
        Language::Pt => false,
        Language::En => true,
    };
    ENGLISH.store(en, Ordering::Relaxed);
}

#[cfg(windows)]
pub fn windows_in_portuguese() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
        fn GetUserDefaultLCID() -> u32;
    }
    const LANG_PORTUGUESE: u32 = 0x16;
    // SAFETY: no arguments; they only read the UI language and the regional format.
    let (interface, regional) =
        unsafe { (GetUserDefaultUILanguage() as u32, GetUserDefaultLCID()) };
    (interface & 0x3ff) == LANG_PORTUGUESE || (regional & 0x3ff) == LANG_PORTUGUESE
}

#[cfg(not(windows))]
pub fn windows_in_portuguese() -> bool {
    std::env::var("LANG").is_ok_and(|l| l.starts_with("pt"))
}

/// A text in both languages: `tr!("Ligado", "On")`.
macro_rules! tr {
    ($pt:expr, $en:expr $(,)?) => {
        (if $crate::i18n::is_english() { $en } else { $pt })
    };
}

/// A text with variables in both languages, like `format!`.
macro_rules! trf {
    ($pt:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        (if $crate::i18n::is_english() {
            format!($en $(, $arg)*)
        } else {
            format!($pt $(, $arg)*)
        })
    };
}
