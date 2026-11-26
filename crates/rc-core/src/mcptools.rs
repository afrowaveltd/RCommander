//! What we remember about another application's MCP tools (Phase 16.37).
//!
//! When a server connects, RCommander reads what it publishes and keeps the list — names, argument
//! schemas, and an estimate of how dangerous each tool is. The list is remembered so a second
//! connection shows the same answers instead of asking again, and every tool carries the user's
//! three-state answer: **allow**, **ask** (the default) or **deny**.
//!
//! Two rules give the catalogue its teeth:
//!
//! * **`ask` is the default.** A tool nobody has answered for is not silently available; it opens
//!   the approval dialog. A new version of a server that adds tools therefore cannot widen what the
//!   model can do without a human clicking.
//! * **A destructive tool cannot be switched to `allow` by a click.** It needs the same typed
//!   confirmation a permanent file pre-approval needs, because "allow this tool to delete things" is
//!   the same decision in a different costume.
//!
//! The danger estimate reads the **name and the schema**, not the description: a description is
//! written by the server's author and may say anything, while a parameter called `recursive` on a
//! tool called `delete_directory` is a fact. An unrecognised tool is `Unknown`, which is treated as
//! `ask` — never as safe.

use serde::{Deserialize, Serialize};

/// How dangerous a tool looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Danger {
    /// Reading, listing, searching: nothing is changed.
    ReadOnly,
    /// It changes something, but within what it names.
    Mutating,
    /// It can destroy or publish: deletes, sends, executes, pays.
    Destructive,
    /// We cannot tell. Treated exactly like `Destructive` when asking, never like `ReadOnly`.
    Unknown,
}

impl Danger {
    pub fn name(self) -> &'static str {
        match self {
            Danger::ReadOnly => "read_only",
            Danger::Mutating => "mutating",
            Danger::Destructive => "destructive",
            Danger::Unknown => "unknown",
        }
    }

    /// True when this tool must never be allowed without an explicit confirmation.
    pub fn needs_confirmation(self) -> bool {
        matches!(self, Danger::Destructive | Danger::Unknown)
    }
}

/// The user's answer for one tool.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolTrust {
    /// The model may call it; the call is still a policy request and still audited.
    Allow,
    /// The approval dialog appears each time. The default, in every case.
    #[default]
    Ask,
    /// The tool is not offered to the model at all.
    Deny,
}

impl ToolTrust {
    pub fn name(self) -> &'static str {
        match self {
            ToolTrust::Allow => "allow",
            ToolTrust::Ask => "ask",
            ToolTrust::Deny => "deny",
        }
    }

    pub fn parse(text: &str) -> ToolTrust {
        match text.trim().to_ascii_lowercase().as_str() {
            "allow" | "allowed" | "yes" => ToolTrust::Allow,
            "deny" | "denied" | "no" | "off" => ToolTrust::Deny,
            _ => ToolTrust::Ask,
        }
    }

    /// True when the model may call the tool at all.
    pub fn permits(self) -> bool {
        !matches!(self, ToolTrust::Deny)
    }

    /// True when the call needs the approval dialog.
    pub fn asks(self) -> bool {
        matches!(self, ToolTrust::Ask)
    }
}

/// One tool as a server publishes it, with what we remember about it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolRecord {
    /// Tool name as the server spells it.
    pub name: String,
    /// First line of the server's description, for the list. Never used to judge danger.
    #[serde(default)]
    pub description: String,
    /// Danger estimate from the name and the schema.
    pub danger: Danger,
    /// The user's answer.
    pub trust: ToolTrust,
    /// Why the estimate came out the way it did, so the dialog can explain itself.
    #[serde(default)]
    pub reason: String,
}

/// Everything remembered about one server.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerRecord {
    /// Name the server was configured under.
    pub server: String,
    /// Its tools, alphabetically.
    #[serde(default)]
    pub tools: Vec<ToolRecord>,
}

impl ServerRecord {
    /// True when every tool still has the default answer.
    pub fn untouched(&self) -> bool {
        self.tools.iter().all(|tool| tool.trust == ToolTrust::Ask)
    }

    /// Tools the model may be offered.
    pub fn available(&self) -> Vec<&ToolRecord> {
        self.tools
            .iter()
            .filter(|tool| tool.trust.permits())
            .collect()
    }

    /// Words that make a tool look dangerous, checked against its name.
    const DESTRUCTIVE_WORDS: [&'static str; 16] = [
        "delete", "remove", "rm", "drop", "truncate", "purge", "destroy", "format", "send", "post",
        "publish", "push", "upload", "exec", "run", "pay",
    ];

    /// Words that only change something, within what they name.
    const MUTATING_WORDS: [&'static str; 12] = [
        "write", "create", "update", "edit", "set", "move", "rename", "copy", "commit", "stage",
        "add", "insert",
    ];

    /// Words that read.
    const READ_WORDS: [&'static str; 10] = [
        "get", "list", "read", "search", "find", "query", "describe", "show", "inspect", "export",
    ];

    /// Estimate the danger of a tool from its name and the words in its argument schema.
    pub fn estimate(name: &str, schema_text: &str) -> (Danger, String) {
        let lower_name = name.to_ascii_lowercase();
        let lower_schema = schema_text.to_ascii_lowercase();
        // The name is the strongest signal; the schema can only raise the estimate, never lower it.
        for word in Self::DESTRUCTIVE_WORDS {
            if lower_name.contains(word) {
                return (Danger::Destructive, format!("its name contains “{word}”"));
            }
        }
        // A schema that mentions recursion or a whole-tree operation is a warning even when the
        // name is mild.
        for word in ["recursive", "all_users", "everything", "wildcard"] {
            if lower_schema.contains(word) {
                return (Danger::Mutating, format!("its arguments mention “{word}”"));
            }
        }
        for word in Self::MUTATING_WORDS {
            if lower_name.contains(word) {
                return (Danger::Mutating, format!("its name contains “{word}”"));
            }
        }
        for word in Self::READ_WORDS {
            if lower_name.contains(word) {
                return (Danger::ReadOnly, format!("its name contains “{word}”"));
            }
        }
        (
            Danger::Unknown,
            "nothing about its name or arguments says what it does".to_string(),
        )
    }

    /// Rebuild the record from a fresh tool list, keeping the answers a tool still deserves.
    ///
    /// A tool whose **danger changed** loses its answer and goes back to `ask`: an upgrade that
    /// turned `read_file` into something destructive must not inherit the old permission. A tool
    /// that disappeared simply goes away; a new one starts at `ask`.
    pub fn merge(&mut self, tools: Vec<(String, String, String)>) {
        let mut merged: Vec<ToolRecord> = Vec::new();
        for (name, description, schema_text) in tools {
            let (danger, reason) = Self::estimate(&name, &schema_text);
            let previous = self.tools.iter().find(|tool| tool.name == name);
            let trust = match previous {
                Some(record) if record.danger == danger => record.trust,
                // Unknown danger keeps an answer only when the user explicitly allowed it: a rename
                // must not silently preserve a permission.
                _ => ToolTrust::Ask,
            };
            merged.push(ToolRecord {
                name,
                description,
                danger,
                trust,
                reason,
            });
        }
        merged.sort_by(|a, b| a.name.cmp(&b.name));
        self.tools = merged;
    }

    /// Set one tool's answer, refusing to allow a destructive tool without the confirmation word.
    ///
    /// Returns the previous answer so the caller can report what changed.
    pub fn set_trust(
        &mut self,
        tool: &str,
        trust: ToolTrust,
        typed_confirmation: &str,
    ) -> Result<ToolTrust, String> {
        let Some(record) = self.tools.iter_mut().find(|record| record.name == tool) else {
            return Err(format!("this server publishes no tool called “{tool}”"));
        };
        if trust == ToolTrust::Allow
            && record.danger.needs_confirmation()
            && !typed_confirmation.trim().eq_ignore_ascii_case("yes")
        {
            return Err(format!(
                "“{tool}” looks {} ({}); type the confirmation word to allow it",
                record.danger.name().replace('_', " "),
                record.reason
            ));
        }
        let previous = record.trust;
        record.trust = trust;
        Ok(previous)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(tools: &[(&str, &str)]) -> ServerRecord {
        let mut server = ServerRecord {
            server: "photo-craft".to_string(),
            tools: Vec::new(),
        };
        server.merge(
            tools
                .iter()
                .map(|(name, schema)| (name.to_string(), String::new(), schema.to_string()))
                .collect(),
        );
        server
    }

    #[test]
    fn the_three_answers_parse_and_mean_what_they_say() {
        assert_eq!(ToolTrust::parse("allow"), ToolTrust::Allow);
        assert_eq!(ToolTrust::parse("ALLOWED"), ToolTrust::Allow);
        assert_eq!(ToolTrust::parse("deny"), ToolTrust::Deny);
        assert_eq!(ToolTrust::parse("off"), ToolTrust::Deny);
        // Anything unrecognised is the safe middle: ask.
        assert_eq!(ToolTrust::parse("maybe"), ToolTrust::Ask);
        assert_eq!(ToolTrust::parse(""), ToolTrust::Ask);
        assert_eq!(ToolTrust::default(), ToolTrust::Ask);
        assert!(ToolTrust::Allow.permits());
        assert!(
            ToolTrust::Ask.permits(),
            "ask means offered, but questioned"
        );
        assert!(!ToolTrust::Deny.permits());
        assert!(ToolTrust::Ask.asks());
        assert!(!ToolTrust::Allow.asks());
    }

    #[test]
    fn danger_is_read_from_the_name_not_from_the_description() {
        // A description that claims safety changes nothing.
        let mut server = record(&[("delete_everything", "{}")]);
        server.tools[0].description = "a perfectly safe read-only helper".to_string();
        assert_eq!(server.tools[0].danger, Danger::Destructive);
        assert_eq!(ServerRecord::estimate("get_icon", "{}").0, Danger::ReadOnly);
        assert_eq!(
            ServerRecord::estimate("create_icon", "{}").0,
            Danger::Mutating
        );
        assert_eq!(
            ServerRecord::estimate("send_mail", "{}").0,
            Danger::Destructive
        );
        // A mild name with a dangerous argument is still a warning.
        assert_eq!(
            ServerRecord::estimate("cleanup", "{\"recursive\": true}").0,
            Danger::Mutating
        );
        // Nothing recognisable is Unknown, never ReadOnly.
        assert_eq!(
            ServerRecord::estimate("frobnicate", "{}").0,
            Danger::Unknown
        );
        assert!(Danger::Unknown.needs_confirmation());
    }

    #[test]
    fn every_tool_starts_asking() {
        let server = record(&[("get_icon", "{}"), ("create_icon", "{}")]);
        assert_eq!(server.tools.len(), 2);
        assert!(server.untouched());
        assert_eq!(server.available().len(), 2, "ask still offers the tool");
    }

    #[test]
    fn allowing_a_destructive_tool_needs_the_typed_word() {
        let mut server = record(&[("delete_file", "{}"), ("get_file", "{}")]);
        let error = server
            .set_trust("delete_file", ToolTrust::Allow, "")
            .unwrap_err();
        assert!(error.contains("delete_file"), "{error}");
        assert!(error.contains("confirmation"), "{error}");
        assert_eq!(server.tools[0].trust, ToolTrust::Ask, "nothing changed");
        // With the word it works, and the previous answer is reported.
        let previous = server
            .set_trust("delete_file", ToolTrust::Allow, "yes")
            .expect("the word allows it");
        assert_eq!(previous, ToolTrust::Ask);
        assert_eq!(server.tools[0].trust, ToolTrust::Allow);
        // A read-only tool does not need the word.
        server
            .set_trust("get_file", ToolTrust::Allow, "")
            .expect("reading needs no ceremony");
        // And denying never needs it either.
        server
            .set_trust("delete_file", ToolTrust::Deny, "")
            .expect("denying is always allowed");
        assert_eq!(
            server.available().len(),
            1,
            "the denied tool is not offered"
        );
    }

    #[test]
    fn a_renamed_or_changed_tool_does_not_inherit_a_permission() {
        let mut server = record(&[("read_file", "{}")]);
        server.set_trust("read_file", ToolTrust::Allow, "").unwrap();
        // The same name comes back with a schema that now looks dangerous.
        server.merge(vec![(
            "read_file".to_string(),
            String::new(),
            "{\"recursive\": true}".to_string(),
        )]);
        assert_eq!(server.tools[0].danger, Danger::Mutating);
        assert_eq!(
            server.tools[0].trust,
            ToolTrust::Ask,
            "a changed danger loses the old answer"
        );
        // A tool that keeps its danger keeps its answer.
        let mut server = record(&[("get_file", "{}")]);
        server.set_trust("get_file", ToolTrust::Allow, "").unwrap();
        server.merge(vec![(
            "get_file".to_string(),
            "new description".to_string(),
            "{}".to_string(),
        )]);
        assert_eq!(server.tools[0].trust, ToolTrust::Allow);
        // A tool that vanished is simply gone.
        server.merge(vec![(
            "other_tool".to_string(),
            String::new(),
            "{}".to_string(),
        )]);
        assert_eq!(server.tools.len(), 1);
        assert_eq!(server.tools[0].name, "other_tool");
        assert_eq!(server.tools[0].trust, ToolTrust::Ask);
    }

    #[test]
    fn asking_for_a_tool_the_server_does_not_publish_is_an_error() {
        let mut server = record(&[("get_file", "{}")]);
        let error = server
            .set_trust("not_a_tool", ToolTrust::Allow, "yes")
            .unwrap_err();
        assert!(error.contains("not_a_tool"), "{error}");
    }

    #[test]
    fn the_catalogue_round_trips_through_toml() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Eq, Debug, Default)]
        struct Wrap {
            #[serde(default)]
            servers: Vec<ServerRecord>,
        }
        let mut server = record(&[("create_icon", "{}"), ("delete_icon", "{}")]);
        server
            .set_trust("delete_icon", ToolTrust::Deny, "")
            .unwrap();
        let wrap = Wrap {
            servers: vec![server.clone()],
        };
        let text = toml::to_string_pretty(&wrap).unwrap();
        assert!(text.contains("deny"), "{text}");
        let back: Wrap = toml::from_str(&text).unwrap();
        assert_eq!(back.servers, vec![server]);
    }
}
