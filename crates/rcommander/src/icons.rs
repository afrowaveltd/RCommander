//! Bundled icon glyphs.
//!
//! The font is **Font Awesome Free 6 (Solid)**, redistributed with the
//! application under the SIL Open Font License 1.1; the license text ships next to
//! the font in `assets/fonts/LICENSE-FontAwesome.txt` and the icons themselves are
//! CC BY 4.0. Both allow bundling in a binary as long as attribution is kept, which
//! the About dialog and the repository do.
//!
//! The font is added to egui as the *lowest* fallback, so text keeps the normal
//! proportional font and only the private-use code points below are taken from it.

/// The icon font, compiled into the binary so it also works on Android and in a
/// single-file distribution.
pub const FONT_BYTES: &[u8] = include_bytes!("../assets/fonts/fa-solid-900.ttf");

/// Name under which the font is registered with egui.
pub const FONT_NAME: &str = "rcommander-icons";

/// Icons used by the interface. The character values are Font Awesome code points
/// from the private use area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// Closed folder (`fa-folder`).
    Folder,
    /// Open folder (`fa-folder-open`).
    FolderOpen,
    /// Generic file (`fa-file`).
    File,
    /// Image file (`fa-file-image`).
    Image,
    /// Archive file (`fa-file-zipper`).
    Archive,
    /// Source code (`fa-file-code`).
    Code,
    /// Text document (`fa-file-lines`).
    Text,
    /// Up one directory (`fa-arrow-up`).
    Up,
    /// File system root (`fa-house`).
    Root,
    /// Re-read (`fa-arrows-rotate`).
    Refresh,
    /// Search (`fa-magnifying-glass`).
    Search,
    /// Copy (`fa-copy`).
    Copy,
    /// Delete (`fa-trash-can`).
    Delete,
    /// Edit (`fa-pen`).
    Edit,
    /// Settings (`fa-gear`).
    Settings,
    /// Archive/pack (`fa-box-archive`).
    Pack,
    /// Synchronise (`fa-right-left`).
    Sync,
    /// Terminal (`fa-terminal`).
    Terminal,
    /// Agent chat (`fa-robot`).
    Robot,
    /// Folder tab (`fa-table-columns`).
    Tab,
}

impl Icon {
    /// The character that renders this icon.
    pub fn glyph(self) -> char {
        match self {
            Icon::Folder => '\u{f07b}',
            Icon::FolderOpen => '\u{f07c}',
            Icon::File => '\u{f15b}',
            Icon::Image => '\u{f1c5}',
            Icon::Archive => '\u{f1c6}',
            Icon::Code => '\u{f1c9}',
            Icon::Text => '\u{f15c}',
            Icon::Up => '\u{f062}',
            Icon::Root => '\u{f015}',
            Icon::Refresh => '\u{f021}',
            Icon::Search => '\u{f002}',
            Icon::Copy => '\u{f0c5}',
            Icon::Delete => '\u{f2ed}',
            Icon::Edit => '\u{f304}',
            Icon::Settings => '\u{f013}',
            Icon::Pack => '\u{f187}',
            Icon::Sync => '\u{f362}',
            Icon::Terminal => '\u{f120}',
            Icon::Robot => '\u{f544}',
            Icon::Tab => '\u{f0db}',
        }
    }

    /// Icon for a directory entry, derived from the kind and the extension.
    pub fn for_entry(kind: rc_core::entry::EntryKind, name: &str) -> Icon {
        use rc_core::entry::EntryKind;
        match kind {
            EntryKind::Dir | EntryKind::SymlinkToDir => return Icon::Folder,
            _ => {}
        }
        let extension = std::path::Path::new(name)
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match extension.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "ico" | "tif" | "tiff" | "svg" => {
                Icon::Image
            }
            "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "cab" | "jar" => {
                Icon::Archive
            }
            "rs" | "c" | "h" | "cpp" | "hpp" | "py" | "js" | "ts" | "go" | "java" | "cs" | "sh"
            | "toml" | "json" | "yml" | "yaml" | "xml" | "html" | "css" => Icon::Code,
            "txt" | "md" | "rst" | "log" | "csv" => Icon::Text,
            _ => Icon::File,
        }
    }

    /// Every icon, for tests that check the font really contains the glyph.
    pub fn all() -> [Icon; 20] {
        [
            Icon::Folder,
            Icon::FolderOpen,
            Icon::File,
            Icon::Image,
            Icon::Archive,
            Icon::Code,
            Icon::Text,
            Icon::Up,
            Icon::Root,
            Icon::Refresh,
            Icon::Search,
            Icon::Copy,
            Icon::Delete,
            Icon::Edit,
            Icon::Settings,
            Icon::Pack,
            Icon::Sync,
            Icon::Terminal,
            Icon::Robot,
            Icon::Tab,
        ]
    }
}

/// Icon for a tool-bar command, if the command has one.
pub fn for_command(command: &str) -> Option<Icon> {
    match command.to_ascii_lowercase().as_str() {
        "cm_copy" | "cm_copysamepanel" => Some(Icon::Copy),
        "cm_renmov" => Some(Icon::Copy),
        "cm_delete" | "cm_deletepermanent" => Some(Icon::Delete),
        "cm_edit" | "cm_editnewfile" => Some(Icon::Edit),
        "cm_searchfor" | "cm_searchstandalone" => Some(Icon::Search),
        "cm_packfiles" => Some(Icon::Pack),
        "cm_unpackfiles" => Some(Icon::Pack),
        "cm_filesync" | "cm_synchronizedirs" => Some(Icon::Sync),
        "cm_config" => Some(Icon::Settings),
        "cm_rereadsource" => Some(Icon::Refresh),
        "cm_gotoparent" => Some(Icon::Up),
        "cm_executedos" => Some(Icon::Terminal),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_font_is_bundled_and_looks_like_a_truetype_file() {
        assert!(
            FONT_BYTES.len() > 100_000,
            "font bytes: {}",
            FONT_BYTES.len()
        );
        // A TrueType file starts with 0x00010000 or "true"/"OTTO".
        let head = &FONT_BYTES[..4];
        assert!(
            head == [0x00, 0x01, 0x00, 0x00] || head == *b"true" || head == *b"OTTO",
            "unexpected font header: {head:?}"
        );
    }

    #[test]
    fn every_icon_is_a_private_use_character() {
        for icon in Icon::all() {
            let glyph = icon.glyph();
            let code = glyph as u32;
            // Font Awesome puts its icons in the private use area (0xE000-0xF8FF).
            assert!(
                (0xE000..=0xF8FF).contains(&code),
                "{icon:?} is U+{code:04X}"
            );
        }
    }

    #[test]
    fn entry_icons_follow_the_kind_and_extension() {
        use rc_core::entry::EntryKind;
        assert_eq!(Icon::for_entry(EntryKind::Dir, "anything"), Icon::Folder);
        assert_eq!(
            Icon::for_entry(EntryKind::SymlinkToDir, "link"),
            Icon::Folder
        );
        assert_eq!(Icon::for_entry(EntryKind::File, "photo.PNG"), Icon::Image);
        assert_eq!(
            Icon::for_entry(EntryKind::File, "backup.tar.gz"),
            Icon::Archive
        );
        assert_eq!(Icon::for_entry(EntryKind::File, "main.rs"), Icon::Code);
        assert_eq!(Icon::for_entry(EntryKind::File, "notes.md"), Icon::Text);
        assert_eq!(Icon::for_entry(EntryKind::File, "noext"), Icon::File);
        assert_eq!(Icon::for_entry(EntryKind::File, ".bashrc"), Icon::File);
    }

    #[test]
    fn commands_map_to_icons_where_it_makes_sense() {
        assert_eq!(for_command("cm_Copy"), Some(Icon::Copy));
        assert_eq!(for_command("cm_FileSync"), Some(Icon::Sync));
        assert_eq!(for_command("cm_SrcShort"), None);
    }
}
