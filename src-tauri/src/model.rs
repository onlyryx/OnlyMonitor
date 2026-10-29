use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CloseBehavior {
    HideToTray,
    Exit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub schema_version: u32,
    #[ts(type = "number")]
    pub sampling_interval_secs: u64,
    pub close_behavior: CloseBehavior,
    pub selected_network_interface: Option<String>,
    pub hide_on_autostart: bool,
    pub high_load_alert_enabled: bool,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CpuCore {
    pub name: String,
    pub usage_percent: Option<f32>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CpuInfo {
    pub brand: String,
    pub logical_cores: usize,
    pub usage_percent: Option<f32>,
    pub cores: Vec<CpuCore>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MemoryInfo {
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub used_bytes: u64,
    pub usage_percent: Option<f64>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number | null")]
    pub available_bytes: Option<u64>,
    pub usage_percent: Option<f64>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    pub name: String,
    #[ts(type = "number")]
    pub received_bytes: u64,
    #[ts(type = "number")]
    pub transmitted_bytes: u64,
    pub download_bytes_per_sec: Option<f64>,
    pub upload_bytes_per_sec: Option<f64>,
}

#[derive(Clone, Copy, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum CounterStatus {
    Initializing,
    Available,
    Unavailable,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DiskActivity {
    pub mount_point: String,
    pub read_bytes_per_sec: Option<f64>,
    pub write_bytes_per_sec: Option<f64>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GpuEngine {
    pub adapter: String,
    pub engine: String,
    pub engine_index: String,
    pub usage_percent: f64,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThermalReading {
    pub label: String,
    pub celsius: f32,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ThermalReport {
    #[ts(type = "number")]
    pub sampled_at_ms: u64,
    pub readings: Vec<ThermalReading>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GpuTemperature {
    pub name: String,
    pub celsius: Option<u32>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GpuTemperatureReport {
    #[ts(type = "number")]
    pub sampled_at_ms: u64,
    pub devices: Vec<GpuTemperature>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DiskHealthStatus {
    Normal,
    Warning,
    Unhealthy,
    Unknown,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NvmeHealth {
    pub critical_warning: u8,
    pub temperature_celsius: Option<i32>,
    pub available_spare: u8,
    pub spare_threshold: u8,
    pub percentage_used: u8,
    pub power_cycles: String,
    pub power_on_hours: String,
    pub unsafe_shutdowns: String,
    pub media_errors: String,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SataSmartAttribute {
    pub id: u8,
    pub current: u8,
    pub worst: u8,
    pub raw_hex: String,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalDiskHealth {
    pub name: String,
    pub status: DiskHealthStatus,
    pub nvme_health: Option<NvmeHealth>,
    pub nvme_error: Option<String>,
    pub sata_temperature_celsius: Option<i16>,
    pub sata_temperature_error: Option<String>,
    pub sata_predict_failure: Option<bool>,
    pub sata_prediction_error: Option<String>,
    pub sata_smart_attributes: Option<Vec<SataSmartAttribute>>,
    pub sata_smart_error: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DiskHealthReport {
    #[ts(type = "number")]
    pub sampled_at_ms: u64,
    pub disks: Vec<PhysicalDiskHealth>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AtaDeviceStats {
    pub power_on_hours: Option<String>,
    pub power_cycles: Option<String>,
    pub reported_uncorrectable_errors: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AtaDeviceStatsDisk {
    pub name: String,
    pub stats: Option<AtaDeviceStats>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct AtaDeviceStatsReport {
    #[ts(type = "number")]
    pub sampled_at_ms: u64,
    pub disks: Vec<AtaDeviceStatsDisk>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceInfo {
    pub disk_status: CounterStatus,
    pub disks: Vec<DiskActivity>,
    pub gpu_status: CounterStatus,
    pub gpu_engines: Vec<GpuEngine>,
}

impl Default for PerformanceInfo {
    fn default() -> Self {
        Self {
            disk_status: CounterStatus::Initializing,
            disks: vec![],
            gpu_status: CounterStatus::Initializing,
            gpu_engines: vec![],
        }
    }
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage_percent: Option<f32>,
    #[ts(type = "number")]
    pub resident_memory_bytes: u64,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ProcessSnapshot {
    #[ts(type = "number")]
    pub sampled_at_ms: u64,
    pub top_cpu: Vec<ProcessInfo>,
    pub top_memory: Vec<ProcessInfo>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum EventMetric {
    Cpu,
    Memory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum EventStatus {
    Active,
    Recovered,
    Interrupted,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct EventProcess {
    pub name: String,
    pub cpu_usage_percent: Option<f32>,
    #[ts(type = "number")]
    pub resident_memory_bytes: u64,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LoadEvent {
    #[ts(type = "number")]
    pub id: u64,
    pub metric: EventMetric,
    pub status: EventStatus,
    #[ts(type = "number")]
    pub started_at_ms: u64,
    #[ts(type = "number")]
    pub confirmed_at_ms: u64,
    #[ts(type = "number")]
    pub last_at_ms: u64,
    #[ts(type = "number")]
    pub duration_secs: u64,
    pub peak_percent: f64,
    pub processes: Vec<EventProcess>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[ts(type = "number")]
    pub sampled_at_ms: u64,
    #[ts(type = "number")]
    pub disks_sampled_at_ms: u64,
    pub hostname: String,
    pub os: String,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub disks: Vec<DiskInfo>,
    pub networks: Vec<NetworkInfo>,
    pub performance: PerformanceInfo,
    pub warnings: Vec<String>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    #[ts(type = "number")]
    pub sampled_at_ms: u64,
    #[ts(type = "number")]
    pub interval_secs: u64,
    pub snapshot: Option<Snapshot>,
}

#[derive(Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct MonitorState {
    pub latest: Option<Snapshot>,
    pub history: Vec<HistoryEntry>,
    pub process_snapshot: Option<ProcessSnapshot>,
    pub load_events: Vec<LoadEvent>,
    pub process_error: Option<String>,
    pub alert_error: Option<String>,
    pub last_error: Option<String>,
    #[ts(type = "number | null")]
    pub last_attempt_at_ms: Option<u64>,
    pub settings: Settings,
    pub config_error: Option<String>,
    pub tray_error: Option<String>,
    pub window_visible: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StartupStatus {
    pub registered: Option<bool>,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum PingOutcome {
    Reply,
    Timeout,
    Unreachable,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PingSample {
    #[ts(type = "number")]
    pub sampled_at_ms: u64,
    pub outcome: PingOutcome,
    pub rtt_ms: Option<u32>,
    pub detail: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontend_contract_matches_rust() {
        let declarations = [
            CloseBehavior::decl(),
            Settings::decl(),
            CpuCore::decl(),
            CpuInfo::decl(),
            MemoryInfo::decl(),
            DiskInfo::decl(),
            NetworkInfo::decl(),
            CounterStatus::decl(),
            DiskActivity::decl(),
            GpuEngine::decl(),
            ThermalReading::decl(),
            ThermalReport::decl(),
            GpuTemperature::decl(),
            GpuTemperatureReport::decl(),
            DiskHealthStatus::decl(),
            PhysicalDiskHealth::decl(),
            NvmeHealth::decl(),
            SataSmartAttribute::decl(),
            DiskHealthReport::decl(),
            AtaDeviceStats::decl(),
            AtaDeviceStatsDisk::decl(),
            AtaDeviceStatsReport::decl(),
            PerformanceInfo::decl(),
            ProcessInfo::decl(),
            ProcessSnapshot::decl(),
            EventMetric::decl(),
            EventStatus::decl(),
            EventProcess::decl(),
            LoadEvent::decl(),
            Snapshot::decl(),
            HistoryEntry::decl(),
            MonitorState::decl(),
            StartupStatus::decl(),
            PingOutcome::decl(),
            PingSample::decl(),
        ];
        let generated = format!(
            "// Generated from Rust model.rs. Do not edit.\n{}\n",
            declarations.map(|d| format!("export {d}")).join("\n")
        );
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts");
        if std::env::var_os("UPDATE_BINDINGS").is_some() {
            std::fs::write(&path, &generated).unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(path).unwrap().replace("\r\n", "\n"),
            generated
        );
    }
}
