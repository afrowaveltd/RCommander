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
    markdown: Option<Vec<rc_help::Block>>,
    data: Option<super::data_preview::DataPreview>,
    markdown_limited: bool,
    hex: Option<Result<rc_viewer::HexDocument, String>>,
    link_target: Option<String>,
}

impl ViewerUi {
    /// Load `path` into the viewer, honouring the options.
    pub fn load(&mut self, path: &std::path::Path) -> bool {
        self.image_texture = None;
        self.markdown = None;
        self.data = None;
        self.markdown_limited = false;
        self.hex = None;
        self.link_target = None;
        match rc_viewer::load(path, &self.options) {
            Ok(loaded) => {
                self.error = None;
                if loaded.format.format_hint == "markdown"
                    && let ViewerContent::Text(text) = &loaded.content
                {
                    let mut end = text.text.len().min(256 * 1024);
                    while !text.text.is_char_boundary(end) {
                        end -= 1;
                    }
                    self.markdown_limited = end < text.text.len() || text.truncated;
                    self.markdown = Some(rc_help::parse_markdown(&text.text[..end]));
                }
                if let ViewerContent::Text(text) = &loaded.content {
                    self.data = super::data_preview::DataPreview::parse(
                        &loaded.format.format_hint, &loaded.format.mime, &text.text, text.truncated,
                    );
                }
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

    /// Reuse the chat/help renderer. Links are displayed, never opened automatically.
    pub fn markdown_preview(
        &mut self, ui: &mut egui::Ui, palette: &Palette, model: &Model, compact: bool,
    ) -> bool {
        let Some(blocks) = &self.markdown else { return false };
        let limit = if compact { 32 } else { 2048 };
        if self.markdown_limited || blocks.len() > limit {
            ui.label(model.t("viewer.preview.limited"));
        }
        let viewport = ui.available_size().max(egui::vec2(1.0, 1.0));
        egui::ScrollArea::both()
            .id_salt("document_markdown")
            .max_width(viewport.x)
            .max_height(viewport.y)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width((viewport.x - 20.0).max(1.0));
                crate::ui::markdown_view::render_blocks(
                    ui, &blocks[..blocks.len().min(limit)], palette, &mut self.link_target,
                );
                if let Some(target) = &self.link_target {
                    ui.separator();
                    ui.label(model.t("viewer.preview.link"));
                    ui.add(egui::Label::new(target.as_str()).selectable(true));
                }
            });
        true
    }

    pub fn data_preview(&self, ui: &mut egui::Ui, model: &Model, compact: bool) -> bool {
        self.data.as_ref().is_some_and(|data| data.render(ui, model, compact))
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
    render(ctx, None, model, ui_state, palette)
}

/// Render the same viewer in the workspace or a single panel.
pub fn embedded(ui: &mut egui::Ui, model: &mut Model, ui_state: &mut ViewerUi, palette: &Palette) {
    let ctx = ui.ctx().clone();
    render(&ctx, Some(ui), model, ui_state, palette);
}

fn render(
    ctx: &Context,
    embedded: Option<&mut egui::Ui>,
    model: &mut Model,
    ui_state: &mut ViewerUi,
    palette: &Palette,
) -> bool {
    let Some(mut state) = model.viewer.take() else {
        return true;
    };
    let mut open = true;
    let title = format!("{} — {}", model.t("viewer.text"), state.title);
    let occupied = model.editor.as_ref().map(|s| s.placement);
    let mut close_requested = false;
    let mut contents = |ui: &mut egui::Ui| {
        ui.label(&title);
        close_requested = crate::ui::workspace_window::document_controls(
            ui,
            model,
            &mut state.placement,
            occupied,
        );
        ui.horizontal_wrapped(|ui| {
            for (label, mode) in [
                ("viewer.mode.auto", ViewerMode::Auto),
                ("viewer.mode.source", ViewerMode::Text),
                ("viewer.mode.hex", ViewerMode::Hex),
                ("viewer.mode.image", ViewerMode::Image),
            ] {
                if ui.selectable_label(state.mode == mode, model.t(label)).clicked() {
                    state.mode = mode;
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
        if state.mode == ViewerMode::Auto
            && (ui_state.markdown_preview(ui, palette, model, false)
                || ui_state.data_preview(ui, model, false))
        {
            return;
        }
        let mode = effective_mode(state.mode, ui_state);
        if mode == ViewerMode::Hex && ui_state.loaded.is_some() {
            if let Some(LoadedViewer { content: ViewerContent::Hex(doc), .. }) = &ui_state.loaded {
                show_hex(ui, &mut state, doc, palette);
            } else {
                let options = ui_state.options;
                let dump = ui_state.hex.get_or_insert_with(|| {
                    rc_viewer::load_hex(std::path::Path::new(&state.path), &options)
                        .map_err(|e| e.to_string())
                });
                match dump {
                    Ok(doc) => show_hex(ui, &mut state, doc, palette),
                    Err(error) => { ui.label(error.as_str()); }
                }
            }
            return;
        }
        match (&ui_state.loaded, mode) {
            (Some(LoadedViewer { content: ViewerContent::Image(_), .. }), ViewerMode::Image) => {
                show_image(ui, &mut state, ui_state);
            }
            (Some(loaded), ViewerMode::Text) if matches!(&loaded.content, ViewerContent::Text(_)) => {
                show_text(ui, &mut state, loaded, palette);
            }
            (Some(loaded), ViewerMode::Auto) => {
                ui.label(rc_viewer::describe(loaded));
                if !matches!(&loaded.content, ViewerContent::Empty) {
                    ui.label(model.t("viewer.preview.unsupported"));
                }
            }
            (Some(loaded), _) => {
                ui.label(rc_viewer::describe(loaded));
                ui.label(model.t("viewer.preview.mode_unavailable"));
            }
            (None, _) => {
                ui.label(ui_state.error.clone().unwrap_or_else(|| model.t("panel.empty")));
            }
        }
    };
    if let Some(ui) = embedded {
        contents(ui);
    } else {
        egui::Window::new(&title)
            .id(egui::Id::unique("document_viewer"))
            .open(&mut open)
            .resizable(true)
            .default_size([760.0, 560.0])
            .show(ctx, contents);
    }
    open = open && !close_requested;
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
        Some(_) => ViewerMode::Auto,
        None => ViewerMode::Hex,
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
    let viewport = ui.available_size().max(egui::vec2(1.0, 1.0));
    egui::ScrollArea::both()
        .id_salt("document_text")
        .max_width(viewport.x)
        .max_height(viewport.y)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // A borrowed str is a read-only TextBuffer: selection and copying
            // remain available without allocating a mutable clone every frame.
            let mut read_only = text.text.as_str();
            let font = TextStyle::Monospace.resolve(ui.style());
            let wraps = state.wraps;
            let path = state.path.as_str();
            let mut layouter = |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, width: f32| {
                let mut job = crate::ui::markdown_view::source_layout(
                    path, buffer.as_str(), palette, font.clone(), true,
                );
                job.wrap.max_width = if wraps { width } else { f32::INFINITY };
                ui.fonts_mut(|fonts| fonts.layout_job(job))
            };
            ui.add(
                egui::TextEdit::multiline(&mut read_only)
                    .font(TextStyle::Monospace)
                    .desired_width((viewport.x - 20.0).max(1.0))
                    .desired_rows(1)
                    .layouter(&mut layouter),
            );
        });
}

/// The hexdump: real scrolling, a width the user can change, and a detail panel for one row.
///
/// Three things were wrong with the first version and each has a cause worth keeping in mind:
///
/// * the "bytes per line" slider changed a number nothing re-read, so it did nothing — the dump is
///   now re-formatted from its own bytes when the value changes;
/// * the whole dump was one huge label inside an unbounded scroll area, so the *window* grew
///   instead of the content scrolling — now the rows are drawn with `show_rows`, which both scrolls
///   and keeps a 10 MB dump cheap;
/// * a byte was only a dot in a gutter. Clicking a row now shows the offset in hex and decimal and
///   the row's bytes as unsigned, signed, 16- and 32-bit values, which is what "study it in detail"
///   means when you are looking at a file format.
fn show_hex(
    ui: &mut egui::Ui,
    state: &mut crate::model::ViewerState,
    doc: &rc_viewer::HexDocument,
    palette: &Palette,
) {
    // The slider edits a request; the dump is rebuilt when it differs from what is on screen.
    let mut width = state.hex_bytes.clamp(1, 64);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!("{} bytes", doc.total_bytes))
                .small()
                .color(palette.hidden),
        );
        if doc.truncated {
            ui.label(RichText::new("truncated").small().color(palette.readonly));
        }
        if ui
            .add(egui::Slider::new(&mut width, 1..=64).text(model_bytes_label()))
            .changed()
        {
            state.hex_bytes = width;
        }
        // Jumping to an offset is how a person reads a file format: the header says "at 0x200".
        ui.label(RichText::new("|").color(palette.hidden));
        ui.label("go to");
        ui.add(
            egui::TextEdit::singleline(&mut state.hex_goto)
                .desired_width(90.0)
                .hint_text("0x200"),
        );
        if ui.button("→").clicked()
            && let Some(offset) = parse_offset(&state.hex_goto)
        {
            let per_line = doc.bytes_per_line.max(1) as u64;
            state.hex_selected = (offset / per_line) as usize;
        }
    });
    let view: std::borrow::Cow<'_, rc_viewer::HexDocument> =
        if state.hex_bytes != doc.bytes_per_line {
            std::borrow::Cow::Owned(rc_viewer::reformat(doc, state.hex_bytes))
        } else {
            std::borrow::Cow::Borrowed(doc)
        };
    let view = view.as_ref();

    let row_height = ui.text_style_height(&egui::TextStyle::Monospace) + 2.0;
    let selected = state.hex_selected.min(view.lines.len().saturating_sub(1));
    let mut clicked_row: Option<usize> = None;
    // The detail panel is a bottom strip, so the rows keep the rest of the height and scroll.
    let detail_height = if view.lines.is_empty() { 0.0 } else { 66.0 };
    egui::Panel::bottom("hex_detail")
        .resizable(false)
        .exact_size(detail_height)
        .show(ui, |ui| {
            if let Some(line) = view.lines.get(selected) {
                ui.label(
                    RichText::new(format!(
                        "offset {} (0x{:x}) · {} bytes",
                        line.offset,
                        line.offset,
                        line.bytes.len()
                    ))
                    .small()
                    .color(palette.hidden),
                );
                ui.label(
                    RichText::new(format!(
                        "u8: {}   i8: {}   u16: {}   u32: {}",
                        describe_unsigned(&line.bytes),
                        describe_signed(&line.bytes),
                        describe_u16(&line.bytes),
                        describe_u32(&line.bytes)
                    ))
                    .small()
                    .monospace(),
                );
                ui.label(
                    RichText::new(format!("ascii: |{}|", line.ascii))
                        .small()
                        .monospace(),
                );
            }
        });
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .id_salt("hex_rows")
        .show_rows(ui, row_height, view.lines.len(), |ui, range| {
            let font = egui::TextStyle::Monospace.resolve(ui.style());
            for index in range {
                let line = &view.lines[index];
                let text = format!(
                    "{:08x}  {:<width$}  |{}|",
                    line.offset,
                    line.hex,
                    line.ascii,
                    width = view.bytes_per_line * 3
                );
                let mut job = egui::text::LayoutJob::default();
                job.append(
                    &text,
                    0.0,
                    egui::TextFormat {
                        font_id: font.clone(),
                        color: if index == selected {
                            palette.selected_foreground
                        } else {
                            palette.foreground
                        },
                        background: if index == selected {
                            palette.selected_background
                        } else {
                            egui::Color32::TRANSPARENT
                        },
                        ..Default::default()
                    },
                );
                let response = ui.add(egui::Label::new(job).sense(egui::Sense::click()));
                if response.clicked() {
                    clicked_row = Some(index);
                }
            }
        });
    if let Some(index) = clicked_row {
        state.hex_selected = index;
    }
}

/// Parse `0x200`, `512` or `200h` into a byte offset.
fn parse_offset(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    if text.is_empty() {
        return None;
    }
    if let Some(hex) = text.strip_prefix("0x") {
        return u64::from_str_radix(hex, 16).ok();
    }
    if let Some(hex) = text.strip_suffix('h') {
        return u64::from_str_radix(hex, 16).ok();
    }
    if let Ok(decimal) = text.parse::<u64>() {
        return Some(decimal);
    }
    u64::from_str_radix(&text, 16).ok()
}

fn describe_unsigned(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(8)
        .map(|byte| byte.to_string())
        .collect::<Vec<String>>()
        .join(" ")
}

fn describe_signed(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(8)
        .map(|byte| (*byte as i8).to_string())
        .collect::<Vec<String>>()
        .join(" ")
}

fn describe_u16(bytes: &[u8]) -> String {
    bytes
        .chunks_exact(2)
        .take(4)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]).to_string())
        .collect::<Vec<String>>()
        .join(" ")
}

fn describe_u32(bytes: &[u8]) -> String {
    bytes
        .chunks_exact(4)
        .take(2)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]).to_string())
        .collect::<Vec<String>>()
        .join(" ")
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

#[cfg(test)]
mod hex_tests {
    use super::*;

    #[test]
    fn an_offset_can_be_typed_the_ways_a_person_types_it() {
        assert_eq!(parse_offset("0x200"), Some(0x200));
        assert_eq!(parse_offset("512"), Some(512));
        assert_eq!(parse_offset("200h"), Some(0x200));
        assert_eq!(parse_offset("  ff  "), Some(0xff));
        assert_eq!(parse_offset(""), None);
        assert_eq!(parse_offset("nothing"), None);
    }

    #[test]
    fn the_byte_columns_read_as_numbers_in_every_width_that_matters() {
        let bytes = [0x01u8, 0x02, 0xff, 0x00];
        assert_eq!(describe_unsigned(&bytes), "1 2 255 0");
        // The same bytes signed: 0xff is -1, which is the whole reason the column exists.
        assert_eq!(describe_signed(&bytes), "1 2 -1 0");
        assert_eq!(describe_u16(&bytes), "513 255");
        // Little-endian, which is what every x86 file format means by these columns.
        assert_eq!(describe_u32(&bytes), "16712193");
        // An empty row prints nothing rather than panicking.
        assert!(describe_u32(&[]).is_empty());
    }
}
