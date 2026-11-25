//! Font discovery in the model (Phase 14.7).
//!
//! Scanning a machine's font directories costs a filesystem walk, so it happens **once**, and only
//! when somebody needs it: the options dialog asking for the list, or a start-up where a custom
//! face is already configured. An installation that never touches the font settings never pays for
//! it.

use super::Model;

impl Model {
    /// The fonts found on this machine, scanning on first use.
    pub fn font_choices(&mut self) -> Vec<rc_core::font::FontFile> {
        if !self.fonts_scanned {
            self.fonts = rc_core::font::discover();
            self.fonts_scanned = true;
        }
        self.fonts.clone()
    }

    /// Scan again (the user installed a font while the program was running).
    pub fn rescan_fonts(&mut self) -> usize {
        self.fonts = rc_core::font::discover();
        self.fonts_scanned = true;
        self.fonts.len()
    }

    /// The fonts that could suit one role: monospaced for `mono`, everything for the others.
    pub fn font_choices_for(&mut self, role: &str) -> Vec<rc_core::font::FontFile> {
        let all = self.font_choices();
        if role != "mono" {
            return all;
        }
        // A monospaced role wants monospaced faces first, but a proportional face is still allowed:
        // the user may know better than our file-name heuristic.
        let mut mono: Vec<rc_core::font::FontFile> =
            all.iter().filter(|font| font.mono).cloned().collect();
        let mut rest: Vec<rc_core::font::FontFile> =
            all.into_iter().filter(|font| !font.mono).collect();
        mono.append(&mut rest);
        mono
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn model() -> Model {
        let dir = std::env::temp_dir().join(format!("rc-fonts-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(&dir),
            Location::local(&dir),
        );
        model.config_dir_override = Some(dir);
        model
    }

    #[test]
    fn the_scan_happens_once_and_can_be_repeated() {
        let mut model = model();
        assert!(!model.fonts_scanned, "nothing is scanned at boot");
        let first = model.font_choices();
        assert!(model.fonts_scanned, "asking for the list scans");
        let second = model.font_choices();
        assert_eq!(first.len(), second.len(), "the cache is used");
        // A rescan is allowed and does not lose anything on a normal machine.
        let again = model.rescan_fonts();
        assert_eq!(again, first.len());
    }

    #[test]
    fn the_monospace_role_suggests_monospaced_faces_first() {
        let mut model = model();
        let all = model.font_choices_for("body");
        let mono = model.font_choices_for("mono");
        assert_eq!(all.len(), mono.len(), "no font is hidden");
        // Everything monospaced comes before everything else.
        let first_proportional = mono.iter().position(|font| !font.mono);
        if let Some(index) = first_proportional {
            assert!(
                mono[index..].iter().all(|font| !font.mono),
                "the order is mono first"
            );
        }
    }

    #[test]
    fn the_default_settings_do_not_need_any_font_file() {
        let settings = Settings::default();
        assert!(settings.fonts.is_builtin());
        assert!(settings.fonts.families().is_empty());
        let mut fonts = settings.fonts.clone();
        fonts.body_family = "DejaVu Sans".to_string();
        assert!(!fonts.is_builtin());
        assert_eq!(fonts.families(), vec!["DejaVu Sans"]);
    }

    #[test]
    fn the_sizes_are_clamped_to_something_usable() {
        let mut fonts = rc_core::config::Fonts {
            heading_size: 400.0,
            mono_size: 0.5,
            ..Default::default()
        };
        fonts.sanitise();
        assert_eq!(fonts.heading_size, 48.0);
        assert_eq!(fonts.mono_size, 8.0);
    }
}
