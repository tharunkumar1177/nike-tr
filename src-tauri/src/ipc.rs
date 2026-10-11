use serde::Serialize;
use tauri::{AppHandle, PackageInfo};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

impl From<&PackageInfo> for AppInfo {
    fn from(package: &PackageInfo) -> Self {
        AppInfo {
            name: package.name.clone(),
            version: package.version.to_string(),
        }
    }
}

#[tauri::command]
pub fn get_app_info(app: AppHandle) -> AppInfo {
    AppInfo::from(app.package_info())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_serializes_with_the_ipc_field_names() {
        let info = AppInfo {
            name: "Tron".to_owned(),
            version: "0.1.0".to_owned(),
        };
        let json = serde_json::to_value(&info).expect("AppInfo serializes");
        assert_eq!(
            json,
            serde_json::json!({ "name": "Tron", "version": "0.1.0" })
        );
    }
}
