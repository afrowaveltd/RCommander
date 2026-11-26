//! The MCP client panel: connect to external MCP servers and run their tools.
//!
//! The MCP server half lives in [`crate::agent_host`]; this module is the
//! consuming side, built on `rc_agent::McpClient`.

use crate::model::Model;
use crate::theme::Palette;
use egui::{Context, RichText};
use rc_core::config::{McpServerConfig, Settings};

/// Turn the saved server list into client configurations.
///
/// Entries without a usable transport are skipped, so a broken entry in the
/// settings file cannot stop the panel from working.
pub fn configs_from_settings(settings: &Settings) -> Vec<rc_agent::McpClientConfig> {
    settings
        .agent
        .clients
        .iter()
        .filter_map(config_from_entry)
        .collect()
}

/// One saved entry → one client configuration.
pub fn config_from_entry(entry: &McpServerConfig) -> Option<rc_agent::McpClientConfig> {
    let name = if entry.name.trim().is_empty() {
        entry.command.clone()
    } else {
        entry.name.clone()
    };
    let transport = match entry.transport.trim().to_ascii_lowercase().as_str() {
        "stdio" | "" if !entry.command.trim().is_empty() => rc_agent::Transport::Stdio {
            command: entry.command.trim().to_string(),
            args: entry.args.clone(),
            env: entry.env.clone(),
        },
        "http" | "sse" if !entry.url.trim().is_empty() => rc_agent::Transport::Http {
            url: entry.url.trim().to_string(),
            token: None,
        },
        _ => return None,
    };
    Some(rc_agent::McpClientConfig {
        name,
        transport,
        ..Default::default()
    })
}

/// A connected MCP server.
pub struct ConnectedServer {
    pub name: String,
    pub client: rc_agent::McpClient,
    pub tools: Vec<rc_agent::ToolDescriptor>,
    pub selected_tool: usize,
    pub args: String,
    pub output: String,
    pub error: Option<String>,
    /// What the user typed to confirm allowing a dangerous tool.
    pub confirm_word: String,
}

/// Panel state.
#[derive(Default)]
pub struct AgentUi {
    pub servers: Vec<ConnectedServer>,
    pub pending: Vec<rc_agent::McpClientConfig>,
    pub error: Option<String>,
    pub loaded: bool,
}

impl AgentUi {
    /// Load the configured servers once (idempotent).
    pub fn ensure_loaded(&mut self, settings: &Settings) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.pending = configs_from_settings(settings);
    }

    /// Connect to the configured server with `index`.
    pub fn connect(&mut self, index: usize) -> Result<(), String> {
        let Some(config) = self.pending.get(index).cloned() else {
            return Err("no such server".to_string());
        };
        match rc_agent::McpClient::connect(&config) {
            Ok(client) => {
                let tools = client.tools();
                // Remember the catalogue the moment the server connects: the danger estimate and
                // the user's previous answers are what the tool list below is built from.
                let mut args = String::from("{}");
                if let Some(tool) = tools.first() {
                    args = "{}".to_string();
                    let _ = tool;
                }
                self.servers.push(ConnectedServer {
                    name: config.name.clone(),
                    client,
                    tools,
                    selected_tool: 0,
                    args,
                    output: String::new(),
                    error: None,
                    confirm_word: String::new(),
                });
                Ok(())
            }
            Err(e) => {
                let message = format!("{}: {e}", config.name);
                self.error = Some(message.clone());
                Err(message)
            }
        }
    }

    /// Run the selected tool of a connected server.
    pub fn call_tool(&mut self, index: usize) -> Result<(), String> {
        let Some(server) = self.servers.get_mut(index) else {
            return Err("no such server".to_string());
        };
        let Some(tool) = server.tools.get(server.selected_tool).cloned() else {
            return Err("no tool selected".to_string());
        };
        let args: serde_json::Value = serde_json::from_str(server.args.trim())
            .map_err(|e| format!("invalid JSON arguments: {e}"))?;
        match server.client.call_tool(&tool.name, args) {
            Ok(result) => {
                server.output = result.text;
                server.error = if result.is_error {
                    Some("the tool reported an error".to_string())
                } else {
                    None
                };
                Ok(())
            }
            Err(e) => {
                let message = e.to_string();
                server.error = Some(message.clone());
                Err(message)
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty() && self.servers.is_empty()
    }
}

/// Render the agent panel; returns true when the user closed it.
pub fn show(ctx: &Context, model: &mut Model, ui_state: &mut AgentUi, palette: &Palette) -> bool {
    ui_state.ensure_loaded(&model.settings.clone());
    let mut open = true;
    let mut action: Option<usize> = None;
    let mut call: Option<usize> = None;
    egui::Window::new(model.t("agent.panel"))
        .open(&mut open)
        .resizable(true)
        .default_size([680.0, 520.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(model.t("agent.tools"));
                ui.label(
                    RichText::new(format!(
                        "{} server(s) configured, {} connected",
                        ui_state.pending.len(),
                        ui_state.servers.len()
                    ))
                    .small()
                    .color(palette.hidden),
                );
            });
            ui.separator();
            ui.horizontal(|ui| {
                for (index, config) in ui_state.pending.iter().enumerate() {
                    if ui.button(config.name.clone()).clicked() {
                        action = Some(index);
                    }
                }
            });
            if let Some(error) = &ui_state.error {
                ui.label(RichText::new(error).color(palette.readonly));
            }
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (server_index, server) in ui_state.servers.iter_mut().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&server.name).strong());
                            ui.label(
                                RichText::new(format!("{} tool(s)", server.tools.len()))
                                    .small()
                                    .color(palette.hidden),
                            );
                        });
                        ui.horizontal(|ui| {
                            let current = server
                                .tools
                                .get(server.selected_tool)
                                .map(|t| t.name.clone())
                                .unwrap_or_default();
                            egui::ComboBox::from_id_salt(("tool", server_index))
                                .selected_text(current)
                                .show_ui(ui, |ui| {
                                    for (tool_index, tool) in server.tools.iter().enumerate() {
                                        if ui
                                            .selectable_label(
                                                server.selected_tool == tool_index,
                                                &tool.name,
                                            )
                                            .clicked()
                                        {
                                            server.selected_tool = tool_index;
                                        }
                                    }
                                });
                            if ui.button(model.t("agent.tools")).clicked() {
                                call = Some(server_index);
                            }
                        });
                        ui.add(
                            egui::TextEdit::multiline(&mut server.args)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .hint_text("JSON arguments"),
                        );
                        // What the model may call, decided per tool. The list comes from the
                        // remembered catalogue, so a second connection shows the same answers.
                        let server_name = server.name.clone();
                        let mut changes: Vec<(String, rc_core::mcptools::ToolTrust)> = Vec::new();
                        let mut typed: Option<(String, rc_core::mcptools::ToolTrust)> = None;
                        if let Some(catalogue) = model.mcp_catalogue(&server_name).cloned() {
                            egui::CollapsingHeader::new(model.tf(
                                "mcp.trust.header",
                                &[("count", &catalogue.tools.len().to_string())],
                            ))
                            .id_salt(("mcp_trust", server_index))
                            .show(ui, |ui| {
                                for tool in &catalogue.tools {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&tool.name).monospace());
                                        // The danger estimate is explained, not just coloured: the
                                        // reason is the whole argument for asking.
                                        let (colour, label) = match tool.danger {
                                            rc_core::mcptools::Danger::ReadOnly => {
                                                (palette.archive, model.t("mcp.danger.read"))
                                            }
                                            rc_core::mcptools::Danger::Mutating => {
                                                (palette.accent, model.t("mcp.danger.mutate"))
                                            }
                                            rc_core::mcptools::Danger::Destructive => {
                                                (palette.warning, model.t("mcp.danger.destroy"))
                                            }
                                            rc_core::mcptools::Danger::Unknown => {
                                                (palette.readonly, model.t("mcp.danger.unknown"))
                                            }
                                        };
                                        ui.label(RichText::new(label).small().color(colour))
                                            .on_hover_text(&tool.reason);
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                for trust in [
                                                    rc_core::mcptools::ToolTrust::Deny,
                                                    rc_core::mcptools::ToolTrust::Ask,
                                                    rc_core::mcptools::ToolTrust::Allow,
                                                ] {
                                                    let chosen = tool.trust == trust;
                                                    let label = match trust {
                                                        rc_core::mcptools::ToolTrust::Allow => {
                                                            model.t("mcp.trust.allow")
                                                        }
                                                        rc_core::mcptools::ToolTrust::Ask => {
                                                            model.t("mcp.trust.ask")
                                                        }
                                                        rc_core::mcptools::ToolTrust::Deny => {
                                                            model.t("mcp.trust.deny")
                                                        }
                                                    };
                                                    if ui.selectable_label(chosen, label).clicked()
                                                        && !chosen
                                                    {
                                                        changes.push((tool.name.clone(), trust));
                                                    }
                                                }
                                            },
                                        );
                                    });
                                }
                            });
                        }
                        for (tool, trust) in changes {
                            match model.set_mcp_tool_trust(&server_name, &tool, trust, "") {
                                Ok(_) => {}
                                // A dangerous tool needs a word rather than a click, so the panel
                                // asks for it instead of refusing silently.
                                Err(_) => typed = Some((tool, trust)),
                            }
                        }
                        if let Some((tool, trust)) = typed {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(
                                        model.tf("mcp.trust.confirm", &[("tool", &tool)]),
                                    )
                                    .small()
                                    .color(palette.warning),
                                );
                                ui.add(
                                    egui::TextEdit::singleline(&mut server.confirm_word)
                                        .desired_width(80.0)
                                        .hint_text("yes"),
                                );
                                if ui.button(model.t("mcp.trust.confirm.ok")).clicked() {
                                    let word = server.confirm_word.clone();
                                    match model.set_mcp_tool_trust(
                                        &server_name,
                                        &tool,
                                        trust,
                                        &word,
                                    ) {
                                        Ok(_) => {
                                            server.error = None;
                                            server.confirm_word.clear();
                                        }
                                        Err(error) => server.error = Some(error),
                                    }
                                }
                            });
                        }
                        if let Some(error) = &server.error {
                            ui.label(RichText::new(error).color(palette.readonly));
                        }
                        if !server.output.is_empty() {
                            egui::Frame::new()
                                .fill(palette.header_background)
                                .inner_margin(egui::Margin::same(4))
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(RichText::new(&server.output).monospace())
                                            .selectable(true),
                                    );
                                });
                        }
                        ui.separator();
                    }
                    if ui_state.servers.is_empty() {
                        ui.label(model.t("agent.panel"));
                        ui.label(
                            RichText::new(
                                "Add MCP servers under [agent] clients in settings.toml.",
                            )
                            .small()
                            .color(palette.hidden),
                        );
                    }
                });
        });
    if let Some(index) = action
        && let Err(error) = ui_state.connect(index)
    {
        model.report_error(error);
    }
    if let Some(index) = call
        && let Err(error) = ui_state.call_tool(index)
    {
        model.report_error(error);
    }
    !open
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn settings_with(entries: Vec<McpServerConfig>) -> Settings {
        let mut settings = Settings::default();
        settings.agent.clients = entries;
        settings
    }

    #[test]
    fn stdio_entries_become_stdio_transports() {
        let mut env = BTreeMap::new();
        env.insert("TOKEN".to_string(), "abc".to_string());
        let settings = settings_with(vec![McpServerConfig {
            name: "filesystem".into(),
            transport: "stdio".into(),
            command: "npx".into(),
            args: vec!["-y".into(), "server".into()],
            url: String::new(),
            env,
        }]);
        let configs = configs_from_settings(&settings);
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].name, "filesystem");
        match &configs[0].transport {
            rc_agent::Transport::Stdio { command, args, env } => {
                assert_eq!(command, "npx");
                assert_eq!(args, &vec!["-y".to_string(), "server".to_string()]);
                assert_eq!(env.get("TOKEN").map(|s| s.as_str()), Some("abc"));
            }
            other => panic!("expected stdio transport, got {other:?}"),
        }
    }

    #[test]
    fn http_entries_become_http_transports() {
        let settings = settings_with(vec![McpServerConfig {
            name: "remote".into(),
            transport: "http".into(),
            command: String::new(),
            args: Vec::new(),
            url: "http://127.0.0.1:9000".into(),
            env: BTreeMap::new(),
        }]);
        let configs = configs_from_settings(&settings);
        assert_eq!(configs.len(), 1);
        assert!(
            matches!(&configs[0].transport, rc_agent::Transport::Http { url, .. } if url == "http://127.0.0.1:9000")
        );
    }

    #[test]
    fn unusable_entries_are_skipped() {
        let settings = settings_with(vec![
            McpServerConfig {
                name: "no command".into(),
                transport: "stdio".into(),
                command: String::new(),
                ..Default::default()
            },
            McpServerConfig {
                name: "no url".into(),
                transport: "http".into(),
                ..Default::default()
            },
            McpServerConfig {
                name: "weird".into(),
                transport: "carrier-pigeon".into(),
                command: "x".into(),
                ..Default::default()
            },
        ]);
        assert!(configs_from_settings(&settings).is_empty());
    }

    #[test]
    fn an_entry_without_a_name_uses_the_command() {
        let settings = settings_with(vec![McpServerConfig {
            name: "  ".into(),
            transport: "stdio".into(),
            command: "my-server".into(),
            ..Default::default()
        }]);
        let configs = configs_from_settings(&settings);
        assert_eq!(configs[0].name, "my-server");
    }

    #[test]
    fn the_empty_transport_defaults_to_stdio() {
        let entry = McpServerConfig {
            name: "x".into(),
            transport: String::new(),
            command: "srv".into(),
            ..Default::default()
        };
        assert!(matches!(
            config_from_entry(&entry).map(|c| c.transport),
            Some(rc_agent::Transport::Stdio { .. })
        ));
    }

    #[test]
    fn connecting_to_an_unknown_server_reports_an_error() {
        let mut ui_state = AgentUi::default();
        assert!(ui_state.connect(0).is_err());
        assert!(ui_state.call_tool(0).is_err());
    }

    #[test]
    fn loading_is_idempotent_and_reports_the_server_list() {
        let settings = settings_with(vec![McpServerConfig {
            name: "a".into(),
            transport: "stdio".into(),
            command: "cmd".into(),
            ..Default::default()
        }]);
        let mut ui_state = AgentUi::default();
        assert!(ui_state.is_empty());
        ui_state.ensure_loaded(&settings);
        assert_eq!(ui_state.pending.len(), 1);
        // A second call must not duplicate the list.
        ui_state.ensure_loaded(&settings);
        assert_eq!(ui_state.pending.len(), 1);
    }
}
