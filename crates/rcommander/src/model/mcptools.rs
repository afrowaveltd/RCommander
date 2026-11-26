//! Remembering what the connected MCP servers publish (Phase 16.37).
//!
//! The panel connects to a server and lists its tools; this module is the memory behind that list:
//! it stores a catalogue in the settings (so the answers survive a restart), estimates each tool's
//! danger, and applies the user's three-state answer.

use super::Model;
use rc_core::mcptools::{Danger, ServerRecord, ToolRecord, ToolTrust};

impl Model {
    /// The remembered catalogue for one server, if it has been connected before.
    pub fn mcp_catalogue(&self, server: &str) -> Option<&ServerRecord> {
        self.settings
            .agent
            .mcp_servers
            .iter()
            .find(|record| record.server == server)
    }

    /// The tools the model may be offered from a server: everything not denied.
    pub fn mcp_available_tools(&self, server: &str) -> Vec<ToolRecord> {
        self.mcp_catalogue(server)
            .map(|record| record.available().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    /// True when the catalogue allows the model to call this tool without asking.
    pub fn mcp_tool_allowed(&self, server: &str, tool: &str) -> bool {
        self.mcp_catalogue(server)
            .and_then(|record| record.tools.iter().find(|entry| entry.name == tool))
            .map(|entry| entry.trust == ToolTrust::Allow)
            .unwrap_or(false)
    }

    /// True when a tool the model wants to call needs the approval dialog.
    ///
    /// A tool nobody answered for asks. That is the point of the catalogue: a server that adds tools
    /// in an upgrade cannot widen what the model can do without a human clicking.
    pub fn mcp_tool_asks(&self, server: &str, tool: &str) -> bool {
        self.mcp_catalogue(server)
            .and_then(|record| record.tools.iter().find(|entry| entry.name == tool))
            .map(|entry| entry.trust.asks())
            .unwrap_or(true)
    }

    /// Remember what a server publishes, keeping the answers that are still deserved.
    ///
    /// Called every time the agent panel connects, so the list is current without asking the user
    /// again for tools whose nature has not changed.
    pub fn remember_mcp_tools(
        &mut self,
        server: &str,
        tools: Vec<(String, String, String)>,
    ) -> usize {
        let mut record = self
            .mcp_catalogue(server)
            .cloned()
            .unwrap_or_else(|| ServerRecord {
                server: server.to_string(),
                tools: Vec::new(),
            });
        record.merge(tools);
        let count = record.tools.len();
        self.settings
            .agent
            .mcp_servers
            .retain(|existing| existing.server != server);
        self.settings.agent.mcp_servers.push(record);
        self.settings_dirty = true;
        count
    }

    /// Set one tool's answer, with the confirmation word when the tool is dangerous.
    pub fn set_mcp_tool_trust(
        &mut self,
        server: &str,
        tool: &str,
        trust: ToolTrust,
        typed: &str,
    ) -> Result<ToolTrust, String> {
        let Some(position) = self
            .settings
            .agent
            .mcp_servers
            .iter()
            .position(|record| record.server == server)
        else {
            return Err(self.t("mcp.unknown_server").to_string());
        };
        let previous = self.settings.agent.mcp_servers[position].set_trust(tool, trust, typed)?;
        self.settings_dirty = true;
        // Allowing a destructive tool is a decision with consequences, so it goes to the audit log
        // as well as the settings file.
        if trust == ToolTrust::Allow {
            self.audit.record(
                &format!("mcp:{server}"),
                "policy",
                tool,
                "",
                rc_core::audit::Verdict::Approved,
                "the user allowed this MCP tool",
            );
        }
        Ok(previous)
    }

    /// The danger the catalogue estimated for a tool.
    pub fn mcp_tool_danger(&self, server: &str, tool: &str) -> Option<Danger> {
        self.mcp_catalogue(server)
            .and_then(|record| record.tools.iter().find(|entry| entry.name == tool))
            .map(|entry| entry.danger)
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Model;
    use rc_core::config::Settings;
    use rc_core::location::Location;
    use rc_core::mcptools::{Danger, ToolTrust};

    fn model(dir: &std::path::Path) -> Model {
        let mut model = Model::for_test(
            Settings::default(),
            Location::local(dir),
            Location::local(dir),
        );
        model.config_dir_override = Some(dir.to_path_buf());
        model
    }

    fn tools() -> Vec<(String, String, String)> {
        vec![
            (
                "create_icon".to_string(),
                "draw an icon".to_string(),
                "{}".to_string(),
            ),
            (
                "delete_document".to_string(),
                String::new(),
                "{}".to_string(),
            ),
            ("get_document".to_string(), String::new(), "{}".to_string()),
        ]
    }

    #[test]
    fn connecting_remembers_the_tools_and_every_one_starts_asking() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        assert!(model.mcp_catalogue("photo-craft").is_none());
        assert_eq!(model.remember_mcp_tools("photo-craft", tools()), 3);
        let record = model.mcp_catalogue("photo-craft").cloned().expect("stored");
        assert_eq!(record.tools.len(), 3);
        assert!(record.untouched(), "nothing was answered yet");
        // Ask is the default for every tool, including the dangerous one.
        assert!(model.mcp_tool_asks("photo-craft", "delete_document"));
        assert!(!model.mcp_tool_allowed("photo-craft", "create_icon"));
        // And an unknown server is even more careful: everything asks.
        assert!(model.mcp_tool_asks("nobody", "anything"));
        assert!(model.settings_dirty, "the catalogue is worth saving");
    }

    #[test]
    fn the_danger_estimate_reaches_the_model_and_the_audit_log() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.remember_mcp_tools("photo-craft", tools());
        assert_eq!(
            model.mcp_tool_danger("photo-craft", "delete_document"),
            Some(Danger::Destructive)
        );
        assert_eq!(
            model.mcp_tool_danger("photo-craft", "get_document"),
            Some(Danger::ReadOnly)
        );
        // Allowing a destructive tool needs the word...
        assert!(
            model
                .set_mcp_tool_trust("photo-craft", "delete_document", ToolTrust::Allow, "")
                .is_err()
        );
        // ...and with it, both the settings and the audit know.
        assert!(
            model
                .set_mcp_tool_trust("photo-craft", "delete_document", ToolTrust::Allow, "yes")
                .is_ok()
        );
        assert!(model.mcp_tool_allowed("photo-craft", "delete_document"));
        let events = model.audit().read();
        let entry = events
            .iter()
            .rev()
            .find(|event| event.target == "delete_document")
            .expect("an audit entry for the tool");
        // Allowing is a decision the user made, so it is recorded as approved rather than allowed.
        assert_eq!(entry.verdict, rc_core::audit::Verdict::Approved);
        assert_eq!(entry.operation, "policy");
        assert!(entry.origin.contains("photo-craft"), "{}", entry.origin);
    }

    #[test]
    fn a_denied_tool_is_not_offered_at_all() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.remember_mcp_tools("mail-craft", tools());
        assert_eq!(model.mcp_available_tools("mail-craft").len(), 3);
        model
            .set_mcp_tool_trust("mail-craft", "delete_document", ToolTrust::Deny, "")
            .expect("denying needs no ceremony");
        let available = model.mcp_available_tools("mail-craft");
        assert_eq!(available.len(), 2);
        assert!(
            !available.iter().any(|tool| tool.name == "delete_document"),
            "{available:?}"
        );
    }

    #[test]
    fn reconnecting_keeps_the_answers_and_a_new_tool_starts_asking() {
        let dir = tempfile::tempdir().unwrap();
        let mut model = model(dir.path());
        model.remember_mcp_tools("photo-craft", tools());
        model
            .set_mcp_tool_trust("photo-craft", "create_icon", ToolTrust::Allow, "")
            .unwrap();
        // The same server comes back with the same tools plus one more.
        let mut more = tools();
        more.push(("export_png".to_string(), String::new(), "{}".to_string()));
        model.remember_mcp_tools("photo-craft", more);
        assert!(
            model.mcp_tool_allowed("photo-craft", "create_icon"),
            "an unchanged tool keeps its answer"
        );
        assert!(
            model.mcp_tool_asks("photo-craft", "export_png"),
            "a new tool starts by asking"
        );
    }
}
