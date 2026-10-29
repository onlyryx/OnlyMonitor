use crate::model::{CloseBehavior, Settings};
use std::path::Path;

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 3,
            sampling_interval_secs: 1,
            close_behavior: CloseBehavior::HideToTray,
            selected_network_interface: None,
            hide_on_autostart: true,
            high_load_alert_enabled: false,
        }
    }
}

pub fn validate_settings(settings: &Settings) -> Result<(), String> {
    if settings.schema_version != 3 {
        return Err("不支持的配置文件版本".into());
    }
    if ![1, 2, 5].contains(&settings.sampling_interval_secs) {
        return Err("采样间隔只能为 1、2、5 秒".into());
    }
    if settings
        .selected_network_interface
        .as_ref()
        .is_some_and(|name| name.trim().is_empty() || name.len() > 1024)
    {
        return Err("无效的网络接口名称".into());
    }
    Ok(())
}
pub fn load_settings(path: &Path) -> (Settings, Option<String>) {
    let result = (|| {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Settings::default())
            }
            Err(error) => return Err(format!("无法读取设置：{error}")),
        };
        let mut value: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("配置文件损坏：{e}"))?;
        if value.get("schemaVersion").and_then(|v| v.as_u64()) == Some(1) {
            let object = value.as_object_mut().ok_or("配置格式无效")?;
            if object.contains_key("hideOnAutostart") {
                return Err("旧版配置包含不支持的字段".into());
            }
            object.insert("schemaVersion".into(), 2.into());
            object.insert("hideOnAutostart".into(), true.into());
            object.insert("highLoadAlertEnabled".into(), false.into());
        }
        if value.get("schemaVersion").and_then(|v| v.as_u64()) == Some(2) {
            let object = value.as_object_mut().ok_or("配置格式无效")?;
            object.insert("schemaVersion".into(), 3.into());
            object.insert("highLoadAlertEnabled".into(), false.into());
        }
        let settings: Settings =
            serde_json::from_value(value).map_err(|e| format!("配置文件损坏：{e}"))?;
        validate_settings(&settings)?;
        Ok(settings)
    })();
    match result {
        Ok(settings) => (settings, None),
        Err(error) => (Settings::default(), Some(error)),
    }
}
pub fn save_settings(path: &Path, settings: &Settings) -> Result<(), String> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    validate_settings(settings)?;
    let parent = path.parent().ok_or("无效的配置目录")?;
    std::fs::create_dir_all(parent).map_err(|e| format!("无法创建配置目录：{e}"))?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    let temporary = parent.join(format!(
        ".settings-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| format!("无法保存设置：{e}"))?;
    let result = (|| {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|e| format!("无法保存设置：{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "only-monitor-settings-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rejects_invalid_intervals_and_unknown_schema() {
        let mut settings = Settings::default();
        for interval in [0, 3, 60] {
            settings.sampling_interval_secs = interval;
            assert!(validate_settings(&settings).is_err());
        }
        settings.sampling_interval_secs = 1;
        settings.schema_version = 4;
        assert!(validate_settings(&settings).is_err());
    }
    #[test]
    fn migration_preserves_v02_file_and_values() {
        let dir = Temp::new();
        let path = dir.0.join("settings.json");
        let old = r#"{"schemaVersion":1,"samplingIntervalSecs":5,"closeBehavior":"exit","selectedNetworkInterface":"Ethernet"}"#;
        std::fs::write(&path, old).unwrap();
        let (mut settings, error) = load_settings(&path);
        assert!(error.is_none());
        assert_eq!(settings.schema_version, 3);
        assert!(!settings.high_load_alert_enabled);
        assert_eq!(settings.sampling_interval_secs, 5);
        assert_eq!(settings.close_behavior, CloseBehavior::Exit);
        assert_eq!(
            settings.selected_network_interface.as_deref(),
            Some("Ethernet")
        );
        assert_eq!(
            serde_json::to_value(&settings).unwrap()["hideOnAutostart"],
            true
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), old);
        settings.hide_on_autostart = false;
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path).0, settings);
    }

    #[test]
    fn malformed_v3_is_not_silently_migrated() {
        let dir = Temp::new();
        let path = dir.0.join("settings.json");
        for schema in [3, 4] {
            let bytes = format!(
                r#"{{"schemaVersion":{schema},"samplingIntervalSecs":1,"closeBehavior":"hideToTray","selectedNetworkInterface":null}}"#
            );
            std::fs::write(&path, &bytes).unwrap();
            assert!(load_settings(&path).1.is_some());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), bytes);
        }
    }
    #[test]
    fn v2_configuration_migrates_with_alerts_disabled() {
        let dir = Temp::new();
        let path = dir.0.join("settings.json");
        let old = r#"{"schemaVersion":2,"samplingIntervalSecs":2,"closeBehavior":"hideToTray","selectedNetworkInterface":null,"hideOnAutostart":false}"#;
        std::fs::write(&path, old).unwrap();
        let (settings, error) = load_settings(&path);
        assert!(error.is_none());
        assert_eq!(settings.schema_version, 3);
        assert_eq!(settings.sampling_interval_secs, 2);
        assert!(!settings.hide_on_autostart);
        assert!(!settings.high_load_alert_enabled);
        assert_eq!(std::fs::read_to_string(path).unwrap(), old);
    }
    #[test]
    fn missing_and_corrupt_files_do_not_get_overwritten() {
        let dir = Temp::new();
        let path = dir.0.join("settings.json");
        assert_eq!(load_settings(&path).0, Settings::default());
        assert!(load_settings(&path).1.is_none());
        std::fs::write(&path, "broken").unwrap();
        assert!(load_settings(&path).1.is_some());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "broken");
    }
    #[test]
    fn round_trip_and_replace_existing_settings() {
        let dir = Temp::new();
        let path = dir.0.join("settings.json");
        let mut settings = Settings::default();
        save_settings(&path, &settings).unwrap();
        settings.sampling_interval_secs = 5;
        settings.close_behavior = CloseBehavior::Exit;
        settings.selected_network_interface = Some("Ethernet".into());
        save_settings(&path, &settings).unwrap();
        assert_eq!(load_settings(&path).0, settings);
        assert!(load_settings(&path).1.is_none());
        let before = std::fs::read(&path).unwrap();
        settings.sampling_interval_secs = 3;
        assert!(save_settings(&path, &settings).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    #[test]
    fn file_parent_returns_a_write_error() {
        let dir = Temp::new();
        let parent = dir.0.join("file");
        std::fs::write(&parent, "data").unwrap();
        assert!(save_settings(&parent.join("settings.json"), &Settings::default()).is_err());
        assert_eq!(std::fs::read_to_string(parent).unwrap(), "data");
    }
}
