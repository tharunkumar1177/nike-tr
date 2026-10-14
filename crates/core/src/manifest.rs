//! The `tron.json` project manifest, which holds Tron-only settings.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::units::Nm;

pub const MANIFEST_FORMAT_VERSION: u32 = 1;
/// 50 mil, KiCad's default schematic grid.
pub const DEFAULT_GRID: Nm = Nm(1_270_000);
pub const MIN_GRID: Nm = Nm(10_000);
pub const MAX_GRID: Nm = Nm(25_400_000);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub schematic: SchematicSettings,
    #[serde(default, skip_serializing_if = "SimulationSettings::is_empty")]
    pub simulation: SimulationSettings,
    /// Keys written by newer versions of Tron, kept so a save doesn't drop them.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SimulationSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub analysis: Option<AnalysisSettings>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl SimulationSettings {
    fn is_empty(&self) -> bool {
        self.analysis.is_none() && self.extra.is_empty()
    }
}

/// The analysis a simulation runs. Times are in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum AnalysisSettings {
    /// `.op`: the DC operating point.
    OperatingPoint,
    /// `.tran`: the response over time.
    Transient {
        step: f64,
        stop: f64,
        /// Results before this time are computed but not kept.
        start: f64,
        /// The largest internal time step, or `null` to let ngspice choose.
        #[serde(default, rename = "maxStep")]
        max_step: Option<f64>,
        /// Start from the components' initial conditions instead of the operating point.
        #[serde(default, rename = "initialConditions")]
        initial_conditions: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchematicSettings {
    pub grid_size_nm: i64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for SchematicSettings {
    fn default() -> Self {
        SchematicSettings {
            grid_size_nm: DEFAULT_GRID.0,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },
    Invalid(String),
    UnsupportedVersion(u32),
}

impl Manifest {
    pub fn new(name: &str) -> Manifest {
        Manifest {
            format_version: MANIFEST_FORMAT_VERSION,
            name: name.to_owned(),
            schematic: SchematicSettings::default(),
            simulation: SimulationSettings::default(),
            extra: Map::new(),
        }
    }

    pub fn parse(source: &str) -> Result<Manifest, ManifestError> {
        let manifest: Manifest = serde_json::from_str(source).map_err(|error| {
            let text = error.to_string();
            let message = match text.rsplit_once(" at line ") {
                Some((message, _)) => message.to_owned(),
                None => text,
            };
            ManifestError::Syntax {
                line: error.line(),
                column: error.column(),
                message,
            }
        })?;
        if manifest.format_version > MANIFEST_FORMAT_VERSION {
            return Err(ManifestError::UnsupportedVersion(manifest.format_version));
        }
        if manifest.format_version == 0 {
            return Err(ManifestError::Invalid(
                "formatVersion must be at least 1".to_owned(),
            ));
        }
        let grid = Nm(manifest.schematic.grid_size_nm);
        if !(MIN_GRID..=MAX_GRID).contains(&grid) {
            return Err(ManifestError::Invalid(format!(
                "schematic.gridSizeNm {} is outside the supported range {} to {}",
                grid.0, MIN_GRID.0, MAX_GRID.0
            )));
        }
        Ok(manifest)
    }

    pub fn to_file_string(&self) -> String {
        let mut text = serde_json::to_string_pretty(self)
            .expect("a manifest with string keys always serializes");
        text.push('\n');
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_the_same_manifest() {
        let manifest = Manifest::new("amp");
        assert_eq!(
            manifest.to_file_string(),
            "{\n  \"formatVersion\": 1,\n  \"name\": \"amp\",\n  \"schematic\": {\n    \"gridSizeNm\": 1270000\n  }\n}\n"
        );
        assert_eq!(Manifest::parse(&manifest.to_file_string()), Ok(manifest));
    }

    #[test]
    fn keeps_unknown_keys() {
        let source = r#"{"formatVersion":1,"name":"amp","schematic":{"gridSizeNm":2540000,"snap":true},"theme":"dark"}"#;
        let manifest = Manifest::parse(source).unwrap();
        assert_eq!(manifest.schematic.grid_size_nm, 2_540_000);
        assert_eq!(manifest.extra.get("theme"), Some(&Value::from("dark")));
        let written = manifest.to_file_string();
        assert!(written.contains("\"snap\": true"));
        assert_eq!(Manifest::parse(&written), Ok(manifest));
    }

    #[test]
    fn defaults_missing_schematic_settings() {
        let manifest = Manifest::parse(r#"{"formatVersion":1,"name":"amp"}"#).unwrap();
        assert_eq!(manifest.schematic, SchematicSettings::default());
    }

    #[test]
    fn stores_the_simulation_analysis() {
        let mut manifest = Manifest::new("amp");
        manifest.simulation.analysis = Some(AnalysisSettings::Transient {
            step: 1e-6,
            stop: 1e-3,
            start: 0.0,
            max_step: None,
            initial_conditions: true,
        });
        let written = manifest.to_file_string();
        assert!(written.contains("\"kind\": \"transient\""), "{written}");
        assert!(written.contains("\"initialConditions\": true"), "{written}");
        assert_eq!(Manifest::parse(&written), Ok(manifest));

        let source = r#"{"formatVersion":1,"name":"amp","simulation":{"analysis":{"kind":"operatingPoint"},"probes":[]}}"#;
        let manifest = Manifest::parse(source).unwrap();
        let analysis = manifest.simulation.analysis;
        assert_eq!(analysis, Some(AnalysisSettings::OperatingPoint));
        assert!(manifest.to_file_string().contains("\"probes\": []"));
    }

    #[test]
    fn defaults_missing_simulation_settings() {
        let manifest = Manifest::parse(r#"{"formatVersion":1,"name":"amp"}"#).unwrap();
        assert_eq!(manifest.simulation, SimulationSettings::default());
        assert!(!manifest.to_file_string().contains("simulation"));
    }

    #[test]
    fn reports_syntax_errors_with_their_position() {
        let error = Manifest::parse("{\n  \"formatVersion\": 1,\n  \"name\": amp\n}").unwrap_err();
        let ManifestError::Syntax { line, column, .. } = &error else {
            panic!("expected a syntax error, got {error:?}");
        };
        assert_eq!(*line, 3);
        assert!((10..=12).contains(column));
    }

    #[test]
    fn rejects_newer_versions_and_out_of_range_grids() {
        assert_eq!(
            Manifest::parse(r#"{"formatVersion":2,"name":"amp"}"#),
            Err(ManifestError::UnsupportedVersion(2))
        );
        assert!(matches!(
            Manifest::parse(r#"{"formatVersion":1,"name":"amp","schematic":{"gridSizeNm":0}}"#),
            Err(ManifestError::Invalid(_))
        ));
    }
}
