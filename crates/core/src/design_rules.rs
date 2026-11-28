//! The board design rules Tron routes and checks with, stored where KiCad keeps them in
//! the `.kicad_pro`: the constraints in `board.design_settings.rules` and the routing
//! defaults and clearance in the `Default` entry of `net_settings.classes`. Lengths are
//! millimeters in the file.

use serde_json::{Map, Value, json};

use crate::project_file::{PROJECT_FILE_VERSION, ProjectFile};
use crate::units::Nm;

const RULES: &str = "/board/design_settings/rules";
const CLASSES: &str = "/net_settings/classes";
const DEFAULT_CLASS: &str = "Default";
/// The largest value any rule accepts.
pub const MAX_RULE: Nm = Nm(25_000_000);

/// The rules for one board. Every length is positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesignRules {
    /// The copper-to-copper clearance between different nets: the larger of the `Default`
    /// net class clearance and `min_clearance`.
    pub clearance: Nm,
    pub min_track_width: Nm,
    pub min_via_diameter: Nm,
    /// The smallest via drill, KiCad's `min_through_hole_diameter`.
    pub min_via_drill: Nm,
    /// The smallest via annular ring, KiCad's `min_via_annular_width`.
    pub min_annular_ring: Nm,
    pub edge_clearance: Nm,
    /// The width new tracks get.
    pub track_width: Nm,
    /// The diameter new vias get.
    pub via_diameter: Nm,
    /// The drill new vias get.
    pub via_drill: Nm,
}

impl Default for DesignRules {
    /// KiCad's rules for a new project.
    fn default() -> DesignRules {
        DesignRules {
            clearance: Nm(200_000),
            min_track_width: Nm(200_000),
            min_via_diameter: Nm(500_000),
            min_via_drill: Nm(300_000),
            min_annular_ring: Nm(100_000),
            edge_clearance: Nm(500_000),
            track_width: Nm(200_000),
            via_diameter: Nm(600_000),
            via_drill: Nm(300_000),
        }
    }
}

/// The constraint keys in `board.design_settings.rules`.
const CONSTRAINT_KEYS: [&str; 5] = [
    "min_track_width",
    "min_via_diameter",
    "min_through_hole_diameter",
    "min_via_annular_width",
    "min_copper_edge_clearance",
];

/// The `Default` net class keys.
const CLASS_KEYS: [&str; 4] = ["clearance", "track_width", "via_diameter", "via_drill"];

impl DesignRules {
    /// The rules as `(constraints, class)` values in the order of [`CONSTRAINT_KEYS`] and
    /// [`CLASS_KEYS`].
    fn values(&self) -> ([Nm; 5], [Nm; 4]) {
        (
            [
                self.min_track_width,
                self.min_via_diameter,
                self.min_via_drill,
                self.min_annular_ring,
                self.edge_clearance,
            ],
            [
                self.clearance,
                self.track_width,
                self.via_diameter,
                self.via_drill,
            ],
        )
    }

    /// Checks that every rule is positive and at most [`MAX_RULE`], and that each drill is
    /// smaller than its via.
    pub fn validate(&self) -> Result<(), String> {
        let (constraints, class) = self.values();
        let all: Vec<Nm> = constraints.into_iter().chain(class).collect();
        if all.iter().any(|value| value.0 <= 0) {
            return Err("Every design rule must be greater than 0.".to_owned());
        }
        if all.iter().any(|value| *value > MAX_RULE) {
            return Err("Design rules can be at most 25 mm.".to_owned());
        }
        if self.via_drill >= self.via_diameter {
            return Err("The via drill must be smaller than the via diameter.".to_owned());
        }
        if self.min_via_drill >= self.min_via_diameter {
            let message = "Minimum via drill must be smaller than minimum via diameter.";
            return Err(message.to_owned());
        }
        Ok(())
    }
}

impl ProjectFile {
    /// The stored design rules, each missing or unreadable one taking KiCad's default.
    pub fn design_rules(&self) -> DesignRules {
        let mut rules = DesignRules::default();
        let ProjectFile::Json(root) = self else {
            return rules;
        };
        let stored = root.pointer(RULES);
        let class = default_class(root);
        let read = |parent: Option<&Value>, key: &str| {
            let value = parent?.get(key)?.as_f64()?;
            Nm::from_mm(value).ok().filter(|value| value.0 > 0)
        };
        let constraints = [
            &mut rules.min_track_width,
            &mut rules.min_via_diameter,
            &mut rules.min_via_drill,
            &mut rules.min_annular_ring,
            &mut rules.edge_clearance,
        ];
        for (slot, key) in constraints.into_iter().zip(CONSTRAINT_KEYS) {
            if let Some(value) = read(stored, key) {
                *slot = value;
            }
        }
        let defaults = [
            &mut rules.clearance,
            &mut rules.track_width,
            &mut rules.via_diameter,
            &mut rules.via_drill,
        ];
        for (slot, key) in defaults.into_iter().zip(CLASS_KEYS) {
            if let Some(value) = read(class, key) {
                *slot = value;
            }
        }
        if let Some(floor) = read(stored, "min_clearance") {
            rules.clearance = rules.clearance.max(floor);
        }
        rules
    }

    /// This file with `rules` stored, every other key kept. A missing file becomes one
    /// holding just the rules and a `meta` block naming `file_name`.
    pub fn with_design_rules(
        &self,
        file_name: &str,
        rules: &DesignRules,
    ) -> Result<ProjectFile, String> {
        let mut root = match self {
            ProjectFile::Unreadable(reason) => return Err(reason.clone()),
            ProjectFile::Json(value) => value.clone(),
            ProjectFile::Missing => json!({
                "meta": { "filename": file_name, "version": PROJECT_FILE_VERSION },
            }),
        };
        let not_project = || "the file is not a KiCad project file".to_owned();
        let (constraints, class) = rules.values();
        let stored = root
            .as_object_mut()
            .and_then(|root| object_entry(root, "board"))
            .and_then(|board| object_entry(board, "design_settings"))
            .and_then(|settings| object_entry(settings, "rules"))
            .ok_or_else(not_project)?;
        for (key, value) in CONSTRAINT_KEYS.into_iter().zip(constraints) {
            stored.insert(key.to_owned(), Value::from(value.to_mm()));
        }
        let classes = root
            .as_object_mut()
            .and_then(|root| object_entry(root, "net_settings"))
            .and_then(|settings| array_entry(settings, "classes"))
            .ok_or_else(not_project)?;
        let position = classes.iter().position(is_default_class);
        let entry = match position {
            Some(position) => &mut classes[position],
            None => {
                classes.push(json!({ "name": DEFAULT_CLASS }));
                classes.last_mut().ok_or_else(not_project)?
            }
        };
        let entry = entry.as_object_mut().ok_or_else(not_project)?;
        for (key, value) in CLASS_KEYS.into_iter().zip(class) {
            entry.insert(key.to_owned(), Value::from(value.to_mm()));
        }
        Ok(ProjectFile::Json(root))
    }
}

/// The `Default` entry of `net_settings.classes`.
fn default_class(root: &Value) -> Option<&Value> {
    let classes = root.pointer(CLASSES)?.as_array()?;
    classes.iter().find(|class| is_default_class(class))
}

/// Whether a net class entry is the `Default` class.
fn is_default_class(class: &Value) -> bool {
    class.get("name").and_then(Value::as_str) == Some(DEFAULT_CLASS)
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

/// The array under `key`, created when missing. `None` when `key` holds something else.
fn array_entry<'a>(map: &'a mut Map<String, Value>, key: &str) -> Option<&'a mut Vec<Value>> {
    map.entry(key)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_rules_take_kicad_defaults() {
        assert_eq!(ProjectFile::Missing.design_rules(), DesignRules::default());
        let file = ProjectFile::parse(
            r#"{"board": {"design_settings": {"rules": {
                "min_track_width": 0.15, "min_clearance": 0.3, "min_via_diameter": "big"
            }}}, "net_settings": {"classes": [
                {"name": "Power", "track_width": 1.0},
                {"name": "Default", "clearance": 0.25, "via_drill": 0.4}
            ]}}"#,
        );
        let rules = file.design_rules();
        assert_eq!(rules.min_track_width, Nm(150_000));
        assert_eq!(rules.min_via_diameter, Nm(500_000));
        assert_eq!(rules.clearance, Nm(300_000));
        assert_eq!(rules.via_drill, Nm(400_000));
        assert_eq!(rules.track_width, Nm(200_000));
    }

    #[test]
    fn writing_rules_keeps_every_other_key() {
        let file = ProjectFile::parse(
            r#"{"board": {"design_settings": {"defaults": {"line_width": 0.1},
                "rules": {"min_hole_clearance": 0.25}}},
              "erc": {"rule_severities": {"pin_to_pin": "error"}},
              "net_settings": {"classes": [{"name": "Default", "pcb_color": "rgba(0,0,0,0)"}]}}"#,
        );
        let rules = DesignRules {
            clearance: Nm(150_000),
            track_width: Nm(250_000),
            ..DesignRules::default()
        };
        let written = file.with_design_rules("a.kicad_pro", &rules).unwrap();
        assert_eq!(written.design_rules(), rules);
        let ProjectFile::Json(value) = &written else {
            panic!("expected JSON");
        };
        let settings = &value["board"]["design_settings"];
        assert_eq!(settings["defaults"]["line_width"], 0.1);
        assert_eq!(settings["rules"]["min_hole_clearance"], 0.25);
        assert_eq!(settings["rules"]["min_copper_edge_clearance"], 0.5);
        let class = &value["net_settings"]["classes"][0];
        assert_eq!(class["pcb_color"], "rgba(0,0,0,0)");
        assert_eq!(class["track_width"], 0.25);
        assert_eq!(value["erc"]["rule_severities"]["pin_to_pin"], "error");

        let created = ProjectFile::Missing
            .with_design_rules("b.kicad_pro", &rules)
            .unwrap();
        assert_eq!(created.design_rules(), rules);
        let unreadable = ProjectFile::parse("[1]");
        assert!(unreadable.with_design_rules("c.kicad_pro", &rules).is_err());
    }

    #[test]
    fn rules_must_be_positive_and_drills_smaller_than_vias() {
        assert_eq!(DesignRules::default().validate(), Ok(()));
        let zero = DesignRules {
            edge_clearance: Nm::ZERO,
            ..DesignRules::default()
        };
        assert!(zero.validate().is_err());
        let drill = DesignRules {
            via_drill: Nm(600_000),
            ..DesignRules::default()
        };
        let message = drill.validate().unwrap_err();
        assert!(message.contains("via drill"), "{message}");
        let huge = DesignRules {
            track_width: Nm(26_000_000),
            ..DesignRules::default()
        };
        assert!(huge.validate().is_err());
    }
}
