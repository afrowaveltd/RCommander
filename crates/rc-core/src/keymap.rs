//! Keyboard shortcuts.
//!
//! A shortcut is a modifier set plus a normalised key token, for example
//! `ctrl+shift+f5` or `alt+f7`. Bindings map a shortcut to an internal command
//! name; users override them in the settings file and the UI translates key
//! events into these tokens.

use crate::error::{CoreError, CoreResult};
use std::collections::BTreeMap;
use std::fmt;

/// A normalised key combination.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Shortcut {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// Platform command/meta key (Windows key, Super, Command).
    pub meta: bool,
    /// Upper-case key token: `A`, `F3`, `INSERT`, `NUMPLUS`, `LEFT`, …
    pub key: String,
}

impl Shortcut {
    pub fn new(key: impl Into<String>) -> Self {
        Shortcut {
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
            key: key.into().to_ascii_uppercase(),
        }
    }

    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }
    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }
    pub fn alt(mut self) -> Self {
        self.alt = true;
        self
    }
    pub fn meta(mut self) -> Self {
        self.meta = true;
        self
    }

    /// Parse `ctrl+shift+f5` (modifiers case-insensitive, separators `+`).
    pub fn parse(text: &str) -> CoreResult<Self> {
        let mut sc = Shortcut::new("");
        let mut key: Option<String> = None;
        // Keypad tokens contain the separator themselves (`num+`), so they are
        // replaced by unambiguous words before the split.
        let lowered = text.to_ascii_lowercase();
        let canonical = lowered
            .replace("num+", "numplus")
            .replace("num-", "numminus")
            .replace("num*", "nummultiply")
            .replace("num/", "numdivide");
        for part in canonical.split('+') {
            let p = part.trim();
            if p.is_empty() {
                continue;
            }
            match p.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => sc.ctrl = true,
                "shift" => sc.shift = true,
                "alt" => sc.alt = true,
                "meta" | "cmd" | "super" | "win" => sc.meta = true,
                other => {
                    if key.is_some() {
                        return Err(CoreError::Invalid(format!(
                            "multiple keys in shortcut {text:?}"
                        )));
                    }
                    key = Some(normalise_key(other));
                }
            }
        }
        match key {
            Some(k) => {
                if !is_known_key(&k) {
                    return Err(CoreError::Invalid(format!(
                        "unknown key {k:?} in shortcut {text:?}"
                    )));
                }
                sc.key = k;
                Ok(sc)
            }
            None => Err(CoreError::Invalid(format!("no key in shortcut {text:?}"))),
        }
    }
}

/// Named keys the UI understands, in addition to single characters and F1-F24.
const NAMED_KEYS: &[&str] = &[
    "ESC",
    "DELETE",
    "INSERT",
    "ENTER",
    "SPACE",
    "BACKSPACE",
    "PGUP",
    "PGDN",
    "UP",
    "DOWN",
    "LEFT",
    "RIGHT",
    "HOME",
    "END",
    "TAB",
    "NUMPLUS",
    "NUMMINUS",
    "NUMMULTIPLY",
    "NUMDIVIDE",
    "NUMDECIMAL",
    "MINUS",
    "MENU",
    "PAUSE",
    "PRINTSCREEN",
    "CAPSLOCK",
];

fn is_known_key(token: &str) -> bool {
    if token.chars().count() == 1 {
        return true;
    }
    if let Some(num) = token.strip_prefix('F')
        && let Ok(n) = num.parse::<u8>()
    {
        return (1..=24).contains(&n);
    }
    NAMED_KEYS.contains(&token)
}

fn normalise_key(raw: &str) -> String {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "esc" | "escape" => "ESC".to_string(),
        "del" | "delete" => "DELETE".to_string(),
        "ins" | "insert" => "INSERT".to_string(),
        "return" | "enter" => "ENTER".to_string(),
        "space" | "spacebar" => "SPACE".to_string(),
        "back" | "backspace" => "BACKSPACE".to_string(),
        "pgup" | "prior" | "pageup" | "page up" => "PGUP".to_string(),
        "pgdn" | "next" | "pagedown" | "page down" => "PGDN".to_string(),
        "up" | "uparrow" | "up arrow" => "UP".to_string(),
        "down" | "downarrow" | "down arrow" => "DOWN".to_string(),
        "left" | "leftarrow" | "left arrow" => "LEFT".to_string(),
        "right" | "rightarrow" | "right arrow" => "RIGHT".to_string(),
        "home" => "HOME".to_string(),
        "end" => "END".to_string(),
        "tab" => "TAB".to_string(),
        "plus" | "num+" | "numplus" | "numadd" | "numpadadd" | "add" => "NUMPLUS".to_string(),
        "minus" | "num-" | "numminus" | "numpadsubtract" | "subtract" => "NUMMINUS".to_string(),
        "mul" | "num*" | "nummultiply" | "multiply" | "numpadmultiply" => "NUMMULTIPLY".to_string(),
        "div" | "num/" | "numdivide" | "divide" | "numpaddivide" => "NUMDIVIDE".to_string(),
        "numdecimal" | "decimal" => "NUMDECIMAL".to_string(),
        "minus_key" => "MINUS".to_string(),
        other => {
            // f1..f24 and single characters keep their canonical form.
            let mut chars = other.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => c.to_ascii_uppercase().to_string(),
                _ => other.to_ascii_uppercase(),
            }
        }
    }
}

impl fmt::Display for Shortcut {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<&str> = Vec::new();
        if self.ctrl {
            parts.push("ctrl");
        }
        if self.shift {
            parts.push("shift");
        }
        if self.alt {
            parts.push("alt");
        }
        if self.meta {
            parts.push("meta");
        }
        let key = self.key.to_ascii_lowercase();
        parts.push(&key);
        write!(f, "{}", parts.join("+"))
    }
}

/// One key → command assignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub shortcut: Shortcut,
    pub command: String,
}

/// The effective key map.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyMap {
    bindings: Vec<Binding>,
}

impl KeyMap {
    pub fn new() -> Self {
        KeyMap::default()
    }

    /// The classic commander layout, restated for this application.
    pub fn defaults() -> Self {
        let mut km = KeyMap::new();
        let mut bind = |sc: &str, cmd: &str| {
            if let Ok(shortcut) = Shortcut::parse(sc) {
                km.bindings.push(Binding {
                    shortcut,
                    command: cmd.to_string(),
                });
            }
        };
        // Function keys
        bind("f1", "cm_HelpIndex");
        bind("f2", "cm_RereadSource");
        bind("f3", "cm_List");
        bind("shift+f3", "cm_ListInternalOnly");
        bind("alt+f3", "cm_ListExternal");
        bind("f4", "cm_Edit");
        bind("f5", "cm_Copy");
        bind("shift+f5", "cm_CopySamePanel");
        bind("alt+f5", "cm_PackFiles");
        bind("f6", "cm_RenMov");
        bind("shift+f6", "cm_RenameOnly");
        bind("alt+f6", "cm_UnpackFiles");
        bind("alt+shift+f9", "cm_TestArchive");
        bind("f7", "cm_MkDir");
        bind("shift+f7", "cm_MkDirTarget");
        bind("alt+f7", "cm_SearchFor");
        bind("shift+alt+f7", "cm_SearchStandalone");
        bind("f8", "cm_Delete");
        bind("shift+f8", "cm_DeletePermanent");
        bind("f9", "cm_MenuSource");
        bind("f10", "cm_MenuLeft");
        bind("alt+f4", "cm_Exit");
        bind("alt+f8", "cm_CommandLineHistory");
        bind("alt+f10", "cm_CDtree");
        bind("alt+f11", "cm_SrcBreadcrumbar");
        bind("alt+f12", "cm_TrgBreadcrumbar");
        bind("shift+f1", "cm_CustomColumnViewMenu");
        bind("shift+f2", "cm_CompareDirs");
        bind("shift+f4", "cm_EditNew");
        bind("shift+f10", "cm_ContextMenu");
        bind("shift+esc", "cm_Minimize");
        // Panel display
        bind("ctrl+f1", "cm_SrcShort");
        bind("ctrl+f2", "cm_SrcLong");
        bind("ctrl+shift+f1", "cm_SrcThumbs");
        bind("ctrl+shift+f2", "cm_SrcComments");
        bind("ctrl+f3", "cm_SrcByName");
        bind("ctrl+f4", "cm_SrcByExt");
        bind("ctrl+f5", "cm_SrcByDateTime");
        bind("ctrl+f6", "cm_SrcBySize");
        bind("ctrl+f7", "cm_SrcUnsorted");
        bind("ctrl+f8", "cm_SrcTree");
        bind("ctrl+f9", "cm_PrintFile");
        bind("ctrl+f10", "cm_SrcAllFiles");
        bind("ctrl+f11", "cm_SrcExecs");
        bind("ctrl+f12", "cm_SrcUserDef");
        bind("ctrl+shift+f8", "cm_SeparateTree1");
        // Navigation
        bind("ctrl+pgup", "cm_GoToParent");
        bind("backspace", "cm_GoToParent");
        bind("ctrl+pgdn", "cm_OpenDirOrArchive");
        bind("alt+left", "cm_GoBack");
        bind("alt+right", "cm_GoForward");
        bind("alt+down", "cm_HistoryList");
        bind("ctrl+left", "cm_OpenDirInOtherPanel");
        bind("ctrl+right", "cm_OpenDirInOtherPanel");
        bind("tab", "cm_SwitchPanel");
        bind("ctrl+u", "cm_Exchange");
        bind("ctrl+shift+u", "cm_MatchSrc");
        bind("ctrl+r", "cm_RereadSource");
        bind("ctrl+t", "cm_NewTab");
        bind("ctrl+w", "cm_CloseTab");
        bind("ctrl+shift+tab", "cm_PrevTab");
        bind("ctrl+tab", "cm_NextTab");
        bind("ctrl+b", "cm_BranchView");
        bind("ctrl+q", "cm_SrcQuickView");
        bind("ctrl+d", "cm_DirectoryHotlist");
        bind("ctrl+l", "cm_GetFileSpace");
        bind("ctrl+m", "cm_MultiRename");
        bind("ctrl+z", "cm_EditComment");
        bind("alt+enter", "cm_VersionInfo");
        // Selection
        bind("insert", "cm_ToggleSelection");
        bind("space", "cm_ToggleSelectionAndDown");
        bind("numpadmultiply", "cm_ExchangeSelection");
        bind("nummultiply", "cm_ExchangeSelection");
        bind("ctrl+numpadadd", "cm_SelectAll");
        bind("ctrl+numadd", "cm_SelectAll");
        bind("ctrl+numpadsubtract", "cm_Clearall");
        bind("ctrl+numminus", "cm_Clearall");
        bind("numdivide", "cm_RestoreSelection");
        bind("alt+numpadadd", "cm_SelectSameExt");
        bind("alt+numminus", "cm_DeselectSameExt");
        // Misc
        bind("ctrl+shift+f5", "cm_CreateShortcut");
        bind("ctrl+n", "cm_FtpNewConnection");
        bind("ctrl+f", "cm_FtpConnect");
        bind("ctrl+shift+f", "cm_FtpDisconnect");
        bind("ctrl+shift+enter", "cm_OpenCommandLineInDir");
        bind("ctrl+enter", "cm_OpenInCommandLine");
        km
    }

    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The command bound to `shortcut`, if any.
    pub fn lookup(&self, shortcut: &Shortcut) -> Option<&str> {
        self.bindings
            .iter()
            .find(|b| b.shortcut == *shortcut)
            .map(|b| b.command.as_str())
    }

    /// Add or replace a binding.
    pub fn set(&mut self, shortcut: Shortcut, command: impl Into<String>) {
        let command = command.into();
        match self.bindings.iter_mut().find(|b| b.shortcut == shortcut) {
            Some(b) => b.command = command,
            None => self.bindings.push(Binding { shortcut, command }),
        }
    }

    /// Remove a binding; returns true when something was removed.
    pub fn remove(&mut self, shortcut: &Shortcut) -> bool {
        let before = self.bindings.len();
        self.bindings.retain(|b| b.shortcut != *shortcut);
        self.bindings.len() != before
    }

    /// The shortcut currently used for `command`, if any.
    pub fn shortcut_for(&self, command: &str) -> Option<&Shortcut> {
        self.bindings
            .iter()
            .find(|b| b.command == command)
            .map(|b| &b.shortcut)
    }

    /// Apply a settings override map (`"ctrl+f3" = "cm_SrcByName"`).
    pub fn apply_overrides(&mut self, overrides: &BTreeMap<String, String>) {
        for (sc, cmd) in overrides {
            if let Ok(shortcut) = Shortcut::parse(sc) {
                self.set(shortcut, cmd.clone());
            }
        }
    }

    /// Serialise the whole map to settings override syntax.
    pub fn to_overrides(&self) -> BTreeMap<String, String> {
        self.bindings
            .iter()
            .map(|b| (b.shortcut.to_string(), b.command.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display_round_trip() {
        let sc = Shortcut::parse("Ctrl+Shift+F5").unwrap();
        assert!(sc.ctrl && sc.shift && !sc.alt);
        assert_eq!(sc.key, "F5");
        assert_eq!(sc.to_string(), "ctrl+shift+f5");
        let back = Shortcut::parse(&sc.to_string()).unwrap();
        assert_eq!(back, sc);
    }

    #[test]
    fn key_aliases_are_normalised() {
        assert_eq!(Shortcut::parse("del").unwrap().key, "DELETE");
        assert_eq!(Shortcut::parse("esc").unwrap().key, "ESC");
        assert_eq!(Shortcut::parse("num+").unwrap().key, "NUMPLUS");
        assert_eq!(Shortcut::parse("page down").unwrap().key, "PGDN");
        assert_eq!(Shortcut::parse("ctrl+numadd").unwrap().key, "NUMPLUS");
        assert_eq!(Shortcut::parse("a").unwrap().key, "A");
        assert_eq!(Shortcut::parse("alt+left arrow").unwrap().key, "LEFT");
    }

    #[test]
    fn invalid_shortcuts_are_rejected() {
        assert!(Shortcut::parse("").is_err());
        assert!(Shortcut::parse("ctrl").is_err());
        assert!(Shortcut::parse("ctrl+a+b").is_err());
    }

    #[test]
    fn default_map_binds_the_classic_keys() {
        let km = KeyMap::defaults();
        assert!(km.len() > 50, "default bindings: {}", km.len());
        assert_eq!(km.lookup(&Shortcut::parse("f3").unwrap()), Some("cm_List"));
        assert_eq!(km.lookup(&Shortcut::parse("f5").unwrap()), Some("cm_Copy"));
        assert_eq!(
            km.lookup(&Shortcut::parse("ctrl+f7").unwrap()),
            Some("cm_SrcUnsorted")
        );
        assert_eq!(
            km.lookup(&Shortcut::parse("alt+f7").unwrap()),
            Some("cm_SearchFor")
        );
        assert_eq!(km.lookup(&Shortcut::parse("f12").unwrap()), None);
    }

    #[test]
    fn set_remove_and_lookup_by_command() {
        let mut km = KeyMap::defaults();
        let sc = Shortcut::parse("ctrl+shift+p").unwrap();
        km.set(sc.clone(), "cm_TestCommand");
        assert_eq!(km.lookup(&sc), Some("cm_TestCommand"));
        assert_eq!(km.shortcut_for("cm_TestCommand"), Some(&sc));
        assert!(km.remove(&sc));
        assert!(!km.remove(&sc));
        assert_eq!(km.lookup(&sc), None);
    }

    #[test]
    fn overrides_apply_from_settings() {
        let mut km = KeyMap::defaults();
        let mut map = BTreeMap::new();
        map.insert("ctrl+f3".to_string(), "cm_SrcUnsorted".to_string());
        map.insert("ctrl+F7".to_string(), "cm_SrcByName".to_string());
        km.apply_overrides(&map);
        assert_eq!(
            km.lookup(&Shortcut::parse("ctrl+f3").unwrap()),
            Some("cm_SrcUnsorted")
        );
        assert_eq!(
            km.lookup(&Shortcut::parse("ctrl+f7").unwrap()),
            Some("cm_SrcByName")
        );
        let round = km.to_overrides();
        assert_eq!(
            round.get("ctrl+f3").map(|s| s.as_str()),
            Some("cm_SrcUnsorted")
        );
    }

    #[test]
    fn overrides_of_unknown_syntax_are_skipped() {
        let mut km = KeyMap::defaults();
        let mut map = BTreeMap::new();
        map.insert("not a shortcut".to_string(), "cm_X".to_string());
        let before = km.len();
        km.apply_overrides(&map);
        assert_eq!(km.len(), before);
    }
}
