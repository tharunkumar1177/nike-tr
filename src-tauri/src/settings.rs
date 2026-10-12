//! Per-user application settings stored in the OS app config directory.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tauri::{AppHandle, Manager};

use crate::error::AppError;

const SETTINGS_FILE: &str = "settings.json";
pub const MAX_RECENT_PROJECTS: usize = 10;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// Project folders, most recently used first.
    #[serde(default)]
    pub recent_projects: Vec<PathBuf>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Settings {
    pub fn read(path: &Path) -> Result<Settings, AppError> {
        if !path.exists() {
            return Ok(Settings::default());
        }
        let text = fs::read_to_string(path)
            .map_err(|error| AppError::settings("Could not read the Tron settings file.", error))?;
        serde_json::from_str(&text)
            .map_err(|error| AppError::settings("The Tron settings file is damaged.", error))
    }

    pub fn write(&self, path: &Path) -> Result<(), AppError> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|error| {
                AppError::settings("Could not create the Tron settings folder.", error)
            })?;
        }
        let mut text = serde_json::to_string_pretty(self)
            .map_err(|error| AppError::settings("Could not encode the Tron settings.", error))?;
        text.push('\n');
        tron_core::write_atomic(path, &text)
            .map_err(|error| AppError::settings("Could not save the Tron settings file.", error))
    }

    pub fn remember(&mut self, dir: &Path) {
        self.forget(dir);
        self.recent_projects.insert(0, dir.to_path_buf());
        self.recent_projects.truncate(MAX_RECENT_PROJECTS);
    }

    pub fn forget(&mut self, dir: &Path) {
        self.recent_projects.retain(|known| !same_path(known, dir));
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

pub fn settings_path(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(SETTINGS_FILE))
        .map_err(|error| AppError::settings("Could not locate the Tron settings folder.", error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remembering_moves_a_project_to_the_front_without_duplicates() {
        let mut settings = Settings::default();
        settings.remember(Path::new("a"));
        settings.remember(Path::new("b"));
        settings.remember(Path::new("a"));
        assert_eq!(
            settings.recent_projects,
            [PathBuf::from("a"), PathBuf::from("b")]
        );
    }

    #[test]
    fn the_recent_list_is_capped() {
        let mut settings = Settings::default();
        for index in 0..MAX_RECENT_PROJECTS + 3 {
            settings.remember(&PathBuf::from(format!("p{index}")));
        }
        assert_eq!(settings.recent_projects.len(), MAX_RECENT_PROJECTS);
        assert_eq!(
            settings.recent_projects[0],
            PathBuf::from(format!("p{}", MAX_RECENT_PROJECTS + 2))
        );
    }

    #[test]
    fn forgetting_removes_a_project() {
        let mut settings = Settings::default();
        settings.remember(Path::new("a"));
        settings.remember(Path::new("b"));
        settings.forget(Path::new("a"));
        assert_eq!(settings.recent_projects, [PathBuf::from("b")]);
    }

    #[test]
    fn settings_round_trip_and_keep_unknown_keys() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("nested").join(SETTINGS_FILE);
        assert_eq!(Settings::read(&path).unwrap(), Settings::default());

        let mut settings = Settings::default();
        settings.remember(Path::new("projects/amp"));
        settings
            .extra
            .insert("theme".to_owned(), Value::from("dark"));
        settings.write(&path).unwrap();
        assert_eq!(Settings::read(&path).unwrap(), settings);
    }

    #[test]
    fn a_damaged_settings_file_is_reported() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join(SETTINGS_FILE);
        fs::write(&path, "{ not json").unwrap();
        let error = Settings::read(&path).unwrap_err();
        assert_eq!(error.message, "The Tron settings file is damaged.");
    }
}
