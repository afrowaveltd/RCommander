//! Automatic persistence of the settings file.
//!
//! The UI used to write `settings.toml` only when the user ran *Save Settings*,
//! so toggling a view option appeared to work but was lost on restart. This
//! helper writes the file whenever the settings actually changed, with a small
//! debounce so a burst of changes (dragging a slider, typing a name) results in
//! one write instead of dozens.
//!
//! The logic is time-injected so it can be tested without sleeping.

use rc_core::config::Settings;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Result of one [`SettingsWriter::maybe_save`] call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveOutcome {
    /// Settings were unchanged since the last successful write.
    Unchanged,
    /// A write happened.
    Saved,
    /// The change is newer than the debounce interval; it will be written later.
    Throttled,
    /// The write failed; the message is shown in the status bar.
    Failed(String),
}

impl SaveOutcome {
    pub fn is_saved(&self) -> bool {
        matches!(self, SaveOutcome::Saved)
    }
}

/// Writes `settings.toml` when the settings change.
pub struct SettingsWriter {
    path: PathBuf,
    /// Fingerprint of the settings as they were last written (or loaded).
    last_fingerprint: u64,
    /// When the last write happened, for the debounce.
    last_save: Option<Instant>,
    /// Minimum time between two writes.
    interval: Duration,
}

impl SettingsWriter {
    /// Create a writer for `path`, treating `current` as already persisted so a
    /// freshly started application does not rewrite an unchanged file.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        SettingsWriter {
            path: path.into(),
            last_fingerprint: 0,
            last_save: None,
            interval: Duration::from_millis(400),
        }
    }

    /// Like [`SettingsWriter::new`] but with the settings that were just loaded.
    pub fn with_current(path: impl Into<PathBuf>, current: &Settings) -> Self {
        let mut writer = SettingsWriter::new(path);
        writer.last_fingerprint = fingerprint(current);
        writer
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Override the debounce interval (tests use a zero interval).
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// Write the settings when they differ from the last write and the debounce
    /// interval has passed. `now` is injected so tests need no sleeping.
    pub fn maybe_save(&mut self, settings: &Settings, now: Instant) -> SaveOutcome {
        let current = fingerprint(settings);
        if current == self.last_fingerprint {
            return SaveOutcome::Unchanged;
        }
        if let Some(last) = self.last_save
            && now.duration_since(last) < self.interval
        {
            return SaveOutcome::Throttled;
        }
        match settings.save(&self.path) {
            Ok(()) => {
                self.last_fingerprint = current;
                self.last_save = Some(now);
                SaveOutcome::Saved
            }
            Err(error) => SaveOutcome::Failed(error.to_string()),
        }
    }

    /// Write immediately, ignoring the debounce (used by *Save Settings* and on exit).
    pub fn force_save(&mut self, settings: &Settings, now: Instant) -> SaveOutcome {
        self.last_save = None;
        self.maybe_save(settings, now)
    }
}

/// Create `path` with the current settings when it does not exist yet.
///
/// A first start therefore leaves a human-editable `settings.toml` behind instead
/// of keeping everything in memory until the first change. An existing file is
/// never overwritten, so hand-written comments survive.
pub fn ensure_settings_file(path: impl Into<PathBuf>, settings: &Settings) -> bool {
    let path = path.into();
    if path.exists() {
        return false;
    }
    settings.save(&path).is_ok()
}

/// Cheap fingerprint of a settings record: the serialised TOML hashed.
fn fingerprint(settings: &Settings) -> u64 {
    let text = settings.to_toml().unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn an_unchanged_record_is_not_written() {
        let dir = dir();
        let path = dir.path().join("settings.toml");
        let settings = Settings::default();
        let mut writer =
            SettingsWriter::with_current(path.clone(), &settings).with_interval(Duration::ZERO);
        assert_eq!(
            writer.maybe_save(&settings, Instant::now()),
            SaveOutcome::Unchanged
        );
        assert!(!path.exists(), "no file is created for unchanged settings");
    }

    #[test]
    fn a_change_is_written_once() {
        let dir = dir();
        let path = dir.path().join("settings.toml");
        let mut settings = Settings::default();
        let mut writer =
            SettingsWriter::with_current(path.clone(), &settings).with_interval(Duration::ZERO);
        settings.layout.show_status_bar = false;
        let now = Instant::now();
        assert_eq!(writer.maybe_save(&settings, now), SaveOutcome::Saved);
        assert!(path.exists());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("show_status_bar = false"), "{text}");
        // The same content is not written twice.
        assert_eq!(
            writer.maybe_save(&settings, now + Duration::from_secs(1)),
            SaveOutcome::Unchanged
        );
    }

    #[test]
    fn the_debounce_holds_a_second_change_back() {
        let dir = dir();
        let path = dir.path().join("settings.toml");
        let mut settings = Settings::default();
        let mut writer = SettingsWriter::with_current(path.clone(), &settings)
            .with_interval(Duration::from_millis(400));
        let start = Instant::now();
        settings.display.show_hidden = true;
        assert_eq!(writer.maybe_save(&settings, start), SaveOutcome::Saved);
        settings.display.show_hidden = false;
        assert_eq!(
            writer.maybe_save(&settings, start + Duration::from_millis(50)),
            SaveOutcome::Throttled
        );
        // After the interval the pending change goes through.
        assert_eq!(
            writer.maybe_save(&settings, start + Duration::from_millis(500)),
            SaveOutcome::Saved
        );
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("show_hidden = false"), "{text}");
    }

    #[test]
    fn force_save_ignores_the_debounce() {
        let dir = dir();
        let path = dir.path().join("settings.toml");
        let mut settings = Settings::default();
        let mut writer = SettingsWriter::with_current(path.clone(), &settings)
            .with_interval(Duration::from_secs(60));
        let start = Instant::now();
        settings.language = "cs".to_string();
        assert_eq!(writer.maybe_save(&settings, start), SaveOutcome::Saved);
        settings.language = "en".to_string();
        assert_eq!(
            writer.maybe_save(&settings, start + Duration::from_millis(10)),
            SaveOutcome::Throttled
        );
        assert_eq!(
            writer.force_save(&settings, start + Duration::from_millis(20)),
            SaveOutcome::Saved
        );
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("language = \"en\"")
        );
    }

    #[test]
    fn a_failure_is_reported_instead_of_panicking() {
        let bad = PathBuf::from("/proc/definitely/not/writable/settings.toml");
        let mut settings = Settings::default();
        let mut writer = SettingsWriter::with_current(bad, &settings).with_interval(Duration::ZERO);
        settings.layout.show_tabs = false;
        match writer.maybe_save(&settings, Instant::now()) {
            SaveOutcome::Failed(message) => assert!(!message.is_empty()),
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn the_fingerprint_changes_with_the_content() {
        let a = Settings::default();
        let b = Settings {
            language: "cs".to_string(),
            ..Settings::default()
        };
        assert_ne!(fingerprint(&a), fingerprint(&b));
        assert_eq!(fingerprint(&a), fingerprint(&Settings::default()));
    }
}

#[cfg(test)]
mod bootstrap_tests {
    use super::*;

    #[test]
    fn a_missing_settings_file_is_created_on_first_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let settings = Settings::default();
        assert!(!path.exists());
        let written = ensure_settings_file(path.clone(), &settings);
        assert!(
            written,
            "the defaults are written so the user has a file to edit"
        );
        assert!(path.exists());
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("language"));
        // A second call leaves an existing file alone.
        assert!(!ensure_settings_file(path, &settings));
    }

    #[test]
    fn an_existing_settings_file_is_never_overwritten_on_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        std::fs::write(&path, "# hand written\nlanguage = \"cs\"\n").unwrap();
        let settings = Settings::default();
        assert!(!ensure_settings_file(path.clone(), &settings));
        assert!(
            std::fs::read_to_string(&path)
                .unwrap()
                .contains("hand written")
        );
    }
}
