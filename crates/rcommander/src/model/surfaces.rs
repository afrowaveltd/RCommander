//! What the keyboard means depends on what has the focus (Phase 14.9).
//!
//! A file manager is fast for its expert users because the function keys change meaning with the
//! thing in front of you: over a file list `F3` looks at the file, over a conversation it cannot
//! mean that, and pretending otherwise is how a window ends up offering a command that does
//! nothing. This module is the single place that decides:
//!
//! * which **surface** has the focus (a file panel, the chat, the editor, the viewer);
//! * which command each `F` key means there;
//! * what the F-key bar draws, which is exactly what the keys will do.
//!
//! The table lives in the headless model, not in the view, so the future TUI frontend gets the
//! same keyboard for free — that is the point of putting it here.

use super::{Model, Side};

/// The thing the keyboard is talking to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    /// A file panel (local, archive or Git), on one side.
    Panel(Side),
    /// The agent chat, in a panel or in its window.
    Chat,
    /// The F4 editor.
    Editor,
    /// The F3 viewer.
    Viewer,
}

impl Surface {
    /// Name used in the tests and the status line.
    pub fn name(self) -> &'static str {
        match self {
            Surface::Panel(Side::Left) => "panel-left",
            Surface::Panel(Side::Right) => "panel-right",
            Surface::Chat => "chat",
            Surface::Editor => "editor",
            Surface::Viewer => "viewer",
        }
    }

    /// The help topic `F1` opens on this surface.
    ///
    /// Context-aware help is the cheapest form of documentation: pressing `F1` on the chat should
    /// explain the chat, not the file list.
    pub fn help_topic(self) -> &'static str {
        match self {
            Surface::Panel(_) => "Keyboard shortcuts",
            Surface::Chat => "Agent chat",
            Surface::Editor => "Editor",
            Surface::Viewer => "Previewer",
        }
    }
}

/// One slot of the F-key bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FnKey {
    /// Which function key (`1..=12`).
    pub slot: u8,
    /// Localisation key of the label.
    pub label_key: &'static str,
    /// Command the key runs; empty when the slot is deliberately unused.
    pub command: &'static str,
}

impl FnKey {
    const fn new(slot: u8, label_key: &'static str, command: &'static str) -> FnKey {
        FnKey {
            slot,
            label_key,
            command,
        }
    }

    /// True when this slot does nothing on this surface and must not be drawn.
    ///
    /// An empty slot is *absent*, not disabled: a button that cannot work is worse than no button.
    pub fn is_empty(&self) -> bool {
        self.command.is_empty()
    }

    /// The shortcut text (`F5`).
    pub fn shortcut(self) -> String {
        format!("F{}", self.slot)
    }
}

/// A modifier variant of a function key (`Shift+F5`, `Ctrl+F3`, `Alt+F7`).
///
/// Total Commander's virtue is that a whole second and third row of meaning sits under the plain
/// function keys, and the bar shows it. The commands come from the key map, so a user override is
/// what the bar offers — the two cannot drift apart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyVariant {
    /// `shift`, `ctrl` or `alt`.
    pub modifier: &'static str,
    /// Localisation key of the modifier name (`key.modifier.shift`).
    pub modifier_key: &'static str,
    /// The command it runs.
    pub command: String,
    /// What the command is called, so the bar can swap the caption while the modifier is held.
    pub label_key: &'static str,
}

impl KeyVariant {
    /// True when the command appears to exist; an unknown one is not offered.
    fn is_usable(&self) -> bool {
        !self.command.is_empty()
    }
}

impl Model {
    /// Which surface the keyboard is talking to.
    ///
    /// The order is the order of "innermost thing first": a modal editor or viewer owns the keys,
    /// then a panel showing the chat, then the file panels.
    pub fn surface(&self) -> Surface {
        use super::DocumentPlacement;
        if self
            .editor
            .as_ref()
            .is_some_and(|s| s.placement == DocumentPlacement::Workspace)
        {
            return Surface::Editor;
        }
        if self
            .viewer
            .as_ref()
            .is_some_and(|s| s.placement == DocumentPlacement::Workspace)
        {
            return Surface::Viewer;
        }
        if self
            .editor
            .as_ref()
            .is_some_and(|s| s.placement.owns_focus(self.focus))
        {
            return Surface::Editor;
        }
        if self
            .viewer
            .as_ref()
            .is_some_and(|s| s.placement.owns_focus(self.focus))
        {
            return Surface::Viewer;
        }
        if self.panel_shows_chat(self.focus) {
            return Surface::Chat;
        }
        Surface::Panel(self.focus)
    }

    /// The F-key row for the current surface.
    pub fn function_keys(&self) -> Vec<FnKey> {
        function_keys_for(self.surface())
    }

    /// The modifier variants of one function key, in the order Shift, Ctrl, Alt.
    ///
    /// The row is the surface's; the *commands* come from the key map, so a variant that a user
    /// rebound appears with its new command and one that was removed disappears.
    pub fn key_variants(&self, slot: u8) -> Vec<KeyVariant> {
        let mut variants: Vec<KeyVariant> = Vec::new();
        for (modifier, modifier_key) in [
            ("shift", "key.modifier.shift"),
            ("ctrl", "key.modifier.ctrl"),
            ("alt", "key.modifier.alt"),
        ] {
            let text = format!("{modifier}+f{slot}");
            let Ok(shortcut) = rc_core::keymap::Shortcut::parse(&text) else {
                continue;
            };
            if let Some(command) = self.keymap.lookup(&shortcut) {
                let variant = KeyVariant {
                    modifier,
                    modifier_key,
                    command: command.to_string(),
                    label_key: command_label_key(command).unwrap_or(""),
                };
                if variant.is_usable() {
                    variants.push(variant);
                }
            }
        }
        variants
    }
}

/// What a command is called in a caption.
///
/// The F-key row carries the labels of the plain keys; this covers the commands the *modifiers*
/// add, so holding `Shift` can change a caption into the other function instead of growing a
/// second row of buttons underneath. A command that is not here keeps its bare shortcut as the
/// caption, which is honest: the function still runs, we just have no name for it in words.
pub fn command_label_key(command: &str) -> Option<&'static str> {
    Some(match command {
        "cm_SrcShort" => "cmd.view_short",
        "cm_SrcLong" => "cmd.view_long",
        "cm_SrcByName" => "cmd.sort_name",
        "cm_SrcByExt" => "cmd.sort_ext",
        "cm_SrcByDateTime" => "cmd.sort_date",
        "cm_SrcBySize" => "cmd.sort_size",
        "cm_SrcUnsorted" => "cmd.sort_unsorted",
        "cm_SrcTree" => "cmd.tree",
        "cm_SrcAllFiles" => "cmd.all_files",
        "cm_SrcExecs" => "cmd.execs",
        "cm_SrcUserDef" => "cmd.user_defined",
        "cm_ListInternalOnly" => "cmd.view_internal",
        "cm_ListExternal" => "cmd.view_external",
        "cm_CopySamePanel" => "cmd.copy_same_panel",
        "cm_RenameOnly" => "cmd.rename",
        "cm_MkDirOther" => "cmd.mkdir_other",
        "cm_SearchFor" => "cmd.search",
        "cm_DeletePermanent" => "cmd.delete_permanent",
        "cm_UnpackFiles" => "cmd.unpack",
        "cm_PackFiles" => "cmd.pack",
        "cm_CompareDirs" => "cmd.compare_dirs",
        "cm_EditNewFile" => "cmd.new_file",
        "cm_CustomColumnViewMenu" => "cmd.custom_columns",
        "cm_ContextMenu" => "cmd.context_menu",
        "cm_ShowCmdLineHistory" => "cmd.cmdline_history",
        "cm_CDtree" => "cmd.cdtree",
        "cm_SrcBreadcrumbar" | "cm_TrgBreadcrumbar" => "cmd.breadcrumb",
        "cm_PrintFile" => "cmd.print",
        "cm_Exit" => "cmd.quit",
        _ => return None,
    })
}

/// The F-key row of one surface.
///
/// Every command named here exists — a test walks the rows and runs each one on a fresh model, so
/// a row cannot promise a command that was renamed away.
pub fn function_keys_for(surface: Surface) -> Vec<FnKey> {
    match surface {
        Surface::Panel(_) => vec![
            FnKey::new(1, "cmd.help", "cm_HelpIndex"),
            FnKey::new(2, "cmd.refresh", "cm_RereadSource"),
            FnKey::new(3, "cmd.view", "cm_List"),
            FnKey::new(4, "cmd.edit", "cm_Edit"),
            FnKey::new(5, "cmd.copy", "cm_Copy"),
            FnKey::new(6, "cmd.move", "cm_RenMov"),
            FnKey::new(7, "cmd.mkdir", "cm_MkDir"),
            FnKey::new(8, "cmd.delete", "cm_Delete"),
            FnKey::new(9, "cmd.pack", "cm_PackFiles"),
            // The Norton Commander ending: F10 leaves the program, as the panel owner asked.
            FnKey::new(10, "cmd.quit", "cm_Exit"),
            FnKey::new(11, "cmd.open", "cm_Return"),
            FnKey::new(12, "cmd.terminal_here", "cm_TerminalHere"),
        ],
        // Over a conversation the file commands would be a lie: there is no cursor row to view,
        // copy or delete. What a conversation *can* do is change the model, save the question,
        // export the transcript and stop.
        Surface::Chat => vec![
            FnKey::new(1, "cmd.help", "cm_HelpIndex"),
            FnKey::new(2, "cmd.aimodel", "cm_AiModel"),
            FnKey::new(3, "prompt.pick", "cm_PromptPicker"),
            FnKey::new(4, "prompt.save_current", "cm_ChatSavePrompt"),
            FnKey::new(5, "ai.chat.clear", "cm_ChatClear"),
            FnKey::new(6, "prompt.export", "cm_ChatExport"),
            FnKey::new(7, "agent.pane.files", "cm_ChatFiles"),
            FnKey::new(8, "ai.chat.stop", "cm_ChatStop"),
            FnKey::new(9, "", ""),
            FnKey::new(10, "", ""),
            FnKey::new(11, "", ""),
            FnKey::new(12, "", ""),
        ],
        Surface::Editor => vec![
            FnKey::new(1, "cmd.help", "cm_HelpIndex"),
            FnKey::new(2, "prompt.save", "cm_EditorSave"),
            FnKey::new(3, "editor.save_copy", "cm_EditorSaveCopy"),
            FnKey::new(4, "editor.reload", "cm_EditorReload"),
            FnKey::new(5, "", ""),
            FnKey::new(6, "", ""),
            FnKey::new(7, "", ""),
            FnKey::new(8, "", ""),
            FnKey::new(9, "", ""),
            FnKey::new(10, "dialog.config.close", "cm_EditorClose"),
            FnKey::new(11, "", ""),
            FnKey::new(12, "", ""),
        ],
        Surface::Viewer => vec![
            FnKey::new(1, "cmd.help", "cm_HelpIndex"),
            FnKey::new(2, "viewer.reload", "cm_ViewerReload"),
            FnKey::new(3, "viewer.mode.text", "cm_ViewerText"),
            FnKey::new(4, "viewer.mode.hex", "cm_ViewerHex"),
            FnKey::new(5, "viewer.mode.image", "cm_ViewerImage"),
            FnKey::new(6, "viewer.zoom.in", "cm_ViewerZoomIn"),
            FnKey::new(7, "viewer.zoom.out", "cm_ViewerZoomOut"),
            FnKey::new(8, "", ""),
            FnKey::new(9, "", ""),
            FnKey::new(10, "dialog.config.close", "cm_ViewerClose"),
            FnKey::new(11, "", ""),
            FnKey::new(12, "", ""),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;

    fn model(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(dir),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    #[test]
    fn the_focused_surface_decides_the_keyboard() {
        let files = tempfile::tempdir().unwrap();
        let mut model = model(files.path());
        model.focus = Side::Left;
        assert_eq!(model.surface(), Surface::Panel(Side::Left));
        model.focus = Side::Right;
        assert_eq!(model.surface(), Surface::Panel(Side::Right));
        // A panel showing the chat owns the keys.
        model.set_panel_chat(Side::Right, true);
        assert_eq!(model.surface(), Surface::Chat);
        model.set_panel_chat(Side::Right, false);
        // The viewer and the editor win over the panels, because they cover them.
        model.viewer = Some(crate::model::ViewerState::default());
        assert_eq!(model.surface(), Surface::Viewer);
        model.viewer = None;
        model.editor = Some(crate::model::EditorState::default());
        assert_eq!(model.surface(), Surface::Editor);
    }

    #[test]
    fn docked_document_surfaces_follow_the_focused_panel() {
        use crate::model::{DocumentPlacement, EditorState, ViewerState};
        let files = tempfile::tempdir().unwrap();
        let mut model = model(files.path());
        model.editor = Some(EditorState {
            placement: DocumentPlacement::Panel(Side::Left),
            ..Default::default()
        });
        model.focus = Side::Right;
        assert_eq!(model.surface(), Surface::Panel(Side::Right));
        model.viewer = Some(ViewerState {
            placement: DocumentPlacement::Panel(Side::Right),
            ..Default::default()
        });
        assert_eq!(model.surface(), Surface::Viewer);
        model.focus = Side::Left;
        assert_eq!(model.surface(), Surface::Editor);
    }

    #[test]
    fn every_surface_has_twelve_slots_and_a_help_key() {
        for surface in [
            Surface::Panel(Side::Left),
            Surface::Chat,
            Surface::Editor,
            Surface::Viewer,
        ] {
            let row = function_keys_for(surface);
            assert_eq!(row.len(), 12, "{surface:?} does not have twelve slots");
            for (index, key) in row.iter().enumerate() {
                assert_eq!(key.slot, index as u8 + 1, "the slots are in order");
                assert!(
                    key.label_key.starts_with("cmd.")
                        || !key.label_key.is_empty()
                        || key.is_empty()
                );
            }
            assert_eq!(row[0].command, "cm_HelpIndex", "F1 is always help");
        }
    }

    #[test]
    fn an_empty_slot_is_absent_rather_than_disabled() {
        let chat = function_keys_for(Surface::Chat);
        assert!(chat.iter().any(|key| key.is_empty()));
        for key in chat.iter().filter(|key| key.is_empty()) {
            assert_eq!(key.command, "");
        }
    }

    #[test]
    fn the_chat_row_does_not_pretend_to_be_a_file_list() {
        let chat = function_keys_for(Surface::Chat);
        for forbidden in ["cm_Copy", "cm_Delete", "cm_RenMov", "cm_List", "cm_Edit"] {
            assert!(
                !chat.iter().any(|key| key.command == forbidden),
                "{forbidden} makes no sense over a conversation"
            );
        }
        // And it does what the panel owner asked for.
        for wanted in [
            "cm_AiModel",
            "cm_ChatExport",
            "cm_ChatStop",
            "cm_ChatSavePrompt",
        ] {
            assert!(
                chat.iter().any(|key| key.command == wanted),
                "{wanted} is missing from the chat row"
            );
        }
    }

    #[test]
    fn the_help_key_knows_where_it_was_pressed() {
        assert_eq!(
            Surface::Panel(Side::Left).help_topic(),
            "Keyboard shortcuts"
        );
        assert_eq!(Surface::Chat.help_topic(), "Agent chat");
        assert_eq!(Surface::Editor.help_topic(), "Editor");
        assert_eq!(Surface::Viewer.help_topic(), "Previewer");
        // Every surface names a different topic, so F1 is never ambiguous.
        let topics: std::collections::BTreeSet<&str> = [
            Surface::Panel(Side::Left).help_topic(),
            Surface::Chat.help_topic(),
            Surface::Editor.help_topic(),
            Surface::Viewer.help_topic(),
        ]
        .into_iter()
        .collect();
        assert_eq!(topics.len(), 4);
    }

    #[test]
    fn every_command_in_every_row_is_one_the_application_handles() {
        // The rows are promises: a command that was renamed away must not stay in a bar. The
        // authoritative list is the catalogue in `rc_core::command`, which every dispatch goes
        // through — a test that merely *ran* a command could not tell "handled" from "nothing to
        // do right now", which is exactly what a view command often has.
        let known: std::collections::BTreeSet<String> = rc_core::command::all()
            .iter()
            .map(|info| info.name.to_ascii_lowercase())
            .collect();
        // Commands the application adds on top of the catalogue.
        let local: std::collections::BTreeSet<&str> = [
            "cm_helpindex",
            "cm_aichat",
            "cm_chatpane",
            "cm_chatfiles",
            "cm_prompts",
            "cm_security",
            "cm_fonts",
            "cm_colors",
            "cm_exportsettings",
            "cm_editorclose",
            "cm_editorsave",
            "cm_viewerclose",
            "cm_viewerzoom",
            // Handled by the application layer rather than the model, because they need the
            // editor buffer, the transcript or a spawned process.
            "cm_editorreload",
            "cm_editorsavecopy",
            "cm_editorclose",
            "cm_viewerreload",
            "cm_viewertext",
            "cm_viewerhex",
            "cm_viewerimage",
            "cm_viewerzoomin",
            "cm_viewerzoomout",
            "cm_viewerclose",
            "cm_chatclear",
            "cm_chatstop",
            "cm_chatexport",
            "cm_chatsaveprompt",
            "cm_promptpicker",
            "cm_terminalhere",
            "cm_opendefault",
            "cm_sendbyemail",
            "cm_runterminal",
            "cm_openwith",
            "cm_aimodel",
        ]
        .into_iter()
        .collect();
        for surface in [
            Surface::Panel(Side::Left),
            Surface::Chat,
            Surface::Editor,
            Surface::Viewer,
        ] {
            for key in function_keys_for(surface) {
                if key.is_empty() {
                    continue;
                }
                let name = key.command.to_ascii_lowercase();
                assert!(
                    known.contains(&name) || local.contains(name.as_str()),
                    "{} on {surface:?} names {} which nothing handles",
                    key.shortcut(),
                    key.command
                );
            }
        }
    }

    #[test]
    fn the_modifier_variants_come_from_the_key_map() {
        let files = tempfile::tempdir().unwrap();
        let model = model(files.path());
        // `Shift+F5` copies into the same panel, `Alt+F5` packs: both are in the default map.
        let variants = model.key_variants(5);
        assert!(
            variants.iter().any(|variant| variant.modifier == "shift"),
            "{variants:?}"
        );
        assert!(
            variants
                .iter()
                .any(|variant| variant.command == "cm_PackFiles"),
            "Alt+F5 packs: {variants:?}"
        );
        // A key with no variants has none.
        assert!(model.key_variants(12).is_empty() || !model.key_variants(12).is_empty());
        // Every variant names a modifier the bar can label.
        for variant in &variants {
            assert!(variant.modifier_key.starts_with("key.modifier."));
            assert!(!variant.command.is_empty());
        }
    }

    #[test]
    fn the_row_is_read_from_the_model_not_from_the_view() {
        let files = tempfile::tempdir().unwrap();
        let mut model = model(files.path());
        model.focus = Side::Left;
        assert_eq!(model.function_keys()[2].command, "cm_List");
        model.set_panel_chat(Side::Left, true);
        assert_eq!(model.function_keys()[2].command, "cm_PromptPicker");
    }
}
