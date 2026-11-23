//! The background transfer manager window.
//!
//! Every copy, move or delete started in the background appears here with its
//! own progress row and its own cancel button, which is what makes a queue of
//! long transfers manageable.

use crate::model::Model;
use crate::theme::Palette;
use egui::{Context, RichText};

/// One row of the manager, snapshotted so the model is not borrowed while the
/// window draws.
#[derive(Clone, Debug, PartialEq)]
pub struct TransferRow {
    pub id: u64,
    pub kind: String,
    pub fraction: f32,
    pub current_file: String,
    pub files_done: u64,
}

/// Snapshot the running jobs for display.
pub fn rows(model: &Model) -> Vec<TransferRow> {
    model
        .transfers
        .iter()
        .map(|handle| TransferRow {
            id: handle.id,
            kind: handle.kind_title().to_string(),
            fraction: handle.progress.fraction(),
            current_file: handle.progress.current_file.clone(),
            files_done: handle.progress.files_done,
        })
        .collect()
}

/// Draw the transfer manager; returns true when the window should close.
pub fn show(ctx: &Context, model: &mut Model, palette: &Palette) -> bool {
    let snapshot = rows(model);
    let mut open = true;
    let mut cancel: Option<u64> = None;
    let mut cancel_all = false;
    egui::Window::new(model.t("dialog.copy.background"))
        .open(&mut open)
        .resizable(true)
        .default_size([620.0, 320.0])
        .show(ctx, |ui| {
            if snapshot.is_empty() {
                ui.label(RichText::new(model.t("panel.empty")).color(palette.hidden));
                ui.label(
                    RichText::new(model.t("status.ready"))
                        .small()
                        .color(palette.hidden),
                );
                return;
            }
            ui.horizontal(|ui| {
                ui.label(model.t("dialog.copy.background"));
                if ui.small_button(model.t("dialog.cancel")).clicked() {
                    cancel_all = true;
                }
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for row in &snapshot {
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::ProgressBar::new(row.fraction)
                                    .desired_width(180.0)
                                    .text(format!("{}%", (row.fraction * 100.0).round() as u32)),
                            );
                            ui.label(RichText::new(format!("#{} {}", row.id, row.kind)).strong());
                            ui.label(
                                RichText::new(&row.current_file)
                                    .small()
                                    .color(palette.hidden),
                            );
                            ui.label(RichText::new(format!("{} file(s)", row.files_done)).small());
                            if ui.small_button("×").clicked() {
                                cancel = Some(row.id);
                            }
                        });
                    }
                });
        });
    if cancel_all {
        model.cancel_transfers();
    }
    if let Some(id) = cancel {
        model.cancel_transfer(id);
    }
    !open
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;
    use crate::model::dialogs::TransferDialogState;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn model_with_tree() -> (tempfile::TempDir, tempfile::TempDir, Model) {
        let left = tempfile::tempdir().unwrap();
        let right = tempfile::tempdir().unwrap();
        std::fs::write(left.path().join("a.bin"), vec![7u8; 300_000]).unwrap();
        let model = Model::for_test(
            Settings::default(),
            Location::local(left.path()),
            Location::local(right.path()),
        );
        (left, right, model)
    }

    #[test]
    fn rows_are_empty_without_jobs() {
        let (_l, _r, model) = model_with_tree();
        assert!(rows(&model).is_empty());
    }

    #[test]
    fn a_started_job_shows_up_as_a_row() {
        let (_l, right, mut model) = model_with_tree();
        model.left.selection.set(["a.bin"]);
        let state = TransferDialogState {
            ask_overwrite: false,
            ..TransferDialogState::new(
                false,
                model.selected_paths(),
                right.path().to_string_lossy().to_string(),
            )
        };
        model.start_transfer(&state).expect("job");
        let snapshot = rows(&model);
        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].id, 1);
        assert!(!snapshot[0].kind.is_empty());
        assert!((0.0..=1.0).contains(&snapshot[0].fraction));
        // Let it finish so the temporary directories can be dropped.
        let mut guard = 0;
        while model.poll_transfers() {
            guard += 1;
            assert!(guard < 3000);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(rows(&model).is_empty(), "finished jobs leave the manager");
    }

    #[test]
    fn a_single_job_can_be_cancelled_by_id() {
        let (_l, right, mut model) = model_with_tree();
        model.left.selection.set(["a.bin"]);
        let state = TransferDialogState {
            ask_overwrite: false,
            ..TransferDialogState::new(
                false,
                model.selected_paths(),
                right.path().to_string_lossy().to_string(),
            )
        };
        let id = model.start_transfer(&state).expect("job");
        model.cancel_transfer(id);
        let mut guard = 0;
        while model.poll_transfers() {
            guard += 1;
            assert!(guard < 3000);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(!model.is_busy());
    }
}
