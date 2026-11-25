//! Finding the fonts already installed on this machine.
//!
//! RCommander ships with egui's built-in fonts, which are readable but not *yours*. This module
//! looks for `ttf`/`otf` files in the places the platform keeps them and turns each file into a
//! family name, so the options dialog can offer a list instead of asking for a path.
//!
//! Two honest limits:
//!
//! * The family name is guessed from the **file name** (`DejaVuSansMono-Bold.ttf` → *DejaVu Sans
//!   Mono*), because parsing the name table of every font on a machine is a lot of machinery for a
//!   picker. A file whose name says nothing useful is skipped instead of being offered as
//!   something it is not.
//! * Only files that actually **start** like a font are kept ([`looks_like_font`]): egui parses the
//!   bytes lazily, so handing it a stray `.ttf` extension would turn into a panic on the first
//!   frame. Checking the four magic bytes costs nothing and removes that class of crash.

use std::path::{Path, PathBuf};

/// How deep to walk under each font directory; real installations are shallow.
const MAX_DEPTH: usize = 4;

/// How many files to look at, so a pathological fonts directory cannot stall a start-up scan.
const MAX_FILES: usize = 4000;

/// One font file offered in the picker.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FontFile {
    /// Family name as shown to the user (`DejaVu Sans Mono`).
    pub family: String,
    /// Where the bytes come from.
    pub path: PathBuf,
    /// True when the name suggests a monospaced face.
    pub mono: bool,
    /// True for the regular weight, which is the one to load for body text.
    pub regular: bool,
}

impl FontFile {
    /// The label in the picker: the family, plus a hint when the file is not the regular weight.
    pub fn label(&self) -> String {
        if self.regular {
            self.family.clone()
        } else {
            format!("{} (not regular)", self.family)
        }
    }
}

/// The directories a platform keeps fonts in, in the order they should be scanned.
///
/// Missing directories are simply skipped, so the list can be generous.
pub fn font_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = crate::drive::home_dir() {
        #[cfg(unix)]
        {
            dirs.push(home.join(".local/share/fonts"));
            dirs.push(home.join(".fonts"));
            dirs.push(home.join("Library/Fonts"));
        }
        #[cfg(windows)]
        {
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                dirs.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
            }
        }
    }
    #[cfg(unix)]
    {
        dirs.push(PathBuf::from("/usr/share/fonts"));
        dirs.push(PathBuf::from("/usr/local/share/fonts"));
        dirs.push(PathBuf::from("/Library/Fonts"));
        dirs.push(PathBuf::from("/System/Library/Fonts"));
    }
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("SystemRoot") {
            dirs.push(PathBuf::from(root).join("Fonts"));
        }
    }
    dirs
}

/// Scan the platform's font directories.
pub fn discover() -> Vec<FontFile> {
    discover_in(&font_dirs())
}

/// Scan `dirs` (the tests use their own).
pub fn discover_in(dirs: &[PathBuf]) -> Vec<FontFile> {
    let mut found: Vec<FontFile> = Vec::new();
    let mut budget = MAX_FILES;
    for dir in dirs {
        walk(dir, 0, &mut found, &mut budget);
    }
    // One entry per family, preferring the regular weight; sorted so the picker is stable.
    found.sort_by(|a, b| {
        a.family
            .to_lowercase()
            .cmp(&b.family.to_lowercase())
            .then_with(|| b.regular.cmp(&a.regular))
            .then_with(|| a.path.cmp(&b.path))
    });
    found.dedup_by(|a, b| a.family == b.family);
    found
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<FontFile>, budget: &mut usize) {
    if depth > MAX_DEPTH || *budget == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    children.sort();
    for path in children {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        if path.is_dir() {
            walk(&path, depth + 1, out, budget);
            continue;
        }
        if let Some(font) = describe(&path) {
            out.push(font);
        }
    }
}

/// Turn one file into a [`FontFile`], or `None` when it is not something we can offer.
pub fn describe(path: &Path) -> Option<FontFile> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    // `.ttc` is a collection: ab_glyph would need to pick a face out of it, so it is not offered.
    if extension != "ttf" && extension != "otf" {
        return None;
    }
    let stem = path.file_stem()?.to_string_lossy().to_string();
    let (family, mono, regular) = parse_file_name(&stem)?;
    // Only offer files that can actually be parsed as a font; see the module docs.
    if !looks_like_font(path) {
        return None;
    }
    Some(FontFile {
        family,
        path: path.to_path_buf(),
        mono,
        regular,
    })
}

/// Guess a family name, its weight, and whether the face is monospaced from a file stem.
///
/// Returns `None` for names that carry no family (`ab12cd.ttf`), because offering "Ab12cd" as a
/// font would be a lie.
pub fn parse_file_name(stem: &str) -> Option<(String, bool, bool)> {
    // A name with no separator, no capital *and* digits says nothing about its family (`ab12cd`),
    // while `consola` is simply a lowercase name and stays.
    if !stem.contains(['-', '_', ' '])
        && !stem.chars().any(|c| c.is_uppercase())
        && stem.chars().any(|c| c.is_ascii_digit())
    {
        return None;
    }
    // Two spellings coexist: `DejaVuSansMono-Bold` and `DejaVu_Sans_Mono_Bold`.
    let normalised = stem.replace(['-', '_'], " ");
    let lower = normalised.to_ascii_lowercase();
    let mut regular = true;
    // Weight, slant and width words are stripped: they belong to the file, not the family.
    let mut words: Vec<String> = Vec::new();
    for word in lower.split_whitespace() {
        match word {
            "bold" | "light" | "semibold" | "demibold" | "black" | "thin" | "heavy" | "medium"
            | "extrabold" | "extralight" | "ultralight" | "book" => regular = false,
            "italic" | "oblique" | "ital" | "condensed" | "expanded" | "narrow" | "wide"
            | "regular" | "normal" | "roman" => {}
            _ => words.push(word.to_string()),
        }
    }
    if words.is_empty() {
        return None;
    }
    let mono = words
        .iter()
        .any(|word| word.contains("mono") || word.contains("courier") || word.contains("consol"));
    // Names with separators keep their words; a word written without them (`DejaVuSansMono`) is
    // split on its capitals, and each word is cleaned of its weight words.
    let family = normalised
        .split_whitespace()
        .map(split_camel_case)
        .flat_map(|word| {
            word.split_whitespace()
                .map(|part| part.to_string())
                .collect::<Vec<String>>()
        })
        .filter(|word| {
            !matches!(
                word.to_ascii_lowercase().as_str(),
                "bold"
                    | "light"
                    | "italic"
                    | "oblique"
                    | "regular"
                    | "normal"
                    | "medium"
                    | "black"
                    | "thin"
                    | "heavy"
                    | "book"
                    | "semibold"
                    | "demibold"
                    | "extrabold"
                    | "extralight"
                    | "ultralight"
                    | "condensed"
                    | "expanded"
                    | "narrow"
                    | "wide"
                    | "roman"
            )
        })
        .collect::<Vec<String>>()
        .join(" ");
    let family = family.trim().to_string();
    if family.is_empty() || family.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((family, mono, regular))
}

/// `DejaVuSansMono` → `DejaVu Sans Mono`, `NotoSans` → `Noto Sans`.
///
/// A capitalised run that ends a short lowercase tail is glued to the word before it, because
/// names like *DejaVu* and *macOS* are one word; *Sans* and *Mono* are long enough to stand alone.
/// The rule is a heuristic, and the only thing that depends on it is how nice the picker reads —
/// the family is matched by the string the picker showed.
fn split_camel_case(text: &str) -> String {
    let words = split_runs(text);
    words.join(" ")
}

/// Split `text` into capitalisation runs, gluing short ones to the previous word.
fn split_runs(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut current = String::new();
    for (index, c) in chars.iter().copied().enumerate() {
        if c.is_uppercase() && index > 0 && chars[index - 1].is_lowercase() {
            // How long is this run, and how much lowercase follows it?
            let tail = chars[index + 1..]
                .iter()
                .take_while(|next| next.is_lowercase())
                .count();
            let glue = tail <= 2;
            if glue {
                current.push(c);
                continue;
            }
            words.push(std::mem::take(&mut current));
        }
        current.push(c);
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// True when the file starts with one of the font magic numbers egui can parse.
pub fn looks_like_font(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 4];
    if file.read_exact(&mut head).is_err() {
        return false;
    }
    matches!(
        &head,
        // TrueType outlines, CFF/OpenType, and the Apple `true` tag.
        [0x00, 0x01, 0x00, 0x00] | [b'O', b'T', b'T', b'O'] | [b't', b'r', b'u', b'e']
    )
}

/// The regular face of `family`, when it was found.
pub fn find<'a>(fonts: &'a [FontFile], family: &str) -> Option<&'a FontFile> {
    let wanted = family.trim().to_lowercase();
    if wanted.is_empty() {
        return None;
    }
    fonts
        .iter()
        .find(|font| font.family.to_lowercase() == wanted && font.regular)
        .or_else(|| {
            fonts
                .iter()
                .find(|font| font.family.to_lowercase() == wanted)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_name_becomes_a_readable_family() {
        assert_eq!(
            parse_file_name("DejaVuSansMono-Bold"),
            Some(("DejaVu Sans Mono".to_string(), true, false))
        );
        assert_eq!(
            parse_file_name("DejaVuSans"),
            Some(("DejaVu Sans".to_string(), false, true))
        );
        assert_eq!(
            parse_file_name("Noto_Sans_Mono_Regular"),
            Some(("Noto Sans Mono".to_string(), true, true))
        );
        assert_eq!(
            parse_file_name("LiberationSerif-Italic"),
            Some(("Liberation Serif".to_string(), false, true))
        );
        // Monospace is recognised from the usual hints.
        assert!(parse_file_name("CourierNew").unwrap().1);
        assert!(parse_file_name("consola").unwrap().1);
        // A name that says nothing is not offered: "Ab12cd" would be a lie.
        assert_eq!(parse_file_name("ab12cd"), None);
        assert_eq!(parse_file_name("1234"), None);
        assert_eq!(parse_file_name("Bold"), None);
    }

    #[test]
    fn camel_case_names_are_split_on_the_capitals() {
        assert_eq!(split_camel_case("DejaVuSansMono"), "DejaVu Sans Mono");
        assert_eq!(split_camel_case("ABCDef"), "ABCDef");
        assert_eq!(split_camel_case("NotoSans"), "Noto Sans");
    }

    #[test]
    fn a_scan_finds_fonts_and_skips_anything_else() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("nested")).unwrap();
        // A real TrueType header, then nothing: enough for the magic check.
        let write = |path: &std::path::Path, head: &[u8]| {
            std::fs::write(path, head).unwrap();
        };
        write(&root.join("MyFont-Regular.ttf"), b"\x00\x01\x00\x00rest");
        write(&root.join("OtherMono.ttf"), b"\x00\x01\x00\x00rest");
        write(&root.join("nested/DeepSans.otf"), b"OTTOrest");
        // Not a font: a text file with a font extension, a collection, an unrelated file.
        write(&root.join("Broken.ttf"), b"this is not a font");
        write(&root.join("Collection.ttc"), b"ttcfrest");
        write(&root.join("notes.txt"), b"hello");

        let found = discover_in(&[root.to_path_buf()]);
        let families: Vec<&str> = found.iter().map(|font| font.family.as_str()).collect();
        assert!(families.contains(&"My Font"), "{families:?}");
        assert!(families.contains(&"Other Mono"), "{families:?}");
        assert!(families.contains(&"Deep Sans"), "{families:?}");
        assert!(!families.contains(&"Broken"), "a broken file is skipped");
        assert!(!families.contains(&"Collection"), "a collection is skipped");
        assert!(!families.contains(&"notes"), "an unrelated file is skipped");
        // Sorted, so the picker does not jump around between scans.
        let mut sorted = families.clone();
        sorted.sort_by_key(|name| name.to_lowercase());
        assert_eq!(families, sorted);
        // The regular face is preferred when both weights exist.
        write(&root.join("MyFont-Bold.ttf"), b"\x00\x01\x00\x00rest");
        let found = discover_in(&[root.to_path_buf()]);
        let my = find(&found, "my font").expect("the family is found case-insensitively");
        assert!(my.regular, "the regular weight is the one offered: {my:?}");
    }

    #[test]
    fn the_magic_check_recognises_the_real_formats() {
        let dir = tempfile::tempdir().unwrap();
        for (name, head, expected) in [
            ("truetype.ttf", &b"\x00\x01\x00\x00"[..], true),
            ("opentype.otf", &b"OTTO"[..], true),
            ("apple.ttf", &b"true"[..], true),
            ("short.ttf", &b"ab"[..], false),
            ("text.ttf", &b"hello"[..], false),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, head).unwrap();
            assert_eq!(looks_like_font(&path), expected, "{name}");
        }
        assert!(!looks_like_font(&dir.path().join("missing.ttf")));
    }

    #[test]
    fn the_platform_directories_are_plausible() {
        let dirs = font_dirs();
        assert!(!dirs.is_empty(), "there is always somewhere to look");
        // Scanning the real machine must not fail, whatever is installed.
        let fonts = discover();
        for font in fonts.iter().take(5) {
            assert!(!font.family.is_empty(), "{font:?}");
            assert!(
                font.path.extension().is_some(),
                "only files are offered: {font:?}"
            );
        }
        // Nothing is lost by scanning the same list twice.
        let again = discover_in(&font_dirs());
        assert_eq!(fonts.len(), again.len());
    }
}
