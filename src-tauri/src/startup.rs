use crate::model::StartupStatus;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

struct StartupControl {
    changing: Mutex<()>,
    initialization_error: Option<String>,
}

fn unknown(error: String) -> StartupStatus {
    StartupStatus {
        registered: None,
        error: Some(error),
    }
}

pub fn initialize(app: &AppHandle) {
    let error = app
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("Only Monitor")
                .arg("--autostart")
                .build(),
        )
        .err()
        .map(|e| format!("登录启动组件不可用：{e}"));
    app.manage(StartupControl {
        changing: Mutex::new(()),
        initialization_error: error,
    });
}

fn update_registration(
    desired: bool,
    change: impl FnOnce() -> Result<(), String>,
    read: impl FnOnce() -> Result<bool, String>,
) -> StartupStatus {
    let mut errors: Vec<String> = change().err().into_iter().collect();
    let registered = match read() {
        Ok(actual) => {
            if errors.is_empty() && actual != desired {
                errors.push("系统启动项与请求不一致，请刷新后重试".into());
            }
            Some(actual)
        }
        Err(error) => {
            errors.push(error);
            None
        }
    };
    StartupStatus {
        registered,
        error: (!errors.is_empty()).then(|| errors.join("；")),
    }
}

async fn operate(app: AppHandle, desired: Option<bool>) -> StartupStatus {
    tauri::async_runtime::spawn_blocking(move || {
        let control = app.state::<StartupControl>();
        let Ok(_guard) = control.changing.lock() else {
            return unknown("启动项操作状态异常".into());
        };
        if let Some(error) = &control.initialization_error {
            return unknown(error.clone());
        }
        let manager = app.autolaunch();
        let read = || {
            manager
                .is_enabled()
                .map_err(|e| format!("无法读取登录启动登记：{e}"))
        };
        match desired {
            Some(enabled) => update_registration(
                enabled,
                || {
                    if enabled {
                        manager.enable()
                    } else {
                        manager.disable()
                    }
                    .map_err(|e| format!("无法更改登录启动登记：{e}"))
                },
                read,
            ),
            None => match read() {
                Ok(registered) => StartupStatus {
                    registered: Some(registered),
                    error: None,
                },
                Err(error) => unknown(error),
            },
        }
    })
    .await
    .unwrap_or_else(|e| unknown(format!("启动项操作失败：{e}")))
}

#[tauri::command]
pub async fn get_startup_status(app: AppHandle) -> StartupStatus {
    operate(app, None).await
}

#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> StartupStatus {
    operate(app, Some(enabled)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_change_preserves_readable_actual_state() {
        let state = update_registration(true, || Err("denied".into()), || Ok(false));
        assert_eq!(state.registered, Some(false));
        assert!(state.error.unwrap().contains("denied"));
    }

    #[test]
    fn query_failure_is_unknown_and_mismatch_is_an_error() {
        let state = update_registration(true, || Ok(()), || Err("read failed".into()));
        assert_eq!(state.registered, None);
        assert!(state.error.unwrap().contains("read failed"));
        let mismatch = update_registration(true, || Ok(()), || Ok(false));
        assert_eq!(mismatch.registered, Some(false));
        assert!(mismatch.error.is_some());
        let success = update_registration(false, || Ok(()), || Ok(false));
        assert_eq!(success.registered, Some(false));
        assert!(success.error.is_none());
    }
}
