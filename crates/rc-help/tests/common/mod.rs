//! Shared `tempfile` fixture that mirrors the real `Docs/` layout.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use rc_help::HelpIndex;
use tempfile::TempDir;

/// Writes `contents` to `dir/relative`, creating parent folders.
pub fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture folder");
    }
    fs::write(&path, contents).expect("write fixture file");
}

/// Writes raw bytes to `dir/relative`, creating parent folders.
pub fn write_bytes(dir: &Path, relative: &str, contents: &[u8]) {
    let path = dir.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture folder");
    }
    fs::write(&path, contents).expect("write fixture file");
}

/// A temporary help tree plus the `Docs` root inside it.
pub struct Fixture {
    temp: TempDir,
    docs: PathBuf,
}

impl Fixture {
    /// Build the standard fixture tree:
    ///
    /// ```text
    /// Docs/
    ///   README.md                     (stray markdown in the root: never a topic)
    ///   .hidden/en.md                 (hidden folder: ignored)
    ///   Previewer/{en,cs}.md, notes.txt
    ///   FTP connections/en.md
    ///   01 - Keyboard shortcuts/en.md
    ///   No heading/en.md
    ///   Only German/de.md
    ///   Settings/{en,cs}.md
    ///   Advanced/Packing/en.md
    /// ```
    pub fn new() -> Fixture {
        let temp = TempDir::new().expect("tempdir");
        let docs = temp.path().join("Docs");

        write(
            &docs,
            "Previewer/en.md",
            "# Previewer\n\
             \n\
             Open any file with **F3**; `Esc` closes the viewer.\n\
             \n\
             - read only\n\
             - remembers the position\n\
             \n\
             See [Settings](../Settings/en.md) and [Keyboard shortcuts](../01 - Keyboard shortcuts/en.md).\n",
        );
        write(
            &docs,
            "Previewer/cs.md",
            "# Prohlížeč\n\
             \n\
             Soubor otevřete klávesou **F3**.\n\
             \n\
             Viz [Nastavení](../Settings/cs.md).\n",
        );
        write(
            &docs,
            "Previewer/notes.txt",
            "not markdown, must be ignored\n",
        );
        write(
            &docs,
            "FTP connections/en.md",
            "# FTP connections\n\
             \n\
             Connect to **remote** servers.\n\
             \n\
             Use the [settings page](../Settings/en.md) first.\n",
        );
        write(
            &docs,
            "01 - Keyboard shortcuts/en.md",
            "# Keyboard shortcuts\n\
             \n\
             Press **F1** to open this help.\n\
             \n\
             Press **F3** to open the previewer.\n",
        );
        write(
            &docs,
            "No heading/en.md",
            "This file has no heading at all.\n\nJust a second paragraph.\n",
        );
        write(
            &docs,
            "Only German/de.md",
            "# Nur Deutsch\n\nDieser Hilfeartikel ist nur auf Deutsch verfügbar.\n",
        );
        write(
            &docs,
            "Settings/en.md",
            "# Settings\n\nConfigure the app here.\n",
        );
        write(
            &docs,
            "Settings/cs.md",
            "# Nastavení\n\nZde aplikaci nastavíte.\n",
        );
        write(
            &docs,
            "Advanced/Packing/en.md",
            "# Packing\n\nPacking uses the archive engine.\n",
        );
        write(
            &docs,
            "README.md",
            "# Index\n\nA stray markdown file in the root.\n",
        );
        write(
            &docs,
            ".hidden/en.md",
            "# Hidden\n\nMust never be indexed.\n",
        );

        Fixture { temp, docs }
    }

    /// Root directory of the fixture (`…/Docs`).
    pub fn docs(&self) -> &Path {
        &self.docs
    }

    /// Top-level temporary directory, for building odd trees.
    pub fn temp(&self) -> &Path {
        self.temp.path()
    }

    /// Load the index of the fixture tree.
    pub fn index(&self) -> HelpIndex {
        HelpIndex::load(&self.docs).expect("load fixture index")
    }

    /// Topic ids in index order.
    pub fn ids(&self) -> Vec<String> {
        self.index()
            .topics()
            .iter()
            .map(|topic| topic.id.clone())
            .collect()
    }
}

/// Topic ids of the standard fixture, in index order.
pub const EXPECTED_ORDER: &[&str] = &[
    "Keyboard shortcuts",
    "FTP connections",
    "No heading",
    "Only German",
    "Packing",
    "Previewer",
    "Settings",
];
