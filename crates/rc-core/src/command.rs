//! Catalogue of internal commands.
//!
//! The catalogue is a static, platform-neutral list of the command identifiers
//! known to the classic commander interface (`cm_*` names plus numeric ids).
//! The UI maps each command to a localised caption through the language packs;
//! nothing in this module is UI text.

/// One internal command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandInfo {
    /// Identifier usable in button bars, aliases and the command line.
    pub name: &'static str,
    /// Numeric command id (used by legacy button bars).
    pub id: u32,
    /// Functional group, e.g. `file_operations`.
    pub section: &'static str,
}

static RAW: &str = include_str!("../data/commands.tsv");

fn parse_raw() -> &'static [CommandInfo] {
    use std::sync::OnceLock;
    static PARSED: OnceLock<Vec<CommandInfo>> = OnceLock::new();
    PARSED.get_or_init(|| {
        RAW.lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| {
                let mut it = l.split('\t');
                let name = it.next()?;
                let id = it.next()?.parse().ok()?;
                let section = it.next()?.trim();
                Some(CommandInfo {
                    name: Box::leak(name.to_string().into_boxed_str()),
                    id,
                    section: Box::leak(section.to_string().into_boxed_str()),
                })
            })
            .collect()
    })
}

/// Every known command, sorted by section then id.
pub fn all() -> &'static [CommandInfo] {
    parse_raw()
}

/// Total number of catalogue entries.
pub fn count() -> usize {
    all().len()
}

/// Look a command up by its identifier, case-insensitively.
pub fn by_name(name: &str) -> Option<&'static CommandInfo> {
    let lower = name.trim().to_ascii_lowercase();
    all().iter().find(|c| c.name.eq_ignore_ascii_case(&lower))
}

/// Look a command up by its numeric id.
pub fn by_id(id: u32) -> Option<&'static CommandInfo> {
    all().iter().find(|c| c.id == id)
}

/// Resolve either a `cm_Name` identifier or a numeric id.
pub fn resolve(spec: &str) -> Option<&'static CommandInfo> {
    let spec = spec.trim();
    if let Ok(id) = spec.parse::<u32>() {
        return by_id(id);
    }
    by_name(spec)
}

/// All commands of one section.
pub fn in_section(section: &str) -> Vec<&'static CommandInfo> {
    all().iter().filter(|c| c.section == section).collect()
}

/// Distinct section names in catalogue order.
pub fn sections() -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for c in all() {
        if !out.contains(&c.section) {
            out.push(c.section);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_is_populated_and_stable() {
        assert!(count() > 500, "catalogue has {} entries", count());
        assert_eq!(count(), all().len());
    }

    #[test]
    fn known_commands_resolve_by_name_and_id() {
        let c = by_name("cm_SrcByName").expect("cm_SrcByName exists");
        assert_eq!(c.id, 321);
        assert_eq!(c.section, "source");
        assert_eq!(
            by_name("CM_SRCBYNAME").map(|c| c.name),
            Some("cm_SrcByName")
        );
        assert_eq!(by_id(321).map(|c| c.name), Some("cm_SrcByName"));
        assert_eq!(resolve("321").map(|c| c.name), Some("cm_SrcByName"));
        assert_eq!(resolve("cm_srcbyname").map(|c| c.id), Some(321));
    }

    #[test]
    fn unknown_commands_return_none() {
        assert!(by_name("cm_DoesNotExist").is_none());
        assert!(by_id(999_999).is_none());
        assert!(resolve("nonsense").is_none());
    }

    #[test]
    fn sections_group_commands() {
        let secs = sections();
        assert!(secs.contains(&"file_operations"));
        assert!(secs.contains(&"ftp"));
        assert!(!in_section("file_operations").is_empty());
        assert!(in_section("no_such_section").is_empty());
    }
}
