//! The KiCad project file, `<name>.kicad_pro`: JSON that KiCad owns. Tron changes only
//! `erc.rule_severities` and writes every other key back as it read it. Keys come out
//! sorted, the way KiCad writes them.

use serde_json::{Map, Value, json};

use crate::erc::{ERC_RULES, RuleSeverities, RuleSeverity};

/// The `meta.version` of a project file Tron creates.
pub const PROJECT_FILE_VERSION: u64 = 1;
const SEVERITIES: &str = "/erc/rule_severities";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectFile {
    /// There is no `.kicad_pro`; one is written once a setting changes.
    Missing,
    /// The parsed file. Its root, and `erc` and `erc.rule_severities` where present, are
    /// objects.
    Json(Value),
    /// The file can't be read or isn't a KiCad project file, for the reason given. It is
    /// never written.
    Unreadable(String),
}

impl ProjectFile {
    pub fn parse(source: &str) -> ProjectFile {
        let value: Value = match serde_json::from_str(source) {
            Ok(value) => value,
            Err(error) => return ProjectFile::Unreadable(error.to_string()),
        };
        match validate(&value) {
            Ok(()) => ProjectFile::Json(value),
            Err(reason) => ProjectFile::Unreadable(reason.to_owned()),
        }
    }

    /// The setting of every rule Tron checks: the stored one, or the rule's default when
    /// it is missing or not one KiCad writes.
    pub fn rule_severities(&self) -> RuleSeverities {
        let stored = match self {
            ProjectFile::Json(value) => value.pointer(SEVERITIES).and_then(Value::as_object),
            _ => None,
        };
        ERC_RULES
            .iter()
            .map(|rule| {
                let default = RuleSeverity::from(rule.severity);
                let setting = stored
                    .and_then(|map| map.get(rule.key))
                    .and_then(Value::as_str)
                    .and_then(RuleSeverity::from_key)
                    .unwrap_or(default);
                (rule.key.to_owned(), setting)
            })
            .collect()
    }

    /// Errors with the reason when the file can't be changed.
    pub fn check_writable(&self) -> Result<(), String> {
        match self {
            ProjectFile::Unreadable(reason) => Err(reason.clone()),
            _ => Ok(()),
        }
    }

    /// This file with `severities` stored in `erc.rule_severities`. A new file lists every
    /// rule Tron checks and a `meta` block naming `file_name`.
    pub fn with_rule_severities(
        &self,
        file_name: &str,
        severities: &RuleSeverities,
    ) -> Result<ProjectFile, String> {
        let mut value = match self {
            ProjectFile::Unreadable(reason) => return Err(reason.clone()),
            ProjectFile::Json(value) => value.clone(),
            ProjectFile::Missing => {
                let mut all = self.rule_severities();
                for (key, setting) in severities {
                    all.insert(key.clone(), *setting);
                }
                let stored: Map<String, Value> = all
                    .into_iter()
                    .map(|(key, setting)| (key, Value::from(setting.key())))
                    .collect();
                return Ok(ProjectFile::Json(json!({
                    "erc": { "rule_severities": stored },
                    "meta": { "filename": file_name, "version": PROJECT_FILE_VERSION },
                })));
            }
        };
        let stored = value
            .as_object_mut()
            .and_then(|root| object_entry(root, "erc"))
            .and_then(|erc| object_entry(erc, "rule_severities"))
            .ok_or_else(|| "the file is not a KiCad project file".to_owned())?;
        for (key, setting) in severities {
            stored.insert(key.clone(), Value::from(setting.key()));
        }
        Ok(ProjectFile::Json(value))
    }

    /// The file's text with `meta.filename` set to `file_name`, as KiCad writes it: two-space
    /// indents and a final newline. `None` when there is nothing to write.
    pub fn to_file_string(&self, file_name: &str) -> Option<String> {
        let ProjectFile::Json(value) = self else {
            return None;
        };
        let mut value = value.clone();
        if let Some(meta) = value.get_mut("meta").and_then(Value::as_object_mut) {
            meta.insert("filename".to_owned(), Value::from(file_name));
        }
        let mut text = serde_json::to_string_pretty(&value).ok()?;
        text.push('\n');
        Some(text)
    }
}

fn validate(value: &Value) -> Result<(), &'static str> {
    let Some(root) = value.as_object() else {
        return Err("the file is not a JSON object");
    };
    match root.get("erc") {
        None => Ok(()),
        Some(Value::Object(erc)) => match erc.get("rule_severities") {
            None | Some(Value::Object(_)) => Ok(()),
            Some(_) => Err("erc.rule_severities is not an object"),
        },
        Some(_) => Err("erc is not an object"),
    }
}

/// The object under `key`, created when missing. `None` when `key` holds something else.
fn object_entry<'a>(
    map: &'a mut Map<String, Value>,
    key: &str,
) -> Option<&'a mut Map<String, Value>> {
    map.entry(key)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn severities(pairs: &[(&str, RuleSeverity)]) -> RuleSeverities {
        pairs
            .iter()
            .map(|(key, setting)| ((*key).to_owned(), *setting))
            .collect()
    }

    #[test]
    fn reads_stored_settings_and_defaults_the_rest() {
        let file = ProjectFile::parse(
            r#"{"erc": {"rule_severities": {
                "pin_not_connected": "warning",
                "pin_to_pin": "ignore",
                "label_dangling": "loud",
                "simulation_model_issue": "ignore"
            }}}"#,
        );
        let settings = file.rule_severities();
        assert_eq!(settings.len(), ERC_RULES.len());
        assert_eq!(settings["pin_not_connected"], RuleSeverity::Warning);
        assert_eq!(settings["pin_to_pin"], RuleSeverity::Ignore);
        assert_eq!(settings["label_dangling"], RuleSeverity::Error);
        assert_eq!(settings["isolated_pin_label"], RuleSeverity::Warning);
        assert!(!settings.contains_key("simulation_model_issue"));

        let mut defaults = settings.clone();
        defaults.insert("pin_not_connected".to_owned(), RuleSeverity::Error);
        defaults.insert("pin_to_pin".to_owned(), RuleSeverity::Warning);
        assert_eq!(ProjectFile::Missing.rule_severities(), defaults);
    }

    #[test]
    fn rejects_files_that_are_not_project_files() {
        let sources = [
            "not json",
            "[1, 2]",
            r#"{"erc": 3}"#,
            r#"{"erc": {"rule_severities": []}}"#,
        ];
        for source in sources {
            let file = ProjectFile::parse(source);
            assert!(matches!(file, ProjectFile::Unreadable(_)), "{source}");
            assert!(file.check_writable().is_err());
            let edit = severities(&[("pin_to_pin", RuleSeverity::Ignore)]);
            assert!(file.with_rule_severities("a.kicad_pro", &edit).is_err());
            assert_eq!(file.to_file_string("a.kicad_pro"), None);
        }
    }

    #[test]
    fn changes_only_the_given_settings_and_keeps_every_other_key() {
        let source = r#"{
  "board": {"design_settings": {"defaults": {"line_width": 0.15}}},
  "erc": {
    "erc_exclusions": [["pin_not_connected|1|2|a|b", "kept"]],
    "meta": {"version": 0},
    "pin_map": [[0, 1], [1, 2]],
    "rule_severities": {"pin_not_connected": "error", "simulation_model_issue": "ignore"}
  },
  "meta": {"filename": "old.kicad_pro", "version": 1},
  "text_variables": {}
}"#;
        let file = ProjectFile::parse(source);
        let edit = severities(&[("pin_not_connected", RuleSeverity::Warning)]);
        let edited = file.with_rule_severities("amp.kicad_pro", &edit).unwrap();
        let text = edited.to_file_string("amp.kicad_pro").unwrap();
        assert!(text.ends_with("}\n"), "{text}");
        assert!(
            text.starts_with("{\n  \"board\": {\n    \"design_settings\""),
            "{text}"
        );

        let ProjectFile::Json(mut written) = ProjectFile::parse(&text) else {
            panic!("the written file should parse: {text}");
        };
        let ProjectFile::Json(mut expected) = file else {
            panic!("the source should parse");
        };
        expected["erc"]["rule_severities"]["pin_not_connected"] = json!("warning");
        expected["meta"]["filename"] = json!("amp.kicad_pro");
        assert_eq!(written, expected);
        let line_width = &written["board"]["design_settings"]["defaults"]["line_width"];
        assert_eq!(line_width, 0.15);
        written["erc"]["rule_severities"]["pin_not_connected"] = json!("error");
        assert_ne!(written, expected);
    }

    #[test]
    fn adds_the_erc_section_to_a_file_without_one() {
        let file = ProjectFile::parse(r#"{"meta": {"filename": "a.kicad_pro", "version": 1}}"#);
        let edit = severities(&[("unannotated", RuleSeverity::Ignore)]);
        let edited = file.with_rule_severities("a.kicad_pro", &edit).unwrap();
        let ProjectFile::Json(value) = &edited else {
            panic!("expected JSON");
        };
        let expected = json!({"rule_severities": {"unannotated": "ignore"}});
        assert_eq!(value["erc"], expected);
        assert_eq!(edited.rule_severities()["unannotated"], RuleSeverity::Ignore);
    }

    #[test]
    fn a_new_file_lists_every_rule_and_names_itself() {
        let edit = severities(&[("pin_to_pin", RuleSeverity::Ignore)]);
        let created = ProjectFile::Missing
            .with_rule_severities("amp.kicad_pro", &edit)
            .unwrap();
        let ProjectFile::Json(value) = &created else {
            panic!("expected JSON");
        };
        let meta = json!({"filename": "amp.kicad_pro", "version": 1});
        assert_eq!(value["meta"], meta);
        let stored = value["erc"]["rule_severities"].as_object().unwrap();
        assert_eq!(stored.len(), ERC_RULES.len());
        assert_eq!(stored["pin_to_pin"], "ignore");
        assert_eq!(stored["pin_not_connected"], "error");
        assert_eq!(stored["unconnected_wire_endpoint"], "warning");
        assert_eq!(ProjectFile::Missing.to_file_string("amp.kicad_pro"), None);
    }
}
