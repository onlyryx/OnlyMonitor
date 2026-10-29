mod alert;
mod collector;
mod desktop;
mod events;
mod hardware_health;
pub mod model;
mod performance;
mod ping;
pub mod providers;
mod runtime;
mod settings;
mod startup;
mod thermal;

use collector::Collector;
use model::{MonitorState, Settings};
use runtime::MonitorService;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tauri::Manager;
use tauri_plugin_notification::NotificationExt;

struct SettingsPath(Result<PathBuf, String>);

#[tauri::command]
fn get_monitor_state(state: tauri::State<'_, Arc<MonitorService>>) -> Result<MonitorState, String> {
    state.read()
}

#[tauri::command]
fn clear_load_events(state: tauri::State<'_, Arc<MonitorService>>) -> Result<(), String> {
    state.update(|state| state.load_events.clear())
}

#[tauri::command]
async fn save_settings(
    settings: Settings,
    state: tauri::State<'_, Arc<MonitorService>>,
    path: tauri::State<'_, SettingsPath>,
) -> Result<Settings, String> {
    let service = Arc::clone(state.inner());
    let path = path.0.clone()?;
    tauri::async_runtime::spawn_blocking(move || service.save(&path, settings))
        .await
        .map_err(|e| format!("设置保存失败：{e}"))?
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(
            tauri_plugin_frame::FramePluginBuilder::new()
                .auto_titlebar(false)
                .snap_overlay(true)
                .titlebar_height(32)
                .button_width(46)
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            let _ = desktop::set_main_visible(app, true);
        }))
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            use tauri_plugin_frame::WebviewWindowExt;
            app.get_webview_window("main")
                .ok_or("主窗口不可用")?
                .create_overlay_titlebar_with_height(32)?;
            let path = app
                .path()
                .app_config_dir()
                .map(|p| p.join("settings.json"))
                .map_err(|e| format!("无法确定配置目录：{e}"));
            let (settings, error) = match &path {
                Ok(path) => settings::load_settings(path),
                Err(error) => (Settings::default(), Some(error.clone())),
            };
            let hide_on_autostart = settings.hide_on_autostart;
            let mut collector = Collector::new();
            let handle = app.handle().clone();
            let mut alert_policy = alert::HighLoadAlert::default();
            let service = Arc::new(MonitorService::start_with_observer(
                settings,
                move |interval| collector.sample(interval),
                move |state| {
                    if let Some(tray) = handle.tray_by_id("main") {
                        let _ = tray.set_tooltip(Some(desktop::tooltip_summary(&state)));
                    }
                    let Some(snapshot) = &state.latest else {
                        return;
                    };
                    let valid = state.last_error.is_none();
                    let metrics = alert_policy.observe(
                        state.settings.high_load_alert_enabled,
                        valid.then_some(snapshot.cpu.usage_percent).flatten(),
                        valid.then_some(snapshot.memory.usage_percent).flatten(),
                        Instant::now(),
                    );
                    if !metrics.is_empty() {
                        let labels = metrics.iter().map(|metric| match metric {
                            alert::LoadMetric::Cpu => "CPU",
                            alert::LoadMetric::Memory => "内存",
                        }).collect::<Vec<_>>().join("、");
                        let error = handle
                            .notification()
                            .builder()
                            .title("Only Monitor · 持续高负载")
                            .body(format!(
                                "{labels}已持续达到 90%。当前 CPU {}、内存 {}。打开应用查看活动进程。",
                                snapshot.cpu.usage_percent.map_or("—".into(), |v| format!("{v:.1}%")),
                                snapshot.memory.usage_percent.map_or("—".into(), |v| format!("{v:.1}%"))
                            ))
                            .show()
                            .err()
                            .map(|e| e.to_string());
                        if let Some(service) = handle.try_state::<Arc<MonitorService>>() {
                            let _ = service.update(|s| s.alert_error = error);
                        }
                    }
                },
            ));
            service.update(|state| state.config_error = error)?;
            app.manage(service);
            app.manage(SettingsPath(path));
            startup::initialize(app.handle());
            let tray_ready = desktop::initialize(app.handle());
            let autostart = std::env::args_os().skip(1).any(|arg| arg == "--autostart");
            let hide = desktop::initially_hidden(autostart, hide_on_autostart, tray_ready);
            // A failed tray must never strand an initially hidden window.
            desktop::set_main_visible(app.handle(), !hide)?;
            Ok(())
        })
        .on_window_event(desktop::window_event)
        .invoke_handler(tauri::generate_handler![
            get_monitor_state,
            clear_load_events,
            thermal::read_thermal_zones,
            ping::resolve_ping_target,
            ping::ping_once,
            hardware_health::read_gpu_temperatures,
            hardware_health::read_disk_health,
            hardware_health::read_ata_device_stats,
            save_settings,
            startup::get_startup_status,
            startup::set_autostart
        ])
        .build(tauri::generate_context!())
        .expect("Only Monitor could not start");
    app.run(|app, event| {
        if matches!(
            event,
            tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
        ) {
            app.state::<Arc<MonitorService>>().shutdown();
        }
    });
}
