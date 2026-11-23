//! Directory hotlist (the `Ctrl+D` menu).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One hotlist entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotlistItem {
    pub title: String,
    pub path: String,
    /// Separator rows are rendered as a horizontal line and cannot be opened.
    #[serde(default)]
    pub separator: bool,
}

impl HotlistItem {
    pub fn new(title: impl Into<String>, path: impl Into<String>) -> Self {
        HotlistItem {
            title: title.into(),
            path: path.into(),
            separator: false,
        }
    }

    pub fn separator() -> Self {
        HotlistItem {
            title: "-".to_string(),
            path: String::new(),
            separator: true,
        }
    }

    pub fn target(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }
}

/// The whole hotlist, persisted as TOML in the config directory.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotlist {
    pub items: Vec<HotlistItem>,
}

impl Hotlist {
    pub fn new() -> Self {
        Hotlist::default()
    }

    /// Default hotlist offered on first start.
    pub fn with_defaults(home: Option<PathBuf>) -> Self {
        let mut h = Hotlist::new();
        if let Some(home) = home {
            h.items
                .push(HotlistItem::new("Home", home.to_string_lossy().to_string()));
            h.items.push(HotlistItem::new(
                "Documents",
                home.join("Documents").to_string_lossy().to_string(),
            ));
            h.items.push(HotlistItem::new(
                "Downloads",
                home.join("Downloads").to_string_lossy().to_string(),
            ));
            h.items.push(HotlistItem::separator());
            h.items.push(HotlistItem::new("Root", "/"));
        }
        h
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Append an entry, replacing one with the same title.
    pub fn add(&mut self, title: impl Into<String>, path: impl Into<String>) {
        let item = HotlistItem::new(title, path);
        if let Some(existing) = self.items.iter_mut().find(|i| i.title == item.title) {
            *existing = item;
        } else {
            self.items.push(item);
        }
    }

    pub fn insert(&mut self, index: usize, item: HotlistItem) {
        let index = index.min(self.items.len());
        self.items.insert(index, item);
    }

    pub fn remove(&mut self, index: usize) -> Option<HotlistItem> {
        if index < self.items.len() {
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    /// Move an entry one position up; returns true when it moved.
    pub fn move_up(&mut self, index: usize) -> bool {
        if index == 0 || index >= self.items.len() {
            return false;
        }
        self.items.swap(index, index - 1);
        true
    }

    /// Move an entry one position down; returns true when it moved.
    pub fn move_down(&mut self, index: usize) -> bool {
        if index + 1 >= self.items.len() {
            return false;
        }
        self.items.swap(index, index + 1);
        true
    }

    pub fn rename(&mut self, index: usize, title: impl Into<String>) -> bool {
        match self.items.get_mut(index) {
            Some(item) => {
                item.title = title.into();
                true
            }
            None => false,
        }
    }

    pub fn find_by_title(&self, title: &str) -> Option<usize> {
        self.items.iter().position(|i| i.title == title)
    }

    pub fn find_by_path(&self, path: &str) -> Option<usize> {
        self.items.iter().position(|i| i.path == path)
    }

    /// Menu entries, with separators flagged.
    pub fn menu_items(&self) -> Vec<&HotlistItem> {
        self.items.iter().collect()
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self).map_err(|e| e.to_string())
    }

    pub fn from_toml(text: &str) -> Result<Self, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    pub fn load(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::from_toml(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.to_toml().unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_replaces_same_title_and_appends_new() {
        let mut h = Hotlist::new();
        h.add("Work", "/work");
        h.add("Home", "/home");
        assert_eq!(h.len(), 2);
        h.add("Work", "/work2");
        assert_eq!(h.len(), 2);
        assert_eq!(h.items[0].path, "/work2");
    }

    #[test]
    fn insert_move_remove_and_rename() {
        let mut h = Hotlist::new();
        h.add("a", "/a");
        h.add("b", "/b");
        h.insert(1, HotlistItem::new("mid", "/mid"));
        let titles: Vec<&str> = h.items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["a", "mid", "b"]);
        assert!(h.move_up(2));
        let titles: Vec<&str> = h.items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["a", "b", "mid"]);
        assert!(h.move_down(0));
        assert!(!h.move_up(0));
        assert!(!h.move_down(2));
        assert!(h.rename(0, "bb"));
        assert_eq!(h.items[0].title, "bb");
        assert!(!h.rename(99, "x"));
        assert_eq!(h.remove(0).unwrap().path, "/b");
        assert!(h.remove(99).is_none());
    }

    #[test]
    fn lookups_by_title_and_path() {
        let mut h = Hotlist::new();
        h.add("Work", "/work");
        assert_eq!(h.find_by_title("Work"), Some(0));
        assert_eq!(h.find_by_path("/work"), Some(0));
        assert_eq!(h.find_by_title("Nope"), None);
    }

    #[test]
    fn separators_are_marked_and_have_no_target() {
        let s = HotlistItem::separator();
        assert!(s.separator);
        assert!(s.target().as_os_str().is_empty());
        assert!(!HotlistItem::new("x", "/x").separator);
    }

    #[test]
    fn toml_round_trip() {
        let mut h = Hotlist::new();
        h.add("Work", "/work");
        h.items.push(HotlistItem::separator());
        let text = h.to_toml().unwrap();
        assert!(text.contains("Work"));
        let back = Hotlist::from_toml(&text).unwrap();
        assert_eq!(back, h);
        assert!(Hotlist::from_toml("garbage = [").is_err());
    }

    #[test]
    fn save_and_load_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hotlist.toml");
        let mut h = Hotlist::new();
        h.add("Work", "/work");
        h.save(&path).unwrap();
        assert_eq!(Hotlist::load(&path), h);
        assert_eq!(
            Hotlist::load(&dir.path().join("missing.toml")),
            Hotlist::default()
        );
    }

    #[test]
    fn defaults_offer_home_directories() {
        let h = Hotlist::with_defaults(Some(PathBuf::from("/home/u")));
        assert!(h.find_by_title("Home").is_some());
        assert!(h.find_by_title("Documents").is_some());
        assert!(h.items.iter().any(|i| i.separator));
        assert_eq!(Hotlist::with_defaults(None).len(), 0);
    }
}
