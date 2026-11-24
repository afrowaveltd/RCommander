//! Colour themes in the model (Phase 14).
//!
//! `rc_core::theme` owns the file format; this layer keeps the loaded theme set,
//! knows where the files live and applies a theme name from the settings.

use super::{CommandOutcome, Model};
use rc_core::theme::{Resolved, Theme, ThemeSet};

impl Model {
    /// Directory the theme files are read from and written to.
    pub fn themes_dir(&self) -> std::path::PathBuf {
        self.themes_dir_override
            .clone()
            .unwrap_or_else(crate::paths::themes_dir)
    }

    /// Reload the theme directory; built-ins are always available.
    pub fn load_themes(&mut self) {
        self.theme_set = ThemeSet::load_dir(&self.themes_dir());
    }

    /// Names of the available themes, sorted.
    pub fn theme_names(&self) -> Vec<String> {
        self.theme_set.names()
    }

    /// The name to show for a theme key: the `[meta] name` of the file, else the
    /// key itself. The settings always store the key, so renaming a theme in its
    /// file cannot lose the active selection.
    pub fn theme_display_name(&self, key: &str) -> String {
        self.theme_set
            .get(key)
            .map(|theme| theme.meta.name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| key.to_string())
    }

    /// Themes that could not be parsed, as `path: message`.
    pub fn theme_problems(&self) -> &[String] {
        &self.theme_set.problems
    }

    /// The resolved palette of the active theme.
    pub fn resolved_theme(&self) -> Resolved {
        self.resolved_theme_named(&self.settings.colors.theme)
    }

    /// The resolved palette of a theme by name.
    pub fn resolved_theme_named(&self, name: &str) -> Resolved {
        self.theme_set.resolve(name)
    }

    /// A theme file from the set.
    pub fn theme(&self, name: &str) -> Option<&Theme> {
        self.theme_set.get(name)
    }

    /// Switch to `name` when it exists; returns false for an unknown name.
    pub fn set_theme(&mut self, name: &str) -> bool {
        if self.theme_set.get(name).is_none() {
            return false;
        }
        let key = self
            .theme_names()
            .into_iter()
            .find(|candidate| candidate.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| name.to_ascii_lowercase());
        self.settings.colors.theme = key;
        self.mark_settings_dirty();
        true
    }

    /// Write a theme into the theme directory and make it the active one.
    pub fn save_theme(&mut self, theme: Theme) -> Result<String, String> {
        theme.validate()?;
        let name = if theme.meta.name.trim().is_empty() {
            "theme".to_string()
        } else {
            theme.meta.name.trim().to_string()
        };
        let dir = self.themes_dir();
        self.theme_set
            .store(&dir, &name, &theme)
            .map_err(|e| e.to_string())?;
        let key = name.to_ascii_lowercase();
        self.settings.colors.theme = key.clone();
        self.mark_settings_dirty();
        // Re-read so inheritance and the file name stay in sync with the disk.
        self.load_themes();
        Ok(key)
    }

    /// Delete a theme file; the built-ins are refused.
    pub fn delete_theme(&mut self, name: &str) -> Result<bool, String> {
        let dir = self.themes_dir();
        let removed = self.theme_set.remove(&dir, name)?;
        if self.settings.colors.theme.eq_ignore_ascii_case(name) {
            self.settings.colors.theme = "light".to_string();
            self.mark_settings_dirty();
        }
        self.load_themes();
        Ok(removed)
    }

    /// Switch between light and dark, following the luminance of the active theme.
    pub fn toggle_theme(&mut self) -> CommandOutcome {
        // Pick the built-in that is the opposite of what is on screen, so a dark
        // custom theme toggles to light and vice versa.
        let next = if self.resolved_theme().is_dark() {
            "light"
        } else {
            "dark"
        };
        if self.theme_set.get(next).is_some() {
            self.settings.colors.theme = next.to_string();
            self.mark_settings_dirty();
        }
        CommandOutcome::Ok
    }

    /// Move `step` places through the theme list, wrapping around.
    pub fn cycle_theme(&mut self, step: i32) -> CommandOutcome {
        let names = self.theme_names();
        if names.is_empty() {
            return CommandOutcome::Ok;
        }
        let current = self
            .theme_names()
            .iter()
            .position(|name| name.eq_ignore_ascii_case(&self.settings.colors.theme))
            .unwrap_or(0);
        let len = names.len() as i32;
        let next = ((current as i32 + step).rem_euclid(len)) as usize;
        self.settings.colors.theme = names[next].clone();
        self.mark_settings_dirty();
        let message = self.tf("status.theme", &[("name", &names[next])]);
        self.report(message);
        CommandOutcome::Ok
    }

    /// The theme that is currently active, ready to be edited: a complete copy
    /// with every role spelled out.
    pub fn active_theme_for_editing(&self) -> Theme {
        self.active_theme_for_editing_named(&self.settings.colors.theme.clone())
    }

    /// Any theme, resolved and complete, ready to be edited.
    pub fn active_theme_for_editing_named(&self, wanted: &str) -> Theme {
        let resolved = self.resolved_theme_named(wanted);
        let name = wanted.to_string();
        let base = self
            .theme_set
            .get(&name)
            .map(|theme| theme.meta.base.clone())
            .filter(|base| !base.trim().is_empty())
            .unwrap_or_else(|| {
                if resolved.is_dark() {
                    "dark".into()
                } else {
                    "light".into()
                }
            });
        let mut theme = Theme::complete(name.clone(), base, &resolved);
        theme.file_types = self
            .theme_set
            .get(&name)
            .map(|theme| theme.file_types.clone())
            .unwrap_or_default();
        theme
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;
    use rc_core::theme::Role;

    fn model_with_themes(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local("/tmp"),
            Location::local("/tmp"),
        );
        model.themes_dir_override = Some(dir.to_path_buf());
        model.load_themes();
        model
    }

    #[test]
    fn the_shipped_themes_parse_and_the_builtins_are_always_there() {
        let dir = std::env::temp_dir().join(format!("rc-themes-ship-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // Copy the themes that ship with the repository.
        let shipped = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("themes");
        for entry in std::fs::read_dir(&shipped).unwrap().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                let target = dir.join(path.file_name().unwrap());
                std::fs::copy(&path, target).unwrap();
            }
        }
        let model = model_with_themes(&dir);
        let names = model.theme_names();
        for expected in [
            "light",
            "dark",
            "classic light",
            "classic dark",
            "high contrast",
        ] {
            assert!(
                names.iter().any(|n| n.eq_ignore_ascii_case(expected)),
                "{expected} is missing from {names:?}"
            );
        }
        assert!(
            model.theme_problems().is_empty(),
            "shipped themes must parse: {:?}",
            model.theme_problems()
        );
        // Every shipped theme resolves to a complete palette.
        for name in names {
            let resolved = model.resolved_theme_named(&name);
            assert!(resolved.is_complete(), "{name} is incomplete");
        }
        // Every shipped theme has to be readable: the six pairs that decide whether
        // the window can be used at all stay above a 3:1 ratio, and the text pairs
        // above 4.5:1 where the palette allows it.
        for name in model.theme_names() {
            let resolved = model.resolved_theme_named(&name);
            for (front, back) in [
                (Role::Foreground, Role::Background),
                (Role::CursorForeground, Role::CursorBackground),
                (Role::SelectedForeground, Role::SelectedBackground),
                (Role::Foreground, Role::RowAlternate),
                (Role::Hidden, Role::Background),
                (Role::Accent, Role::HeaderBackground),
            ] {
                let ratio = resolved.get(front).contrast(resolved.get(back));
                assert!(
                    ratio >= 3.0,
                    "{name}: {front:?} on {back:?} is only {ratio:.2}:1"
                );
            }
        }
        let high = model.resolved_theme_named("high contrast");
        assert_eq!(high.get(Role::Background).r, 0x00);
        assert!(
            high.get(Role::Foreground)
                .contrast(high.get(Role::Background))
                > 15.0
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_theme_dropped_into_the_directory_appears_without_recompiling() {
        let dir = std::env::temp_dir().join(format!("rc-themes-drop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut model = model_with_themes(&dir);
        assert!(!model.theme_names().iter().any(|n| n == "ocean"));
        std::fs::write(
            dir.join("ocean.toml"),
            "[meta]\nname = 'Ocean'\nbase = 'dark'\n[colors]\nbackground = '#001a2b'\naccent = '#00a0ff'\n",
        )
        .unwrap();
        model.load_themes();
        assert!(
            model
                .theme_names()
                .iter()
                .any(|n| n.eq_ignore_ascii_case("ocean")),
            "{:?}",
            model.theme_names()
        );
        let resolved = model.resolved_theme_named("Ocean");
        assert_eq!(resolved.get(Role::Background).b, 0x2b);
        assert_eq!(resolved.get(Role::Accent).to_hex(), "#00a0ff");
        assert!(resolved.is_dark());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saving_a_theme_writes_a_file_and_activates_it() {
        let dir = std::env::temp_dir().join(format!("rc-themes-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut model = model_with_themes(&dir);
        let mut theme = model.active_theme_for_editing();
        theme.meta.name = "My Theme".to_string();
        theme
            .colors
            .insert("accent".to_string(), "#ff00ff".to_string());
        let name = model.save_theme(theme).unwrap();
        assert_eq!(name, "my theme", "the key is the lower-case name");
        assert_eq!(model.settings.colors.theme, "my theme");
        assert_eq!(
            model.theme_display_name("my theme"),
            "My Theme",
            "the file's own name is what the user sees"
        );
        assert!(dir.join("my-theme.toml").is_file());
        assert_eq!(
            model.resolved_theme().get(Role::Accent).to_hex(),
            "#ff00ff",
            "the saved theme is the active one"
        );
        assert!(model.settings_dirty, "the change has to be persisted");
        // Deleting it falls back to the light theme.
        assert!(model.delete_theme("My Theme").unwrap());
        assert!(!dir.join("my-theme.toml").exists());
        assert_eq!(model.settings.colors.theme, "light");
        assert!(
            model.delete_theme("light").is_err(),
            "built-ins are protected"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_active_theme_can_be_edited_as_a_complete_copy() {
        let dir = std::env::temp_dir().join(format!("rc-themes-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut model = model_with_themes(&dir);
        model.settings.colors.theme = "dark".to_string();
        let theme = model.active_theme_for_editing();
        assert_eq!(theme.meta.name, "dark");
        assert_eq!(
            theme.colors.len(),
            Role::ALL.len(),
            "every role is editable"
        );
        assert!(theme.validate().is_ok());
        assert_eq!(
            theme.colors.get("background").map(|s| s.as_str()),
            Some("#1e1e1e")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn switching_to_a_theme_that_does_not_exist_changes_nothing() {
        let dir = std::env::temp_dir().join(format!("rc-themes-set-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut model = model_with_themes(&dir);
        let before = model.settings.colors.theme.clone();
        assert!(!model.set_theme("nope"));
        assert_eq!(model.settings.colors.theme, before);
        assert!(model.set_theme("dark"));
        assert_eq!(model.settings.colors.theme, "dark");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
