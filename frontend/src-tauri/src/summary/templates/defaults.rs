/// Embedded default templates using compile-time inclusion
///
/// These templates are bundled into the binary and serve as fallbacks
/// when custom templates are not available.

/// Daily standup template for engineering/product teams
pub const DAILY_STANDUP: &str = include_str!("../../../templates/daily_standup.json");

/// Standard meeting notes template
pub const STANDARD_MEETING: &str = include_str!("../../../templates/standard_meeting.json");

/// ACT-format meeting conclusions template without tables
pub const THEO_MAU_ACT_NO_TABLE: &str =
    include_str!("../../../templates/theo_mau_act_no_table.json");

/// Project sync / progress update template
pub const PROJECT_SYNC: &str = include_str!("../../../templates/project_sync.json");

/// Sprint retrospective template
pub const RETROSPECTIVE: &str = include_str!("../../../templates/retrospective.json");

/// Sales & marketing client call template
pub const SALES_MARKETING_CLIENT_CALL: &str =
    include_str!("../../../templates/sales_marketing_client_call.json");

const BUILTIN_TEMPLATES: &[(&str, &str)] = &[
    ("daily_standup", DAILY_STANDUP),
    ("standard_meeting", STANDARD_MEETING),
    ("theo_mau_act_no_table", THEO_MAU_ACT_NO_TABLE),
    ("project_sync", PROJECT_SYNC),
    ("retrospective", RETROSPECTIVE),
    ("sales_marketing_client_call", SALES_MARKETING_CLIENT_CALL),
];

/// Registry of all built-in templates
///
/// Maps template identifiers to their embedded JSON content
pub fn get_builtin_templates() -> Vec<(&'static str, &'static str)> {
    BUILTIN_TEMPLATES.to_vec()
}

/// Get a built-in template by identifier
///
/// # Arguments
/// * `id` - Template identifier (e.g., "daily_standup", "standard_meeting")
///
/// # Returns
/// The template JSON content if found, None otherwise
pub fn get_builtin_template(id: &str) -> Option<&'static str> {
    BUILTIN_TEMPLATES
        .iter()
        .find_map(|(template_id, content)| (*template_id == id).then_some(*content))
}

/// List all built-in template identifiers
pub fn list_builtin_template_ids() -> Vec<&'static str> {
    BUILTIN_TEMPLATES
        .iter()
        .map(|(template_id, _)| *template_id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_templates_valid_json() {
        for (id, content) in get_builtin_templates() {
            let result = serde_json::from_str::<serde_json::Value>(content);
            assert!(
                result.is_ok(),
                "Built-in template '{}' contains invalid JSON: {:?}",
                id,
                result.err()
            );
        }
    }

    #[test]
    fn test_get_builtin_template() {
        assert!(get_builtin_template("daily_standup").is_some());
        assert!(get_builtin_template("standard_meeting").is_some());
        assert!(get_builtin_template("theo_mau_act_no_table").is_some());
        assert!(get_builtin_template("theo_mau_act").is_none());
        assert!(get_builtin_template("nonexistent").is_none());
    }
}
