use crate::model::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{DiskRefreshKind, Disks, Networks, ProcessRefreshKind, ProcessesToUpdate, System};

const DISK_INTERVAL: Duration = Duration::from_secs(30);
const PROCESS_LIMIT: usize = 8;

pub struct CollectedSample {
    pub snapshot: Snapshot,
    pub process_snapshot: ProcessSnapshot,
}

pub struct Collector {
    system: System,
    disks: Disks,
    networks: Networks,
    previous_networks: HashMap<String, (u64, u64)>,
    previous_processes: HashSet<(u32, u64)>,
    previous_at: Option<Instant>,
    disks_at: Option<Instant>,
    disk_rows: Vec<DiskInfo>,
    disks_timestamp: u64,
    previous_wall: Option<u64>,
    performance: Option<crate::performance::PerformanceCollector>,
}

fn timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|t| t.as_millis() as u64)
        .map_err(|e| format!("系统时钟无效：{e}"))
}

fn byte_rate(current: u64, previous: Option<u64>, seconds: f64) -> Option<f64> {
    if seconds <= 0.0 || !seconds.is_finite() {
        return None;
    }
    current
        .checked_sub(previous?)
        .map(|delta| delta as f64 / seconds)
}

fn normalize_disks(rows: Vec<(String, DiskInfo)>) -> Vec<DiskInfo> {
    let mut seen = HashSet::new();
    rows.into_iter()
        .filter_map(|(key, mut disk)| {
            if !seen.insert(key) {
                return None;
            }
            if disk.total_bytes == 0 {
                disk.available_bytes = None;
                disk.usage_percent = None;
                disk.error = Some("无法读取容量，设备可能未就绪或无权访问".into());
            } else if let Some(available) = disk.available_bytes {
                let available = available.min(disk.total_bytes);
                disk.available_bytes = Some(available);
                disk.usage_percent =
                    Some((disk.total_bytes - available) as f64 / disk.total_bytes as f64 * 100.0);
            }
            Some(disk)
        })
        .collect()
}

fn volume_key(mount: &std::path::Path) -> String {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut path: Vec<u16> = mount.as_os_str().encode_wide().collect();
        if path.last() != Some(&92) {
            path.push(92);
        }
        path.push(0);
        let mut buffer = [0u16; 1024];
        // Both buffers are valid and null-terminated; no elevation is required.
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetVolumeNameForVolumeMountPointW(
                path.as_ptr(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
            )
        };
        if ok != 0 {
            let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
            return String::from_utf16_lossy(&buffer[..end]).to_lowercase();
        }
    }
    mount
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_lowercase()
}

impl Collector {
    pub fn new() -> Self {
        Self {
            system: System::new(),
            disks: Disks::new(),
            networks: Networks::new(),
            previous_networks: HashMap::new(),
            previous_processes: HashSet::new(),
            previous_at: None,
            disks_at: None,
            disk_rows: Vec::new(),
            disks_timestamp: 0,
            previous_wall: None,
            performance: None,
        }
    }

    pub fn sample(&mut self, interval_secs: u64) -> Result<CollectedSample, String> {
        let wall = timestamp()?;
        let interrupted =
            self.previous_at
                .zip(self.previous_wall)
                .is_some_and(|(at, previous_wall)| {
                    baseline_interrupted(
                        at.elapsed().as_secs_f64(),
                        wall as i128 - previous_wall as i128,
                        interval_secs,
                    )
                });
        if interrupted {
            self.previous_networks.clear();
            self.previous_processes.clear();
            self.previous_at = None;
            self.performance = None;
        }
        self.system.refresh_cpu_usage();
        self.system.refresh_memory();
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        self.networks.refresh(true);
        let now = Instant::now();
        let seconds = self
            .previous_at
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        let cpu_ready = self.previous_at.is_some()
            && seconds >= sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.as_secs_f64();
        let mut network_rows = Vec::new();
        let mut next_networks = HashMap::new();
        for (name, data) in &self.networks {
            let received = data.total_received();
            let transmitted = data.total_transmitted();
            let previous = self.previous_networks.get(name);
            network_rows.push(NetworkInfo {
                name: name.clone(),
                received_bytes: received,
                transmitted_bytes: transmitted,
                download_bytes_per_sec: byte_rate(received, previous.map(|p| p.0), seconds),
                upload_bytes_per_sec: byte_rate(transmitted, previous.map(|p| p.1), seconds),
            });
            next_networks.insert(name.clone(), (received, transmitted));
        }
        network_rows.sort_by(|a, b| a.name.cmp(&b.name));
        if self.disks_at.is_none_or(|t| t.elapsed() >= DISK_INTERVAL) {
            // sysinfo can retain old capacities on a failed refresh. Re-enumerate
            // from an empty list so unreadable volumes cannot appear freshly read.
            self.disks = Disks::new();
            self.disks
                .refresh_specifics(true, DiskRefreshKind::nothing().with_storage());
            self.disk_rows = normalize_disks(
                self.disks
                    .iter()
                    .map(|disk| {
                        (
                            volume_key(disk.mount_point()),
                            DiskInfo {
                                name: disk.name().to_string_lossy().into_owned(),
                                mount_point: disk.mount_point().to_string_lossy().into_owned(),
                                total_bytes: disk.total_space(),
                                available_bytes: Some(disk.available_space()),
                                usage_percent: None,
                                error: None,
                            },
                        )
                    })
                    .collect(),
            );
            self.disk_rows
                .sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
            self.disks_at = Some(Instant::now());
            self.disks_timestamp = timestamp()?;
        }
        let mounts = self
            .disk_rows
            .iter()
            .map(|disk| disk.mount_point.clone())
            .collect::<Vec<_>>();
        let performance = self
            .performance
            .get_or_insert_with(crate::performance::PerformanceCollector::new)
            .sample(&mounts);
        let mut warnings = Vec::new();
        if self.system.cpus().is_empty() {
            warnings.push("无法读取 CPU 信息".into());
        }
        let total = self.system.total_memory();
        let used = self.system.used_memory().min(total);
        if total == 0 {
            warnings.push("无法读取系统内存".into());
        }
        if network_rows.is_empty() {
            warnings.push("未发现可读取的网络接口".into());
        }
        if self.disk_rows.is_empty() {
            warnings.push("未发现可读取的磁盘".into());
        }
        for disk in &self.disk_rows {
            if let Some(error) = &disk.error {
                warnings.push(format!("{}：{error}", disk.mount_point));
            }
        }
        let sampled_at_ms = timestamp()?;
        let logical_cores = self.system.cpus().len();
        let mut current_processes = HashSet::new();
        let process_rows = self
            .system
            .processes()
            .iter()
            .map(|(pid, process)| {
                let pid = pid.as_u32();
                let start_time = process.start_time();
                current_processes.insert((pid, start_time));
                ProcessInfo {
                    pid,
                    name: process.name().to_string_lossy().into_owned(),
                    cpu_usage_percent: process_cpu_usage(
                        process.cpu_usage(),
                        pid,
                        start_time,
                        &self.previous_processes,
                        logical_cores,
                    ),
                    resident_memory_bytes: process.memory(),
                }
            })
            .collect();
        let process_snapshot = rank_processes(sampled_at_ms, process_rows);
        self.previous_processes = current_processes;
        let snapshot = Snapshot {
            sampled_at_ms,
            disks_sampled_at_ms: self.disks_timestamp,
            hostname: System::host_name().unwrap_or_else(|| "本机".into()),
            os: System::long_os_version().unwrap_or_else(|| "Windows".into()),
            cpu: CpuInfo {
                brand: self
                    .system
                    .cpus()
                    .first()
                    .map(|c| c.brand().to_owned())
                    .unwrap_or_default(),
                logical_cores: self.system.cpus().len(),
                usage_percent: cpu_ready.then(|| self.system.global_cpu_usage()),
                cores: self
                    .system
                    .cpus()
                    .iter()
                    .map(|cpu| CpuCore {
                        name: cpu.name().to_owned(),
                        usage_percent: cpu_ready.then(|| cpu.cpu_usage()),
                    })
                    .collect(),
            },
            memory: MemoryInfo {
                total_bytes: total,
                used_bytes: used,
                usage_percent: (total > 0).then(|| used as f64 / total as f64 * 100.0),
            },
            disks: self.disk_rows.clone(),
            networks: network_rows,
            performance,
            warnings,
        };
        self.previous_networks = next_networks;
        self.previous_at = Some(now);
        self.previous_wall = Some(wall);
        Ok(CollectedSample {
            snapshot,
            process_snapshot,
        })
    }
}

fn process_cpu_usage(
    total_cpu_percent: f32,
    pid: u32,
    start_time: u64,
    previous: &HashSet<(u32, u64)>,
    logical_cores: usize,
) -> Option<f32> {
    if logical_cores == 0
        || !previous.contains(&(pid, start_time))
        || !total_cpu_percent.is_finite()
    {
        return None;
    }
    Some((total_cpu_percent / logical_cores as f32).clamp(0.0, 100.0))
}

fn rank_processes(sampled_at_ms: u64, processes: Vec<ProcessInfo>) -> ProcessSnapshot {
    let mut top_cpu = processes.clone();
    top_cpu.sort_by(|a, b| {
        b.cpu_usage_percent
            .unwrap_or(f32::NEG_INFINITY)
            .total_cmp(&a.cpu_usage_percent.unwrap_or(f32::NEG_INFINITY))
            .then_with(|| a.pid.cmp(&b.pid))
    });
    top_cpu.truncate(PROCESS_LIMIT);

    let mut top_memory = processes;
    top_memory.sort_by(|a, b| {
        b.resident_memory_bytes
            .cmp(&a.resident_memory_bytes)
            .then_with(|| a.pid.cmp(&b.pid))
    });
    top_memory.truncate(PROCESS_LIMIT);
    ProcessSnapshot {
        sampled_at_ms,
        top_cpu,
        top_memory,
    }
}

fn baseline_interrupted(seconds: f64, wall_delta_ms: i128, interval_secs: u64) -> bool {
    seconds > (10.0_f64).max(interval_secs as f64 * 3.0) || clock_jumped(seconds, wall_delta_ms)
}

pub(crate) fn clock_jumped(seconds: f64, wall_delta_ms: i128) -> bool {
    (wall_delta_ms - (seconds * 1000.0) as i128).abs() > 5000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_cpu_needs_matching_pid_and_start_time_and_is_normalized() {
        let previous = HashSet::from([(7, 100)]);
        assert_eq!(process_cpu_usage(200.0, 7, 100, &previous, 8), Some(25.0));
        assert_eq!(process_cpu_usage(200.0, 8, 100, &previous, 8), None);
        assert_eq!(process_cpu_usage(200.0, 7, 101, &previous, 8), None);
        assert_eq!(process_cpu_usage(200.0, 7, 100, &previous, 0), None);
    }

    #[test]
    fn process_snapshot_keeps_only_eight_entries_per_resource_ranking() {
        let processes = (0..12)
            .map(|pid| ProcessInfo {
                pid,
                name: format!("process-{pid}"),
                cpu_usage_percent: Some(pid as f32),
                resident_memory_bytes: pid as u64,
            })
            .collect();
        let snapshot = rank_processes(1234, processes);
        assert_eq!(snapshot.top_cpu.len(), 8);
        assert_eq!(snapshot.top_memory.len(), 8);
        assert_eq!(snapshot.top_cpu[0].pid, 11);
        assert_eq!(snapshot.top_memory[0].pid, 11);
    }

    #[test]
    fn rates_need_baseline_and_handle_resets() {
        assert_eq!(byte_rate(100, None, 1.0), None);
        assert_eq!(byte_rate(400, Some(100), 2.0), Some(150.0));
        assert_eq!(byte_rate(10, Some(100), 1.0), None);
        assert_eq!(byte_rate(100, Some(0), 0.0), None);
    }

    #[test]
    fn interruptions_and_clock_jumps_reset_baseline() {
        assert!(!baseline_interrupted(5.1, 5100, 5));
        assert!(!baseline_interrupted(11.0, 11000, 5));
        assert!(baseline_interrupted(16.0, 16000, 5));
        assert!(baseline_interrupted(11.0, 11000, 1));
        assert!(baseline_interrupted(1.0, -6000, 1));
        assert!(baseline_interrupted(1.0, 7000, 1));
    }

    #[test]
    fn duplicate_mounts_and_unreadable_disks_are_handled() {
        let disk = |mount: &str, total| DiskInfo {
            name: "Volume".into(),
            mount_point: mount.into(),
            total_bytes: total,
            available_bytes: Some(25),
            usage_percent: None,
            error: None,
        };
        let result = normalize_disks(vec![
            ("volume-a".into(), disk("C:\\", 100)),
            ("volume-a".into(), disk("C:\\alias\\", 100)),
            ("volume-b".into(), disk("D:\\", 0)),
        ]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].usage_percent, Some(75.0));
        assert_eq!(result[1].available_bytes, None);
        assert!(result[1].error.is_some());
    }

    #[test]
    fn first_sample_is_warming() {
        let mut collector = Collector::new();
        let first = collector.sample(1).unwrap();
        assert_eq!(first.snapshot.cpu.usage_percent, None);
        assert!(first
            .process_snapshot
            .top_cpu
            .iter()
            .all(|process| process.cpu_usage_percent.is_none()));
        assert!(first
            .snapshot
            .networks
            .iter()
            .all(|n| n.download_bytes_per_sec.is_none()));
    }

    #[test]
    fn real_windows_sample_becomes_ready_and_keeps_disk_cache() {
        let mut collector = Collector::new();
        let first = collector.sample(1).unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        let second = collector.sample(1).unwrap();
        assert!(second.snapshot.sampled_at_ms > first.snapshot.sampled_at_ms);
        assert_eq!(
            second.snapshot.disks_sampled_at_ms,
            first.snapshot.disks_sampled_at_ms
        );
        assert!(second.snapshot.memory.total_bytes > 0);
        let usage = second.snapshot.cpu.usage_percent.unwrap();
        assert!((0.0..=100.0).contains(&usage));
        assert_eq!(
            second.snapshot.cpu.logical_cores,
            second.snapshot.cpu.cores.len()
        );
        assert!(
            !second.process_snapshot.top_cpu.is_empty()
                || !second.process_snapshot.top_memory.is_empty()
        );
        for network in &second.snapshot.networks {
            if let Some(previous) = first
                .snapshot
                .networks
                .iter()
                .find(|n| n.name == network.name)
            {
                if network.received_bytes >= previous.received_bytes {
                    assert!(network.download_bytes_per_sec.unwrap() >= 0.0);
                }
            }
        }
    }
}
