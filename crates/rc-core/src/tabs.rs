//! Folder tabs of one panel.

use crate::location::Location;
use crate::sorting::SortSpec;

/// A single folder tab.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelTab {
    pub location: Location,
    pub sort: SortSpec,
    /// File mask shown in this tab (`*.*` by default).
    pub mask: String,
    /// Locked tabs keep their directory when the other panel changes.
    pub locked: bool,
    /// Optional custom title.
    pub title: Option<String>,
}

impl PanelTab {
    pub fn new(location: Location) -> Self {
        PanelTab {
            location,
            sort: SortSpec::default(),
            mask: "*.*".to_string(),
            locked: false,
            title: None,
        }
    }

    /// Tab label: the custom title, else the last path component.
    pub fn label(&self) -> String {
        if let Some(t) = &self.title
            && !t.is_empty()
        {
            return t.clone();
        }
        match self.location.file_name() {
            Some(n) if !n.is_empty() => n,
            _ => self.location.display(),
        }
    }
}

/// The tab strip of one panel.
#[derive(Clone, Debug, PartialEq)]
pub struct TabBar {
    tabs: Vec<PanelTab>,
    active: usize,
}

impl TabBar {
    /// Create a tab bar with a single tab at `location`.
    pub fn new(location: Location) -> Self {
        TabBar {
            tabs: vec![PanelTab::new(location)],
            active: 0,
        }
    }

    pub fn tabs(&self) -> &[PanelTab] {
        &self.tabs
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn active_index(&self) -> usize {
        self.active
    }

    pub fn active(&self) -> &PanelTab {
        &self.tabs[self.active.min(self.tabs.len() - 1)]
    }

    pub fn active_mut(&mut self) -> &mut PanelTab {
        let i = self.active.min(self.tabs.len() - 1);
        &mut self.tabs[i]
    }

    /// Activate the tab at `index`; returns false for an invalid index.
    pub fn activate(&mut self, index: usize) -> bool {
        if index < self.tabs.len() {
            self.active = index;
            true
        } else {
            false
        }
    }

    /// Activate the next tab (wrapping).
    pub fn next(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    /// Activate the previous tab (wrapping).
    pub fn previous(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
        }
    }

    /// Duplicate the active tab and activate the copy.
    pub fn duplicate_active(&mut self) -> usize {
        let mut copy = self.active().clone();
        copy.locked = false;
        self.tabs.insert(self.active + 1, copy);
        self.active += 1;
        self.active
    }

    /// Append a new tab; `after_active` inserts it next to the active tab.
    pub fn add(&mut self, tab: PanelTab, after_active: bool) -> usize {
        if after_active {
            let idx = self.active + 1;
            self.tabs.insert(idx, tab);
            self.active = idx;
            idx
        } else {
            self.tabs.push(tab);
            self.tabs.len() - 1
        }
    }

    /// Close the tab at `index`. The last remaining tab cannot be closed.
    pub fn close(&mut self, index: usize) -> bool {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return false;
        }
        self.tabs.remove(index);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if index < self.active {
            self.active -= 1;
        }
        true
    }

    /// Close every tab except `keep`.
    pub fn close_others(&mut self, keep: usize) -> usize {
        if keep >= self.tabs.len() {
            return 0;
        }
        let kept = self.tabs[keep].clone();
        let removed = self.tabs.len() - 1;
        self.tabs = vec![kept];
        self.active = 0;
        removed
    }

    /// Labels of all tabs, for rendering.
    pub fn labels(&self) -> Vec<String> {
        self.tabs.iter().map(|t| t.label()).collect()
    }

    /// Set the directory of the active tab.
    pub fn set_active_location(&mut self, location: Location) {
        self.active_mut().location = location;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar() -> TabBar {
        TabBar::new(Location::local("/a"))
    }

    #[test]
    fn starts_with_one_active_tab() {
        let b = bar();
        assert_eq!(b.len(), 1);
        assert_eq!(b.active_index(), 0);
        assert_eq!(b.active().label(), "a");
        assert_eq!(b.labels(), ["a"]);
    }

    #[test]
    fn add_after_active_and_at_end() {
        let mut b = bar();
        b.add(PanelTab::new(Location::local("/b")), true);
        assert_eq!(b.labels(), ["a", "b"]);
        assert_eq!(b.active_index(), 1);
        b.add(PanelTab::new(Location::local("/c")), false);
        assert_eq!(b.labels(), ["a", "b", "c"]);
        assert_eq!(b.active_index(), 1);
    }

    #[test]
    fn activation_and_cyclic_navigation() {
        let mut b = bar();
        b.add(PanelTab::new(Location::local("/b")), false);
        assert!(b.activate(0));
        b.previous();
        assert_eq!(b.active_index(), 1);
        b.next();
        assert_eq!(b.active_index(), 0);
        assert!(!b.activate(9));
    }

    #[test]
    fn duplicate_creates_an_unlocked_copy_next_to_the_original() {
        let mut b = bar();
        b.active_mut().locked = true;
        let idx = b.duplicate_active();
        assert_eq!(idx, 1);
        assert_eq!(b.len(), 2);
        assert!(!b.active().locked);
        assert_eq!(b.active().location, Location::local("/a"));
    }

    #[test]
    fn last_tab_cannot_be_closed() {
        let mut b = bar();
        assert!(!b.close(0));
        b.add(PanelTab::new(Location::local("/b")), false);
        b.activate(1);
        assert!(b.close(1));
        assert_eq!(b.len(), 1);
        assert_eq!(b.active_index(), 0);
        assert!(!b.close(5));
    }

    #[test]
    fn closing_a_tab_before_the_active_one_keeps_the_selection() {
        let mut b = bar();
        b.add(PanelTab::new(Location::local("/b")), false);
        b.add(PanelTab::new(Location::local("/c")), false);
        b.activate(2);
        assert!(b.close(0));
        assert_eq!(b.labels(), ["b", "c"]);
        assert_eq!(b.active_index(), 1);
    }

    #[test]
    fn close_others_keeps_one_tab() {
        let mut b = bar();
        for p in ["/b", "/c", "/d"] {
            b.add(PanelTab::new(Location::local(p)), false);
        }
        assert_eq!(b.close_others(2), 3);
        assert_eq!(b.len(), 1);
        assert_eq!(b.active_index(), 0);
        assert_eq!(b.active().label(), "c");
        assert_eq!(b.close_others(9), 0);
    }

    #[test]
    fn labels_prefer_custom_titles_and_fall_back_to_full_path() {
        let mut t = PanelTab::new(Location::local("/a/b"));
        assert_eq!(t.label(), "b");
        t.title = Some("Work".to_string());
        assert_eq!(t.label(), "Work");
        t.title = Some(String::new());
        assert_eq!(t.label(), "b");
        let root = PanelTab::new(Location::local("/"));
        assert_eq!(root.label(), "/");
    }

    #[test]
    fn active_location_updates_only_the_active_tab() {
        let mut b = bar();
        b.add(PanelTab::new(Location::local("/b")), false);
        b.activate(1);
        b.set_active_location(Location::local("/z"));
        assert_eq!(b.tabs()[0].location, Location::local("/a"));
        assert_eq!(b.tabs()[1].location, Location::local("/z"));
    }
}
