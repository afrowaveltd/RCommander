//! The F3 previewer window.
//!
//! It renders whatever `rc-viewer` decoded: text with the detected encoding,
//! a hexdump for unknown binaries, or a decoded image.

use crate::model::{Model, ViewerMode};
use crate::theme::Palette;
use egui::{Context, RichText, TextStyle};
use rc_viewer::{LoadedViewer, ViewerContent, ViewerOptions};

/// Loaded content plus the texture of a decoded image.
#[derive(Default)]
pub struct ViewerUi {
    pub loaded: Option<LoadedViewer>,
    pub image_texture: Option<egui::TextureHandle>,
    pub error: Option<String>,
    pub options: ViewerOptions,
}

impl ViewerUi {
    /// Load `path` into the viewer, honouring the options.
    pub fn load(&mut self, path: &std::path::Path) -> bool {
        self.image_texture = None;
        match rc_viewer::load(path, &self.options) {
            Ok(loaded) => {
                self.error = None;
                self.loaded = Some(loaded);
                true
            }
            Err(e) => {
                self.error = Some(e.to_string());
                self.loaded = None;
                false
            }
        }
    }

    /// Build a GPU texture for the decoded image, once.
    pub fn ensure_texture(&mut self, ctx: &Context) {
        if self.image_texture.is_some() {
            return;
        }
        let Some(loaded) = &self.loaded else { return };
        if let ViewerContent::Image(image) = &loaded.content {
            let size = [image.width as usize, image.height as usize];
            if size[0] == 0 || size[1] == 0 || image.rgba.len() < size[0] * size[1] * 4 {
                self.error = Some("invalid image buffer".to_string());
                return;
            }
            let color = egui::ColorImage::from_rgba_unmultiplied(size, &image.rgba);
            self.image_texture =
                Some(ctx.load_texture("rcommander-viewer", color, egui::TextureOptions::LINEAR));
        }
    }

    /// Status line text for the viewer.
    pub fn status(&self) -> String {
        match &self.loaded {
            Some(loaded) => rc_viewer::describe(loaded),
            None => self.error.clone().unwrap_or_default(),
        }
    }
}

/// Draw the viewer window; returns true when the user closed it.
pub fn show(ctx: &Context, model: &mut Model, ui_state: &mut ViewerUi, palette: &Palette) -> bool {
    let Some(mut state) = model.viewer.clone() else {
        return true;
    };
    let mut open = true;
    let title = format!("{} — {}", model.t("viewer.text"), state.title);
    egui::Window::new(title)
        .open(&mut open)
        .resizable(true)
        .default_size([760.0, 560.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (label, mode) in [
                    ("A", ViewerMode::Auto),
                    ("T", ViewerMode::Text),
                    ("H", ViewerMode::Hex),
                    ("I", ViewerMode::Image),
                ] {
                    if ui.selectable_label(state.mode == mode, label).clicked() {
                        state.mode = mode;
                        let path = std::path::PathBuf::from(&state.path);
                        let _ = ui_state.load(&path);
                    }
                }
                ui.separator();
                if ui.button(model.t("cmd.refresh")).clicked() {
                    let path = std::path::PathBuf::from(&state.path);
                    let _ = ui_state.load(&path);
                }
                ui.label(
                    RichText::new(ui_state.status())
                        .small()
                        .color(palette.hidden),
                );
            });
            ui.separator();
            ui_state.ensure_texture(ctx);
            let mode = effective_mode(state.mode, ui_state);
            match (&ui_state.loaded, mode) {
                (Some(_loaded), ViewerMode::Image) => show_image(ui, &mut state, ui_state),
                (Some(loaded), ViewerMode::Hex) => show_hex(ui, &mut state, loaded, palette),
                (Some(loaded), _) => show_text(ui, &mut state, loaded, palette),
                (None, _) => {
                    ui.label(
                        ui_state
                            .error
                            .clone()
                            .unwrap_or_else(|| model.t("panel.empty")),
                    );
                }
            }
        });
    model.viewer = if open { Some(state) } else { None };
    !open
}

fn effective_mode(mode: ViewerMode, ui_state: &ViewerUi) -> ViewerMode {
    if mode != ViewerMode::Auto {
        return mode;
    }
    match ui_state.loaded.as_ref().map(|l| &l.content) {
        Some(ViewerContent::Image(_)) => ViewerMode::Image,
        Some(ViewerContent::Hex(_)) => ViewerMode::Hex,
        Some(ViewerContent::Text(_)) => ViewerMode::Text,
        _ => ViewerMode::Hex,
    }
}

fn show_text(
    ui: &mut egui::Ui,
    state: &mut crate::model::ViewerState,
    loaded: &LoadedViewer,
    palette: &Palette,
) {
    let ViewerContent::Text(text) = &loaded.content else {
        ui.label(rc_viewer::describe(loaded));
        return;
    };
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{}: {}", "encoding", text.encoding))
                .small()
                .color(palette.hidden),
        );
        if text.is_rtl {
            ui.label(RichText::new("RTL").small().color(palette.directory));
        }
        if text.truncated {
            ui.label(RichText::new("truncated").small().color(palette.readonly));
        }
        ui.checkbox(&mut state.wraps, "wrap");
    });
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut text.text.clone())
                    .font(TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .interactive(false),
            );
        });
}

fn show_hex(
    ui: &mut egui::Ui,
    state: &mut crate::model::ViewerState,
    loaded: &LoadedViewer,
    palette: &Palette,
) {
    let doc = match &loaded.content {
        ViewerContent::Hex(doc) => doc,
        ViewerContent::Binary(binary) => &binary.head_hex,
        _ => {
            ui.label(rc_viewer::describe(loaded));
            return;
        }
    };
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(format!("{} bytes", doc.total_bytes))
                .small()
                .color(palette.hidden),
        );
        if doc.truncated {
            ui.label(RichText::new("truncated").small().color(palette.readonly));
        }
        ui.add(egui::Slider::new(&mut state.hex_bytes, 1..=64).text(model_bytes_label()));
    });
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut text = String::new();
            for line in &doc.lines {
                text.push_str(&format!(
                    "{:08x}  {:<48}  |{}|\n",
                    line.offset, line.hex, line.ascii
                ));
            }
            if text.is_empty() {
                ui.label(rc_viewer::describe(loaded));
            } else {
                ui.add(egui::Label::new(RichText::new(text).monospace()).selectable(true));
            }
        });
}

fn model_bytes_label() -> String {
    "bytes/line".to_string()
}

fn show_image(ui: &mut egui::Ui, state: &mut crate::model::ViewerState, ui_state: &ViewerUi) {
    let Some(texture) = &ui_state.image_texture else {
        ui.label(
            ui_state
                .error
                .clone()
                .unwrap_or_else(|| "image not available".to_string()),
        );
        return;
    };
    let size = texture.size_vec2();
    let available = ui.available_size();
    let scale = if state.zoom <= 0.0 {
        (available.x / size.x).min(available.y / size.y).min(1.0)
    } else {
        state.zoom
    };
    ui.horizontal(|ui| {
        if ui.button("-").clicked() {
            state.zoom = (if state.zoom <= 0.0 { scale } else { state.zoom } * 0.8).max(0.05);
        }
        if ui.button("+").clicked() {
            state.zoom = (if state.zoom <= 0.0 { scale } else { state.zoom } * 1.25).min(16.0);
        }
        if ui.button("fit").clicked() {
            state.zoom = 0.0;
        }
        ui.label(format!("{:.0}%", scale * 100.0));
    });
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.image((texture.id(), size * scale));
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_viewer_state_is_empty() {
        let ui_state = ViewerUi::default();
        assert!(ui_state.loaded.is_none());
        assert!(ui_state.image_texture.is_none());
        assert_eq!(ui_state.status(), "");
        assert_eq!(ui_state.options.hex_bytes_per_line, 16);
    }

    #[test]
    fn loading_a_missing_file_reports_an_error() {
        let mut ui_state = ViewerUi::default();
        assert!(!ui_state.load(&std::path::PathBuf::from("/definitely/not/here.txt")));
        assert!(ui_state.error.is_some());
        assert!(ui_state.status().contains("here.txt") || !ui_state.status().is_empty());
    }

    #[test]
    fn loading_a_text_file_produces_a_text_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.txt");
        std::fs::write(&path, "hello world\n").unwrap();
        let mut ui_state = ViewerUi::default();
        assert!(ui_state.load(&path));
        assert!(matches!(
            ui_state.loaded.as_ref().map(|l| &l.content),
            Some(ViewerContent::Text(_))
        ));
        assert!(!ui_state.status().is_empty());
    }

    #[test]
    fn auto_mode_follows_the_detected_content() {
        assert_eq!(
            effective_mode(ViewerMode::Auto, &ViewerUi::default()),
            ViewerMode::Hex
        );
        assert_eq!(
            effective_mode(ViewerMode::Text, &ViewerUi::default()),
            ViewerMode::Text
        );
    }
}
