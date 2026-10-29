use crate::model::{ThermalReading, ThermalReport};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static READING: AtomicBool = AtomicBool::new(false);

struct ReadGuard;
impl Drop for ReadGuard {
    fn drop(&mut self) {
        READING.store(false, Ordering::Release);
    }
}

#[tauri::command]
pub async fn read_thermal_zones() -> Result<ThermalReport, String> {
    if READING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("上一次温度读取仍在进行中".into());
    }
    tauri::async_runtime::spawn_blocking(|| {
        let _guard = ReadGuard;
        // Re-enumeration avoids showing sysinfo's previously cached value after a failed refresh.
        let components = sysinfo::Components::new_with_refreshed_list();
        let readings: Vec<_> = components
            .iter()
            .filter_map(|component| {
                component
                    .temperature()
                    .filter(|value| value.is_finite() && (-50.0..=150.0).contains(value))
                    .map(|celsius| ThermalReading {
                        label: component.label().to_owned(),
                        celsius,
                    })
            })
            .collect();
        if readings.is_empty() {
            return Err("系统未提供可读取的温度热区，设备可能不支持或限制访问".into());
        }
        let sampled_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("系统时钟无效：{error}"))?
            .as_millis() as u64;
        Ok(ThermalReport {
            sampled_at_ms,
            readings,
        })
    })
    .await
    .map_err(|error| format!("温度读取任务失败：{error}"))?
}
