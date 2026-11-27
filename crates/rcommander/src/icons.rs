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
    /// Brief view (`fa-list`).
    List,
    /// Full/detail view (`fa-table-list`).
    TableList,
    /// Multi-rename (`fa-tags`).
    Tags,
    /// Compare directories (`fa-scale-balanced`).
    Compare,
    /// Hotlist / bookmark (`fa-bookmark`).
    Bookmark,
    /// Disk space (`fa-hard-drive`).
    HardDrive,
    /// New folder (`fa-folder-plus`).
    FolderPlus,
    /// Download (`fa-download`).
    Download,
    /// Network / server (`fa-network-wired`).
    Network,
    /// Save (`fa-floppy-disk`).
    Save,
    /// View / eye (`fa-eye`).
    Eye,
    /// Split file (`fa-scissors`).
    Scissors,
    /// Checksum / fingerprint (`fa-fingerprint`).
    Fingerprint,
    /// Encrypt / lock (`fa-lock`).
    Lock,
    /// Selection / check (`fa-square-check`).
    Check,
    /// History (`fa-clock-rotate-left`).
    History,
    /// Help (`fa-circle-question`).
    Help,
    /// Exit (`fa-right-from-bracket`).
    Exit,
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
            Icon::List => '\u{f03a}',
            Icon::TableList => '\u{f00b}',
            Icon::Tags => '\u{f02c}',
            Icon::Compare => '\u{f24e}',
            Icon::Bookmark => '\u{f02e}',
            Icon::HardDrive => '\u{f0a0}',
            Icon::FolderPlus => '\u{f65e}',
            Icon::Download => '\u{f019}',
            Icon::Network => '\u{f6ff}',
            Icon::Save => '\u{f0c7}',
            Icon::Eye => '\u{f06e}',
            Icon::Scissors => '\u{f0c4}',
            Icon::Fingerprint => '\u{f577}',
            Icon::Lock => '\u{f023}',
            Icon::Check => '\u{f14a}',
            Icon::History => '\u{f1da}',
            Icon::Help => '\u{f059}',
            Icon::Exit => '\u{f08b}',
        }
    }

    /// Which theme role tints this icon.
    ///
    /// The grouping is semantic, not decorative: destructive actions use the
    /// warning colour, creating things the archive/green one, navigation the
    /// accent, and quiet commands the muted one. Because the tints are roles, a
    /// theme decides how bright they are — the dark theme's red is lighter than
    /// the light theme's, for example.
    pub fn colour_role(self) -> rc_core::theme::Role {
        use rc_core::theme::Role;
        match self {
            // Destructive: deleting, and anything that loses data.
            Icon::Delete => Role::Warning,
            // Creating or writing: folders, archives, saving, checksums.
            Icon::FolderPlus | Icon::Pack | Icon::Save | Icon::Fingerprint | Icon::Scissors => {
                Role::Archive
            }
            // Moving data between places: copy, synchronise, compare.
            Icon::Copy | Icon::Sync | Icon::Compare | Icon::Download => Role::Directory,
            // Navigation and view switching.
            Icon::Up
            | Icon::Root
            | Icon::Refresh
            | Icon::Tab
            | Icon::List
            | Icon::TableList
            | Icon::Eye
            | Icon::History
            | Icon::Bookmark
            | Icon::Folder
            | Icon::FolderOpen
            | Icon::HardDrive => Role::Accent,
            // Searching and filtering is a different kind of attention.
            Icon::Search | Icon::Check => Role::Symlink,
            // Network and credentials.
            Icon::Network | Icon::Lock => Role::Symlink,
            // Editing and metadata.
            Icon::Edit | Icon::Tags | Icon::Text | Icon::Code | Icon::Image => Role::Directory,
            // Quiet, secondary commands.
            Icon::Settings
            | Icon::Help
            | Icon::Exit
            | Icon::Terminal
            | Icon::Robot
            | Icon::Archive
            | Icon::File => Role::Hidden,
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
    pub fn all() -> [Icon; 38] {
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
            Icon::List,
            Icon::TableList,
            Icon::Tags,
            Icon::Compare,
            Icon::Bookmark,
            Icon::HardDrive,
            Icon::FolderPlus,
            Icon::Download,
            Icon::Network,
            Icon::Save,
            Icon::Eye,
            Icon::Scissors,
            Icon::Fingerprint,
            Icon::Lock,
            Icon::Check,
            Icon::History,
            Icon::Help,
            Icon::Exit,
        ]
    }
}

/// Icon for a tool-bar command, if the command has one.
///
/// The catalogue spells commands with `cm_Left…`/`cm_Right…`/`cm_Src…` prefixes
/// for the same action, so the panel prefix is stripped before matching.
pub fn for_command(command: &str) -> Option<Icon> {
    let command = command.trim();
    let lower = command.to_ascii_lowercase();
    let base = strip_panel_prefix(&lower);
    match base.as_str() {
        "cm_aichat" | "cm_chatpane" | "cm_processwithagent" | "cm_mcptools" => Some(Icon::Robot),
        "cm_copy" | "cm_copysamepanel" | "cm_renmov" => Some(Icon::Copy),
        "cm_mkdir" => Some(Icon::FolderPlus),
        "cm_delete" | "cm_deletepermanent" => Some(Icon::Delete),
        "cm_edit" | "cm_editnewfile" | "cm_multirenamefiles" => Some(Icon::Edit),
        "cm_searchfor" | "cm_searchstandalone" => Some(Icon::Search),
        "cm_packfiles" | "cm_unpackfiles" => Some(Icon::Pack),
        "cm_filesync" | "cm_synchronizedirs" => Some(Icon::Sync),
        "cm_comparedirs" => Some(Icon::Compare),
        "cm_config" | "cm_buttonconfig" | "cm_fontconfig" => Some(Icon::Settings),
        "cm_rereadsource" => Some(Icon::Refresh),
        "cm_gotoparent" => Some(Icon::Up),
        "cm_gotoroot" => Some(Icon::Root),
        "cm_executedos" | "cm_terminal" => Some(Icon::Terminal),
        "cm_short" | "cm_long" | "cm_allfiles" | "cm_thumbs" | "cm_tree" => Some(Icon::List),
        "cm_quickview" => Some(Icon::Eye),
        "cm_directoryhotlist" | "cm_hotlist" => Some(Icon::Bookmark),
        "cm_getfilespace" => Some(Icon::HardDrive),
        "cm_newtab" => Some(Icon::Tab),
        "cm_view" | "cm_list" => Some(Icon::Eye),
        "cm_split" | "cm_splitfile" => Some(Icon::Scissors),
        "cm_createchecksums" | "cm_verifychecksums" => Some(Icon::Fingerprint),
        "cm_ftpconnect" | "cm_ftpnew" | "cm_ftpnewconnection" | "cm_ftpsessions" => {
            Some(Icon::Network)
        }
        "cm_ftpdisconnect" => Some(Icon::Network),
        "cm_secrets" => Some(Icon::Lock),
        "cm_saveconfig" => Some(Icon::Save),
        "cm_selectall" => Some(Icon::Check),
        "cm_directoryhistory" | "cm_historylist" | "cm_history" => Some(Icon::History),
        "cm_helpindex" | "cm_help" => Some(Icon::Help),
        "cm_exit" | "cm_quit" => Some(Icon::Exit),
        _ => None,
    }
}

/// Remove the `left`/`right`/`src` panel prefix from a lower-case command name,
/// so `cm_leftcopy` and `cm_srcshort` match the same arms as `cm_copy` and
/// `cm_short`.
fn strip_panel_prefix(lower: &str) -> String {
    for prefix in ["cm_left", "cm_right", "cm_src"] {
        if let Some(rest) = lower.strip_prefix(prefix) {
            return format!("cm_{rest}");
        }
    }
    lower.to_string()
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

    /// Every glyph must really exist in the bundled font: a missing one makes
    /// egui panic while painting, which would take the window down at the moment
    /// the button is first drawn.
    #[test]
    fn every_icon_glyph_exists_in_the_bundled_font() {
        use read_fonts::{FontRef, TableProvider};
        let font = FontRef::new(FONT_BYTES).expect("the bundled font parses");
        let cmap = font.cmap().expect("the font has a character map");
        for icon in Icon::all() {
            let code = icon.glyph() as u32;
            assert!(
                cmap.map_codepoint(code).is_some(),
                "{icon:?} (U+{code:04X}) is not in the bundled icon font"
            );
        }
        // Every command that claims an icon must point at a real one, and the
        // panel-prefixed spellings of a command must map to the same glyph.
        for command in [
            "cm_Copy",
            "cm_MkDir",
            "cm_Delete",
            "cm_Edit",
            "cm_SearchFor",
            "cm_PackFiles",
            "cm_UnpackFiles",
            "cm_FileSync",
            "cm_CompareDirs",
            "cm_Config",
            "cm_RereadSource",
            "cm_GoToParent",
            "cm_GoToRoot",
            "cm_ExecuteDOS",
            "cm_SrcShort",
            "cm_SrcLong",
            "cm_SrcThumbs",
            "cm_QuickView",
            "cm_DirectoryHotlist",
            "cm_GetFileSpace",
            "cm_NewTab",
            "cm_Split",
            "cm_CreateChecksums",
            "cm_FtpConnect",
            "cm_Secrets",
            "cm_SaveConfig",
            "cm_SelectAll",
            "cm_DirectoryHistory",
            "cm_HelpIndex",
            "cm_Exit",
        ] {
            let icon =
                for_command(command).unwrap_or_else(|| panic!("{command} should have an icon"));
            assert!(
                cmap.map_codepoint(icon.glyph() as u32).is_some(),
                "{command} maps to {icon:?}, which the font cannot draw"
            );
        }
        assert_eq!(for_command("cm_LeftCopy"), for_command("cm_Copy"));
        assert_eq!(for_command("cm_SrcShort"), for_command("cm_Short"));
        assert_eq!(for_command("cm_RightPackFiles"), Some(Icon::Pack));
        assert_eq!(for_command("cm_SrcBySize"), None);
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
    fn icon_colours_group_by_kind_of_action() {
        use rc_core::theme::Role;
        // Destructive commands warn, creating ones are green, moving ones blue.
        assert_eq!(
            for_command("cm_Delete").unwrap().colour_role(),
            Role::Warning
        );
        assert_eq!(
            for_command("cm_MkDir").unwrap().colour_role(),
            Role::Archive
        );
        assert_eq!(
            for_command("cm_PackFiles").unwrap().colour_role(),
            Role::Archive
        );
        assert_eq!(
            for_command("cm_Copy").unwrap().colour_role(),
            Role::Directory
        );
        assert_eq!(
            for_command("cm_FileSync").unwrap().colour_role(),
            Role::Directory
        );
        assert_eq!(
            for_command("cm_GoToParent").unwrap().colour_role(),
            Role::Accent
        );
        assert_eq!(
            for_command("cm_SearchFor").unwrap().colour_role(),
            Role::Symlink
        );
        assert_eq!(
            for_command("cm_Secrets").unwrap().colour_role(),
            Role::Symlink
        );
        assert_eq!(
            for_command("cm_Config").unwrap().colour_role(),
            Role::Hidden
        );

        // Every icon has a role, and the roles actually differ, so the bar is not
        // painted in one colour.
        let mut roles: Vec<Role> = Icon::all().iter().map(|i| i.colour_role()).collect();
        roles.sort();
        roles.dedup();
        assert!(roles.len() >= 4, "the icons use {roles:?}");
    }

    #[test]
    fn every_default_tool_bar_button_has_an_icon() {
        let bar = rc_core::config::ButtonBar::default();
        assert!(!bar.buttons.is_empty());
        for button in &bar.buttons {
            assert!(
                for_command(&button.command).is_some(),
                "the default button {command} has no icon",
                command = button.command
            );
        }
    }

    #[test]
    fn commands_map_to_icons_where_it_makes_sense() {
        assert_eq!(for_command("cm_Copy"), Some(Icon::Copy));
        assert_eq!(for_command("cm_FileSync"), Some(Icon::Sync));
        // Panel-prefixed spellings resolve to the same icon as the plain command.
        assert_eq!(for_command("cm_SrcShort"), Some(Icon::List));
        assert_eq!(for_command("cm_SrcBySize"), None);
    }
}
