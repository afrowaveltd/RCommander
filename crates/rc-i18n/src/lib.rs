//! `rc-i18n` — the language-pack engine of RCommander.
//!
//! Every string the GUI shows comes from here. The vocabulary lives in
//! human-editable TOML files that a translator can drop into the `locales/`
//! directory at runtime — no compilation, no external tooling:
//!
//! ```toml
//! # RCommander language pack — Czech
//! [meta]
//! code = "cs"
//! name = "Czech"
//! native_name = "Čeština"
//! direction = "ltr"          # optional, auto-detected from the code
//! version = 1
//! authors = ["Somebody"]
//!
//! [strings]
//! "app.title" = "RCommander"
//! "menu.file" = "Soubor"
//! "status.selected" = "{count} vybráno z {total}"
//! ```
//!
//! Design rules that the rest of the application relies on:
//!
//! * **The interface never shows an empty label.** Lookups fall back from the
//!   current language to English (compiled in as [`BUILTIN_EN`]) and finally to
//!   the key itself, so a half-translated pack degrades to English and a typo
//!   shows up as `dialog.copy.title` instead of a blank button.
//! * **Packs are validated, not trusted.** A typo in a key, a non-string value
//!   or an empty translation is an error the "check language pack" dialog can
//!   show, not a silent hole in the UI.
//! * **No panics on user input.** Everything that can fail returns
//!   [`I18nError`]; a broken pack never takes the file manager down.
//! * **Direction is data.** Right-to-left languages are detected from the code
//!   (or stated explicitly in the pack), and individual strings can ask for
//!   their own direction with [`text_is_rtl`] / [`detect_text_direction`].
//!
//! ```no_run
//! use std::path::Path;
//! use rc_i18n::{Locale, Translator, english};
//!
//! // Start-up: built-in English plus every pack the user dropped in.
//! let locales = Translator::load_dir(Path::new("locales")).unwrap_or_default();
//! let mut translator = Translator::new(english(), locales);
//! translator.set_language("cs");
//!
//! // Then, throughout the UI:
//! let title = translator.t("dialog.copy.title");
//! let status = translator.t_args("panel.selection_of", &[("count", "3"), ("total", "7")]);
//! let rtl = translator.direction().is_rtl();
//! # let _ = (title, status, rtl, Locale::load);
//! ```

#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

pub mod direction;
pub mod error;
pub mod locale;
pub mod translator;
pub mod validate;

pub use direction::{
    Direction, base_code, detect_text_direction, direction_for_code, is_ltr_char, is_rtl_char,
    text_is_rtl,
};
pub use error::{I18nError, I18nResult};
pub use locale::{Locale, LocaleMeta};
pub use translator::{BUILTIN_EN, Translator, english, substitute};
pub use validate::validate_pack;
