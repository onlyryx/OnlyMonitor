use crate::collector::CollectedSample;
use crate::model::*;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Control {
    stopped: bool,
    interval: u64,
}
pub struct MonitorService {
    state: Arc<Mutex<MonitorState>>,
    control: Arc<(Mutex<Control>, Condvar)>,
    saving: Mutex<()>,
}

impl MonitorService {
    #[cfg(test)]
    pub fn start(
        settings: Settings,
        sampler: impl FnMut(u64) -> Result<CollectedSample, String> + Send + 'static,
    ) -> Self {
        Self::start_with_observer(settings, sampler, |_| {})
    }

    pub fn start_with_observer(
        settings: Settings,
        mut sampler: impl FnMut(u64) -> Result<CollectedSample, String> + Send + 'static,
        mut observer: impl FnMut(MonitorState) + Send + 'static,
    ) -> Self {
        let service = Self {
            state: Arc::new(Mutex::new(MonitorState {
                latest: None,
                history: vec![],
                process_snapshot: None,
                load_events: vec![],
                process_error: None,
                alert_error: None,
                last_error: None,
                last_attempt_at_ms: None,
                settings: settings.clone(),
                config_error: None,
                tray_error: None,
                window_visible: false,
            })),
            control: Arc::new((
                Mutex::new(Control {
                    stopped: false,
                    interval: settings.sampling_interval_secs,
                }),
                Condvar::new(),
            )),
            saving: Mutex::new(()),
        };
        let state = Arc::clone(&service.state);
        let control = Arc::clone(&service.control);
        let mut previous_clock: Option<(Instant, u64)> = None;
        let mut event_tracker = crate::events::EventTracker::default();
        std::thread::spawn(move || loop {
            let interval = match control.0.lock() {
                Ok(c) if !c.stopped => c.interval,
                _ => return,
            };
            let at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|t| t.as_millis() as u64)
                .unwrap_or(0);
            let reset_timeline = previous_clock.is_some_and(|(instant, wall)| {
                crate::collector::clock_jumped(
                    instant.elapsed().as_secs_f64(),
                    at as i128 - wall as i128,
                )
            });
            previous_clock = Some((Instant::now(), at));
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sampler(interval)))
                    .unwrap_or_else(|_| Err("系统采集任务异常，等待重试".into()));
            let updated = if let Ok(mut state) = state.lock() {
                let (event_at, values, processes) = match &result {
                    Ok(sample) => (
                        sample.snapshot.sampled_at_ms,
                        [
                            sample.snapshot.cpu.usage_percent.map(f64::from),
                            sample.snapshot.memory.usage_percent,
                        ],
                        Some(&sample.process_snapshot),
                    ),
                    Err(_) => (at, [None, None], None),
                };
                event_tracker.observe(
                    &mut state.load_events,
                    event_at,
                    Instant::now(),
                    values,
                    processes,
                    reset_timeline,
                );
                record(&mut state, result, at, interval, reset_timeline);
                Some(state.clone())
            } else {
                return;
            };
            if let Some(updated) = updated {
                observer(updated);
            }
            let Ok(mut guard) = control.0.lock() else {
                return;
            };
            let mut wait_interval = guard.interval;
            let mut deadline = Instant::now() + Duration::from_secs(wait_interval);
            while !guard.stopped {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                let Ok((next, _)) = control.1.wait_timeout(guard, remaining) else {
                    return;
                };
                guard = next;
                if guard.interval != wait_interval {
                    wait_interval = guard.interval;
                    deadline = Instant::now() + Duration::from_secs(wait_interval);
                }
            }
            if guard.stopped {
                return;
            }
        });
        service
    }
    pub fn read(&self) -> Result<MonitorState, String> {
        self.state
            .lock()
            .map(|s| s.clone())
            .map_err(|_| "采集状态异常".into())
    }
    pub fn update(&self, change: impl FnOnce(&mut MonitorState)) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "采集状态异常")?;
        change(&mut state);
        Ok(())
    }
    pub fn apply_settings(&self, settings: Settings) -> Result<(), String> {
        crate::settings::validate_settings(&settings)?;
        let mut control = self.control.0.lock().map_err(|_| "调度状态异常")?;
        self.update(|state| {
            state.settings = settings.clone();
            state.config_error = None;
        })?;
        control.interval = settings.sampling_interval_secs;
        self.control.1.notify_all();
        Ok(())
    }
    pub fn shutdown(&self) {
        if let Ok(mut control) = self.control.0.lock() {
            control.stopped = true;
            self.control.1.notify_all();
        }
    }
    pub fn save(&self, path: &std::path::Path, settings: Settings) -> Result<Settings, String> {
        let _guard = self.saving.lock().map_err(|_| "设置保存状态异常")?;
        if let Err(error) = crate::settings::save_settings(path, &settings) {
            self.update(|state| state.config_error = Some(error.clone()))?;
            return Err(error);
        }
        self.apply_settings(settings.clone())?;
        Ok(settings)
    }
}

impl Drop for MonitorService {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn record(
    state: &mut MonitorState,
    result: Result<CollectedSample, String>,
    at: u64,
    interval: u64,
    reset_timeline: bool,
) {
    if reset_timeline {
        state.history.clear();
    }
    let time = result
        .as_ref()
        .map(|s| s.snapshot.sampled_at_ms)
        .unwrap_or(at);
    if state
        .history
        .last()
        .is_some_and(|entry| entry.sampled_at_ms > time)
    {
        state.history.clear();
    }
    state.last_attempt_at_ms = Some(at);
    let snapshot = match result {
        Ok(sample) => {
            state.last_error = None;
            state.process_error = None;
            state.process_snapshot = Some(sample.process_snapshot);
            state.latest = Some(sample.snapshot.clone());
            Some(sample.snapshot)
        }
        Err(error) => {
            state.process_snapshot = None;
            state.process_error = Some(error.clone());
            state.last_error = Some(error);
            None
        }
    };
    state.history.push(HistoryEntry {
        sampled_at_ms: time,
        interval_secs: interval,
        snapshot,
    });
    state
        .history
        .retain(|entry| entry.sampled_at_ms >= time.saturating_sub(60000));
    if state.history.len() > 60 {
        state.history.drain(..state.history.len() - 60);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    pub fn snapshot(at: u64) -> CollectedSample {
        CollectedSample {
            snapshot: Snapshot {
                sampled_at_ms: at,
                disks_sampled_at_ms: at,
                hostname: "test".into(),
                os: "Windows".into(),
                cpu: CpuInfo {
                    brand: "test".into(),
                    logical_cores: 1,
                    usage_percent: Some(10.0),
                    cores: vec![],
                },
                memory: MemoryInfo {
                    total_bytes: 100,
                    used_bytes: 50,
                    usage_percent: Some(50.0),
                },
                disks: vec![],
                networks: vec![],
                performance: PerformanceInfo::default(),
                warnings: vec![],
            },
            process_snapshot: ProcessSnapshot {
                sampled_at_ms: at,
                top_cpu: vec![],
                top_memory: vec![],
            },
        }
    }
    #[test]
    fn history_is_bounded_and_errors_preserve_latest_and_other_errors() {
        let service = MonitorService::start(Settings::default(), |_| Err("unused".into()));
        let mut state = service.read().unwrap();
        state.config_error = Some("config".into());
        state.tray_error = Some("tray".into());
        state.process_snapshot = Some(ProcessSnapshot {
            sampled_at_ms: 0,
            top_cpu: vec![],
            top_memory: vec![],
        });
        for at in 0..100 {
            record(&mut state, Ok(snapshot(at * 1000)), at * 1000, 1, false);
        }
        assert_eq!(state.history.len(), 60);
        record(&mut state, Err("read failed".into()), 100000, 1, false);
        assert_eq!(state.latest.as_ref().unwrap().sampled_at_ms, 99000);
        assert!(state.history.last().unwrap().snapshot.is_none());
        assert!(state.process_snapshot.is_none());
        assert_eq!(state.process_error.as_deref(), Some("read failed"));
        record(&mut state, Ok(snapshot(200000)), 200000, 1, false);
        assert_eq!(state.history.len(), 1);
        assert!(state.last_error.is_none());
        assert_eq!(state.config_error.as_deref(), Some("config"));
        assert_eq!(state.tray_error.as_deref(), Some("tray"));
        service.shutdown();
    }
    #[test]
    fn reads_do_not_sample_and_shutdown_stops_worker() {
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        let (send, recv) = std::sync::mpsc::channel();
        let service = MonitorService::start(Settings::default(), move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            send.send(()).unwrap();
            Ok(snapshot(1000))
        });
        recv.recv_timeout(Duration::from_secs(3))
            .expect("worker must sample without UI");
        for _ in 0..100 {
            service.read().unwrap();
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let settings = Settings {
            sampling_interval_secs: 5,
            ..Settings::default()
        };
        service.apply_settings(settings.clone()).unwrap();
        assert_eq!(service.read().unwrap().settings, settings);
        service.shutdown();
        std::thread::sleep(Duration::from_millis(1100));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn failed_save_does_not_change_running_settings() {
        let service = MonitorService::start(Settings::default(), |_| Ok(snapshot(1000)));
        let path = std::env::temp_dir().join(format!("only-monitor-parent-{}", std::process::id()));
        std::fs::write(&path, "file").unwrap();
        let settings = Settings {
            sampling_interval_secs: 5,
            ..Settings::default()
        };
        assert!(service.save(&path.join("settings.json"), settings).is_err());
        assert_eq!(service.read().unwrap().settings.sampling_interval_secs, 1);
        assert!(service.read().unwrap().config_error.is_some());
        service.shutdown();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn forward_clock_correction_clears_old_timeline_even_inside_retention_window() {
        let service = MonitorService::start(Settings::default(), |_| Ok(snapshot(1000)));
        let mut state = service.read().unwrap();
        state.history.clear();
        record(&mut state, Ok(snapshot(10000)), 10000, 5, false);
        record(
            &mut state,
            Ok(snapshot(21000)),
            21000,
            5,
            crate::collector::clock_jumped(5.0, 11000),
        );
        assert_eq!(state.history.len(), 1);
        assert_eq!(state.history[0].sampled_at_ms, 21000);
        service.shutdown();
    }

    #[test]
    fn slow_sampler_is_serial_and_interval_change_wakes_same_worker() {
        let active = Arc::new(AtomicUsize::new(0));
        let active_in = active.clone();
        let maximum = Arc::new(AtomicUsize::new(0));
        let maximum_in = maximum.clone();
        let (send, recv) = std::sync::mpsc::channel();
        let service = MonitorService::start(
            Settings {
                sampling_interval_secs: 5,
                ..Settings::default()
            },
            move |interval| {
                let count = active_in.fetch_add(1, Ordering::SeqCst) + 1;
                maximum_in.fetch_max(count, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(100));
                active_in.fetch_sub(1, Ordering::SeqCst);
                let _ = send.send(interval);
                Ok(snapshot(1000))
            },
        );
        assert_eq!(recv.recv_timeout(Duration::from_secs(3)).unwrap(), 5);
        service.apply_settings(Settings::default()).unwrap();
        assert_eq!(recv.recv_timeout(Duration::from_secs(3)).unwrap(), 1);
        service.shutdown();
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
    }
}
