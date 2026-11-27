//! Implemented actions shared by the menu, toolbar and future frontends.
use super::Model;
use rc_core::config::BarButton;

/// Localized menu groups and implemented command/label pairs.
pub const ACTION_GROUPS: &[(&str, &[(&str, &str)])] = &[
    ("menu.file", &[
        ("cm_List", "cmd.view"),
        ("cm_Edit", "cmd.edit"),
        ("cm_EditNewFile", "cmd.new_file"),
        ("cm_Copy", "cmd.copy"),
        ("cm_RenMov", "cmd.move"),
        ("cm_MkDir", "cmd.mkdir"),
        ("cm_Delete", "cmd.delete"),
        ("cm_RenameOnly", "cmd.rename"),
        ("cm_PackFiles", "cmd.pack"),
        ("cm_UnpackFiles", "cmd.unpack"),
        ("cm_TestArchive", "cmd.test_archive"),
        ("cm_Split", "cmd.split_file"),
        ("cm_Combine", "cmd.combine_files"),
        ("cm_Encode", "cmd.encode"),
        ("cm_Decode", "cmd.decode"),
        ("cm_SetAttrib", "cmd.attributes"),
        ("cm_VersionInfo", "cmd.properties"),
        ("cm_Exit", "cmd.quit"),
    ]),
    ("menu.mark", &[
        ("cm_SelectAll", "cmd.select_all"),
        ("cm_Clearall", "cmd.deselect_all"),
        ("cm_ExchangeSelection", "cmd.invert_selection"),
        ("cm_SaveSelectionToFile", "cmd.save_selection"),
        ("cm_LoadSelectionFromFile", "cmd.restore_selection"),
    ]),
    ("menu.commands", &[
        ("cm_SearchFor", "cmd.search"),
        ("cm_FindDuplicates", "cmd.duplicates"),
        ("cm_CompareDirs", "cmd.compare_dirs"),
        ("cm_CompareFilesByContent", "cmd.compare_by_content"),
        ("cm_SynchronizeDirs", "cmd.sync_dirs"),
        ("cm_MultiRename", "cmd.multi_rename"),
        ("cm_CRCcreate", "cmd.checksums"),
        ("cm_CRCcheck", "cmd.verify_checksums"),
        ("cm_DirectoryHotlist", "cmd.hotlist"),
        ("cm_DirectoryHistory", "cmd.history"),
        ("cm_OpenTransferManager", "dialog.copy.background"),
        ("cm_GetFileSpace", "cmd.calculate_size"),
        ("cm_TerminalHere", "cmd.terminal_here"),
    ]),
    ("menu.show", &[
        ("cm_RereadSource", "cmd.refresh"),
        ("cm_SrcShort", "cmd.view_short"),
        ("cm_SrcLong", "cmd.view_long"),
        ("cm_SrcThumbs", "toolbar.thumbnails"),
        ("cm_SrcTree", "cmd.tree"),
        ("cm_SrcQuickView", "toolbar.quick_view"),
        ("cm_CustomColumnViewMenu", "cmd.custom_columns"),
        ("cm_NewTab", "cmd.new_tab"),
        ("cm_CloseTab", "cmd.close_tab"),
        ("cm_SrcByName", "col.name"),
        ("cm_SrcByExt", "cmd.sort_ext"),
        ("cm_SrcBySize", "col.size"),
        ("cm_SrcByDateTime", "col.date"),
        ("cm_SrcUnsorted", "cmd.sort_unsorted"),
        ("cm_BranchView", "cmd.branch_view"),
        ("cm_SrcAllFiles", "cmd.all_files"),
    ]),
    ("menu.agent", &[
        ("cm_ChatPane", "agent.pane.menu"),
        ("cm_AiChat", "ai.chat.title"),
        ("cm_ChatFiles", "agent.pane.files"),
        ("cm_AiModel", "cmd.aimodel"),
        ("cm_Prompts", "prompt.page.manage"),
        ("cm_PromptPicker", "prompt.pick"),
        ("cm_ProcessWithAgent", "toolbar.process_agent"),
        ("cm_ReviewWorkspace", "toolbar.review_workspace"),
        ("cm_OpenWorkspace", "toolbar.open_workspace"),
        ("cm_DiscardWorkspace", "toolbar.discard_workspace"),
        ("cm_McpTools", "toolbar.mcp_tools"),
    ]),
    ("menu.git", &[
        ("cm_GitPanel", "cmd.gitpanel"),
        ("cm_GitClone", "cmd.gitclone"),
        ("cm_GitBranches", "cmd.gitbranches"),
        ("cm_GitRemote", "cmd.gitremote"),
        ("cm_GitCopyOut", "cmd.gitcopyout"),
        ("cm_GitStage", "cmd.gitstage"),
        ("cm_GitUnstage", "cmd.gitunstage"),
        ("cm_GitCommit", "cmd.gitcommit"),
    ]),
    ("menu.net", &[
        ("cm_FtpConnect", "ftp.connect"),
        ("cm_FtpDisconnect", "ftp.disconnect"),
    ]),
    ("menu.configuration", &[
        ("cm_Config", "cmd.config"),
        ("cm_ButtonConfig", "cmd.button_bar"),
        ("cm_Plugins", "dialog.config.plugins"),
        ("cm_ReloadPlugins", "toolbar.reload_plugins"),
        ("cm_Colors", "dialog.config.colors"),
        ("cm_Fonts", "dialog.config.fonts"),
        ("cm_Security", "dialog.config.security"),
        ("cm_Dangerous", "dialog.config.dangerous"),
        ("cm_Secrets", "cmd.secrets"),
        ("cm_GitConfig", "cmd.gitconfig"),
        ("cm_ThemeToggle", "toolbar.theme_toggle"),
        ("cm_ThemeNext", "cmd.themenext"),
        ("cm_SaveSettings", "toolbar.save_settings"),
    ]),
    ("menu.help", &[
        ("cm_HelpIndex", "cmd.help"),
        ("cm_About", "cmd.about"),
    ]),
];

const DEFAULT_CAPTIONS: &[(&str, &str, &str)] = &[
    ("Brief", "cm_SrcShort", "toolbar.brief"),
    ("Full", "cm_SrcLong", "toolbar.full"),
    ("Search", "cm_SearchFor", "toolbar.search"),
    ("Pack", "cm_PackFiles", "cmd.pack"),
    ("Unpack", "cm_UnpackFiles", "cmd.unpack"),
    ("Multi-rename", "cm_MultiRenameFiles", "cmd.multi_rename"),
    ("Compare", "cm_CompareDirs", "toolbar.compare"),
    ("Synchronize", "cm_FileSync", "toolbar.sync"),
    ("Hotlist", "cm_DirectoryHotlist", "toolbar.hotlist"),
    ("Size", "cm_GetFileSpace", "toolbar.size"),
    ("Options", "cm_Config", "cmd.config"),
];

/// Localize factory captions and automatically labelled shortcuts without
/// modifying stored user captions or migrating settings destructively.
pub fn button_caption(model: &Model, button: &BarButton) -> String {
    for &(caption, command, key) in DEFAULT_CAPTIONS {
        if button.caption == caption && button.command.eq_ignore_ascii_case(command) {
            let translated = model.t(key);
            return if translated == key { caption.to_owned() } else { translated };
        }
    }
    if button.caption.trim().is_empty() {
        for &(_, actions) in ACTION_GROUPS {
            for &(command, key) in actions {
                if button.command.eq_ignore_ascii_case(command) {
                    return model.t(key);
                }
            }
        }
    }
    button.label()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_catalogue_has_unique_commands() {
        let mut seen = std::collections::HashSet::new();
        for &(_, actions) in ACTION_GROUPS {
            for &(command, key) in actions {
                assert!(seen.insert(command.to_ascii_lowercase()), "{command}");
                assert!(!key.is_empty());
            }
        }
    }
}
