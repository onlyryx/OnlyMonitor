use crate::model::CloseBehavior;
use crate::runtime::MonitorService;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};

struct TrayStatus(AtomicBool);

pub fn tooltip_summary(state: &crate::model::MonitorState) -> String {
    if state.last_error.is_some() {
        return "Only Monitor · 采集异常".into();
    }
    let Some(snapshot) = &state.latest else {
        return "Only Monitor · 正在初始化".into();
    };
    let cpu = snapshot
        .cpu
        .usage_percent
        .map_or("—".into(), |v| format!("{v:.0}%"));
    let memory = snapshot
        .memory
        .usage_percent
        .map_or("—".into(), |v| format!("{v:.0}%"));
    format!("Only Monitor · CPU {cpu} · RAM {memory}")
}

fn perform_and_reconcile(
    action: impl FnOnce() -> Result<(), String>,
    reconcile: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let result = action();
    let synchronized = reconcile();
    result.and(synchronized)
}

fn should_hide_on_close(behavior: CloseBehavior, tray_ready: bool) -> bool {
    behavior == CloseBehavior::HideToTray && tray_ready
}

pub fn initially_hidden(autostart: bool, preference: bool, tray_ready: bool) -> bool {
    autostart && preference && tray_ready
}

pub fn set_main_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
    let window = app.get_webview_window("main").ok_or("主窗口不可用")?;
    let result = perform_and_reconcile(
        || {
            if visible {
                window.show().map_err(|e| e.to_string())?;
                window.unminimize().map_err(|e| e.to_string())?;
                window.set_focus().map_err(|e| e.to_string())?;
            } else {
                if !app
                    .try_state::<TrayStatus>()
                    .is_some_and(|s| s.0.load(Ordering::SeqCst))
                {
                    return Err("托盘不可用，无法隐藏窗口".into());
                }
                window.hide().map_err(|e| e.to_string())?;
            }
            Ok(())
        },
        || {
            let actual_visibility = window.is_visible().map_err(|e| e.to_string())?;
            if let Some(service) = app.try_state::<Arc<MonitorService>>() {
                service.update(|state| state.window_visible = actual_visibility)?;
            }
            app.emit_to("main", "monitor-visibility-changed", actual_visibility)
                .map_err(|e| e.to_string())?;
            Ok(())
        },
    );
    if let Err(error) = &result {
        if let Some(service) = app.try_state::<Arc<MonitorService>>() {
            let _ = service.update(|s| s.tray_error = Some(format!("窗口操作失败：{error}")));
        }
    }
    result
}

pub fn initialize(app: &AppHandle) -> bool {
    app.manage(TrayStatus(AtomicBool::new(false)));
    let result = (|| -> tauri::Result<()> {
        let show = MenuItem::with_id(app, "show", "显示窗口", true, None::<&str>)?;
        let hide = MenuItem::with_id(app, "hide", "隐藏窗口", true, None::<&str>)?;
        let quit = MenuItem::with_id(app, "quit", "退出 Only Monitor", true, None::<&str>)?;
        let menu = Menu::with_items(app, &[&show, &hide, &quit])?;
        let mut builder = TrayIconBuilder::with_id("main")
            .tooltip("Only Monitor · 正在本机监控")
            .menu(&menu)
            .show_menu_on_left_click(false);
        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        }
        builder
            .on_menu_event(|app, event| match event.id.as_ref() {
                "show" => {
                    let _ = set_main_visible(app, true);
                }
                "hide" => {
                    let _ = set_main_visible(app, false);
                }
                "quit" => {
                    app.state::<Arc<MonitorService>>().shutdown();
                    app.exit(0);
                }
                _ => {}
            })
            .on_tray_icon_event(|tray, event| {
                if matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }
                ) {
                    let _ = set_main_visible(tray.app_handle(), true);
                }
            })
            .build(app)?;
        Ok(())
    })();
    match result {
        Ok(()) => app.state::<TrayStatus>().0.store(true, Ordering::SeqCst),
        Err(error) => {
            let _ = app
                .state::<Arc<MonitorService>>()
                .update(|s| s.tray_error = Some(format!("托盘创建失败，关闭窗口将退出：{error}")));
        }
    }
    app.state::<TrayStatus>().0.load(Ordering::SeqCst)
}

pub fn window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    if window.label() != "main" {
        return;
    }
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let app = window.app_handle();
        let behavior = app
            .state::<Arc<MonitorService>>()
            .read()
            .map(|s| s.settings.close_behavior)
            .unwrap_or(CloseBehavior::Exit);
        let ready = app
            .try_state::<TrayStatus>()
            .is_some_and(|s| s.0.load(Ordering::SeqCst));
        if should_hide_on_close(behavior, ready) && set_main_visible(app, false).is_ok() {
            api.prevent_close();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> crate::model::MonitorState {
        crate::model::MonitorState {
            latest: None,
            history: vec![],
            process_snapshot: None,
            load_events: vec![],
            process_error: None,
            alert_error: None,
            last_error: None,
            last_attempt_at_ms: None,
            settings: crate::model::Settings::default(),
            config_error: None,
            tray_error: None,
            window_visible: true,
        }
    }
    #[test]
    fn tray_tooltip_shows_live_summary_and_never_masks_collection_errors() {
        let mut current = state();
        assert_eq!(tooltip_summary(&current), "Only Monitor · 正在初始化");
        current.latest = Some(crate::model::Snapshot {
            sampled_at_ms: 1,
            disks_sampled_at_ms: 1,
            hostname: "test".into(),
            os: "Windows".into(),
            cpu: crate::model::CpuInfo {
                brand: "test".into(),
                logical_cores: 1,
                usage_percent: Some(10.0),
                cores: vec![],
            },
            memory: crate::model::MemoryInfo {
                total_bytes: 100,
                used_bytes: 50,
                usage_percent: Some(50.0),
            },
            disks: vec![],
            networks: vec![],
            performance: crate::model::PerformanceInfo::default(),
            warnings: vec![],
        });
        assert!(tooltip_summary(&current).contains("CPU 10% · RAM 50%"));
        current.last_error = Some("failed".into());
        assert_eq!(tooltip_summary(&current), "Only Monitor · 采集异常");
    }
    #[test]
    fn close_only_hides_when_tray_is_available_and_quit_always_exits() {
        assert!(should_hide_on_close(CloseBehavior::HideToTray, true));
        assert!(!should_hide_on_close(CloseBehavior::HideToTray, false));
        assert!(!should_hide_on_close(CloseBehavior::Exit, true));
        assert!(!should_hide_on_close(CloseBehavior::Exit, false));
    }

    #[test]
    fn partial_show_failure_still_publishes_actual_visibility() {
        let published = std::cell::Cell::new(false);
        let result = perform_and_reconcile(
            || Err("focus denied".into()),
            || {
                published.set(true);
                Ok(())
            },
        );
        assert!(published.get());
        assert_eq!(result.unwrap_err(), "focus denied");
    }

    #[test]
    fn startup_only_hides_with_explicit_entry_preference_and_ready_tray() {
        for autostart in [false, true] {
            for preference in [false, true] {
                for tray in [false, true] {
                    assert_eq!(
                        initially_hidden(autostart, preference, tray),
                        autostart && preference && tray
                    );
                }
            }
        }
    }
}
