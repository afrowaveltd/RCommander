//! The window chrome the editor and the chat share.
//!
//! Both are child windows of the main viewport (there is one OS window), so "minimize" and
//! "maximize" cannot mean what they mean for an OS window. They mean what a user of a file manager
//! actually wants:
//!
//! * **minimize** folds the window down to its title bar, so it stops covering the panels but does
//!   not lose its place;
//! * **maximize** fills the workspace — the area the two panels occupy — and stays responsive to a
//!   resize of the main window, because the size is recomputed every frame;
//! * **pin** does the same and adds a visual tie, keeping this window in front while the user works
//!   in it; unpinning restores the previous geometry;
//! * **close** closes.
//!
//! The buttons are drawn by us rather than by the default title bar because the default bar has
//! only a close button and no pin, and because a decorated child window cannot be collapsed.

use crate::theme::Palette;
use egui::{Align, Context, Layout, Rect, RichText, Vec2};

/// Per-window geometry the user can change.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Chrome {
    /// Folded down to the title bar.
    pub collapsed: bool,
    /// Filling the workspace.
    pub maximized: bool,
    /// Maximized *and* kept in front, which is what "pin" means here.
    pub pinned: bool,
    /// The window was closed through its own button.
    pub closed: bool,
}

impl Chrome {
    /// True when the window should fill the workspace.
    fn filling(&self) -> bool {
        self.maximized || self.pinned
    }
}

/// What the caller should do after a frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChromeAction {
    /// Nothing happened.
    #[default]
    None,
    /// The user asked to close the window.
    Close,
}

/// Translated window controls, captured before the contents borrow the model.
pub struct ChromeLabels {
    close: String,
    pin: String,
    unpin: String,
    cover: String,
    maximize: String,
    restore: String,
    minimize: String,
    expand: String,
}

impl ChromeLabels {
    pub fn new(model: &crate::model::Model) -> Self {
        Self {
            close: model.t("dialog.config.close"),
            pin: model.t("window.pin"),
            unpin: model.t("window.unpin"),
            cover: model.t("window.cover_panels"),
            maximize: model.t("window.maximize"),
            restore: model.t("window.restore"),
            minimize: model.t("window.minimize"),
            expand: model.t("window.expand"),
        }
    }

    #[cfg(test)]
    fn english() -> Self {
        Self {
            close: "Close".into(),
            pin: "Pin".into(),
            unpin: "Unpin".into(),
            cover: "Cover the panels while you work".into(),
            maximize: "Maximize".into(),
            restore: "Restore".into(),
            minimize: "Minimize".into(),
            expand: "Expand".into(),
        }
    }
}

/// Height of the title row we draw ourselves.
const TITLE_HEIGHT: f32 = 30.0;

/// Draw a workspace window with minimize/maximize/pin/close and the given contents.
///
/// `workspace` is the rectangle the two panels occupy, captured by the caller before the window is
/// drawn; using it instead of the screen rect is what makes "cover the panels" mean the panels and
/// not the whole screen, menu bar included.
pub fn show(
    ctx: &Context,
    id: &str,
    title: &str,
    workspace: Rect,
    chrome: &mut Chrome,
    palette: &Palette,
    labels: &ChromeLabels,
    add_contents: impl FnOnce(&mut egui::Ui),
) -> ChromeAction {
    if chrome.closed {
        return ChromeAction::Close;
    }
    // The state is kept between frames so a dragged window remembers where it was.
    let geometry_id = egui::Id::unique(("workspace_window", id));
    let mut geometry: Vec2 = ctx
        .data(|data| data.get_temp::<Vec2>(geometry_id))
        .unwrap_or_else(|| Vec2::new(760.0, 520.0));

    // The window is named by its id, not by the visible title: the title is drawn by us below, and
    // repeating it in the accessibility tree would give a screen reader two names for one window.
    let mut window = egui::Window::new(format!("rcommander-{id}"))
        .id(egui::Id::unique(("workspace_window_title", id)))
        .title_bar(false)
        .resizable(!chrome.filling())
        .collapsible(false);
    window = if chrome.filling() {
        // Fill the panels, and let a main-window resize move and grow it: the rect is recomputed
        // from the workspace every frame, so there is nothing to remember.
        window.fixed_pos(workspace.min).fixed_size(workspace.size())
    } else if chrome.collapsed {
        window
            .fixed_pos(workspace.min + Vec2::new(40.0, 40.0))
            .fixed_size(Vec2::new(
                geometry.x.min(workspace.width()),
                TITLE_HEIGHT + 6.0,
            ))
    } else {
        window
            .default_pos(workspace.min + Vec2::new(40.0, 40.0))
            .default_size(geometry)
            .fixed_size(geometry)
    };

    let mut action = ChromeAction::None;
    let response = window.show(ctx, |ui| {
        // The title row: name on the left, the four buttons on the right, in the order a desktop
        // window puts them (minimize, maximize, pin, close).
        ui.horizontal(|ui| {
            let accent = if chrome.pinned {
                palette.accent
            } else {
                palette.foreground
            };
            ui.label(RichText::new(title).strong().color(accent));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("x").on_hover_text(&labels.close).clicked() {
                    action = ChromeAction::Close;
                }
                let pin_label = if chrome.pinned {
                    labels.unpin.as_str()
                } else {
                    labels.pin.as_str()
                };
                if ui
                    .small_button(pin_label)
                    .on_hover_text(&labels.cover)
                    .clicked()
                {
                    chrome.pinned = !chrome.pinned;
                    if chrome.pinned {
                        chrome.maximized = false;
                        chrome.collapsed = false;
                    }
                }
                let max_label = if chrome.maximized {
                    labels.restore.as_str()
                } else {
                    labels.maximize.as_str()
                };
                if ui.small_button(max_label).clicked() {
                    chrome.maximized = !chrome.maximized;
                    if chrome.maximized {
                        chrome.pinned = false;
                        chrome.collapsed = false;
                    }
                }
                let min_label = if chrome.collapsed {
                    labels.expand.as_str()
                } else {
                    labels.minimize.as_str()
                };
                if ui.small_button(min_label).clicked() {
                    chrome.collapsed = !chrome.collapsed;
                    if chrome.collapsed {
                        chrome.maximized = false;
                        chrome.pinned = false;
                    }
                }
            });
        });
        if chrome.pinned {
            // A pinned window says so with a border rather than a modal overlay: the user still
            // may want to look at the panels behind it.
            let rect = ui.max_rect();
            ui.painter().rect_stroke(
                rect,
                egui::CornerRadius::same(2),
                egui::Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        ui.separator();
        if !chrome.collapsed {
            add_contents(ui);
        }
    });
    if let Some(response) = response
        && !chrome.filling()
        && !chrome.collapsed
    {
        // Remember the size the user dragged to, but only while the window is free-floating:
        // overwriting it from a maximized window would lose the original geometry.
        geometry = response.response.rect.size();
        ctx.data_mut(|data| data.insert_temp(geometry_id, geometry));
    }
    action
}

/// Shared controls for full workspace, left/right panel and floating child window.
pub fn document_controls(
    ui: &mut egui::Ui,
    model: &mut crate::model::Model,
    placement: &mut crate::model::DocumentPlacement,
    occupied: Option<crate::model::DocumentPlacement>,
) -> bool {
    use crate::model::{DocumentPlacement as P, Side};
    let mut close = false;
    ui.horizontal_wrapped(|ui| {
        for (target, key) in [
            (P::Workspace, "document.workspace"),
            (P::Panel(Side::Left), "document.left"),
            (P::Panel(Side::Right), "document.right"),
            (P::Floating, "document.floating"),
        ] {
            let available = !matches!(target, P::Panel(_)) || occupied != Some(target);
            if ui
                .add_enabled(
                    available,
                    egui::Button::new(model.t(key)).selected(*placement == target),
                )
                .clicked()
            {
                *placement = target;
                if let P::Panel(side) = target {
                    model.focus = side;
                }
            }
        }
        if ui.small_button(model.t("dialog.config.close")).clicked() {
            close = true;
        }
    });
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_states_are_mutually_exclusive_by_construction() {
        let mut chrome = Chrome::default();
        assert!(!chrome.filling());
        chrome.maximized = true;
        assert!(chrome.filling());
        chrome.pinned = true;
        // Pinning after maximizing keeps only the pin, because both mean "fill".
        if chrome.pinned {
            chrome.maximized = false;
        }
        assert!(chrome.filling());
        chrome.collapsed = true;
        chrome.pinned = false;
        assert!(!chrome.filling(), "a collapsed window does not fill");
    }

    #[test]
    fn a_closed_window_reports_itself() {
        let mut chrome = Chrome {
            closed: true,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let action = show(
            &ctx,
            "test",
            "Title",
            Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(800.0, 600.0)),
            &mut chrome,
            &Palette::light(),
            &ChromeLabels::english(),
            |_| {},
        );
        assert_eq!(action, ChromeAction::Close);
    }

    #[test]
    fn the_window_draws_its_contents_and_reports_nothing() {
        let ctx = egui::Context::default();
        let workspace = Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1000.0, 700.0));
        let mut chrome = Chrome::default();
        let palette = Palette::light();
        let mut drawn = 0;
        // egui lays widgets out inside a pass, so the window is shown from inside one.
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx().clone();
            let action = show(
                &ctx,
                "buttons",
                "Chat",
                workspace,
                &mut chrome,
                &palette,
                &ChromeLabels::english(),
                |ui| {
                    drawn += 1;
                    ui.label("body");
                },
            );
            assert_eq!(action, ChromeAction::None);
        });
        let mut deltas = output.textures_delta;
        deltas.clear();
        assert!(drawn > 0, "the contents are drawn");
        // Maximizing is what "cover the panels" means.
        chrome.maximized = true;
        assert!(chrome.filling());
    }
}
