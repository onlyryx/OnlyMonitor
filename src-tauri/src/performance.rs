use crate::model::{CounterStatus, DiskActivity, GpuEngine, PerformanceInfo};
use std::collections::HashMap;

fn gpu_engines(rows: Vec<(String, f64)>) -> Vec<GpuEngine> {
    let mut engines: HashMap<String, (String, String, String, f64)> = HashMap::new();
    for (name, value) in rows {
        let Some((_, identity)) = name.split_once("_luid_") else {
            continue;
        };
        let Some((key, kind)) = identity.split_once("_engtype_") else {
            continue;
        };
        let Some((adapter, engine_index)) = key.split_once("_eng_") else {
            continue;
        };
        let entry = engines.entry(key.to_owned()).or_insert((
            adapter.to_owned(),
            kind.to_owned(),
            engine_index.to_owned(),
            0.0,
        ));
        entry.3 += value;
    }
    let mut result: Vec<_> = engines
        .into_values()
        .map(|(adapter, engine, engine_index, value)| GpuEngine {
            adapter,
            engine,
            engine_index,
            usage_percent: value,
        })
        .collect();
    result.sort_by(|a, b| {
        b.usage_percent
            .total_cmp(&a.usage_percent)
            .then_with(|| a.adapter.cmp(&b.adapter))
            .then_with(|| a.engine.cmp(&b.engine))
    });
    result.truncate(4);
    result
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::c_void,
        ptr,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW, PDH_CSTATUS_NEW_DATA, PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W,
        PDH_FMT_DOUBLE, PDH_MORE_DATA,
    };

    pub struct PerformanceCollector {
        query: usize,
        read: Option<usize>,
        write: Option<usize>,
        gpu: Option<usize>,
        ready: bool,
        last_retry: Instant,
    }

    fn add_counter(query: usize, path: &str) -> Option<usize> {
        if query == 0 {
            return None;
        }
        let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let mut counter = ptr::null_mut();
        (unsafe { PdhAddEnglishCounterW(query as *mut c_void, path.as_ptr(), 0, &mut counter) }
            == 0)
            .then_some(counter as usize)
    }

    impl PerformanceCollector {
        pub fn new() -> Self {
            let mut query = ptr::null_mut();
            if unsafe { PdhOpenQueryW(ptr::null(), 0, &mut query) } != 0 {
                query = ptr::null_mut();
            }
            let query = query as usize;
            Self {
                query,
                read: add_counter(query, "\\LogicalDisk(*)\\Disk Read Bytes/sec"),
                write: add_counter(query, "\\LogicalDisk(*)\\Disk Write Bytes/sec"),
                gpu: add_counter(query, "\\GPU Engine(*)\\Utilization Percentage"),
                ready: false,
                last_retry: Instant::now(),
            }
        }

        pub fn sample(&mut self, mounts: &[String]) -> PerformanceInfo {
            if self.last_retry.elapsed() >= Duration::from_secs(30) {
                if self.query == 0 {
                    *self = Self::new();
                } else {
                    if self.read.is_none() {
                        self.read =
                            add_counter(self.query, "\\LogicalDisk(*)\\Disk Read Bytes/sec");
                    }
                    if self.write.is_none() {
                        self.write =
                            add_counter(self.query, "\\LogicalDisk(*)\\Disk Write Bytes/sec");
                    }
                    if self.gpu.is_none() {
                        self.gpu =
                            add_counter(self.query, "\\GPU Engine(*)\\Utilization Percentage");
                    }
                    self.last_retry = Instant::now();
                }
            }
            let mut result = PerformanceInfo::default();
            if self.query == 0 || unsafe { PdhCollectQueryData(self.query as *mut c_void) } != 0 {
                self.ready = false;
                result.disk_status = CounterStatus::Unavailable;
                result.gpu_status = CounterStatus::Unavailable;
                return result;
            }
            if !self.ready {
                self.ready = true;
                return result;
            }
            let reads = self.read.and_then(read_array);
            let writes = self.write.and_then(read_array);
            if reads.is_some() || writes.is_some() {
                let reads: HashMap<_, _> = reads
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(n, v)| (n.to_ascii_lowercase(), v))
                    .collect();
                let writes: HashMap<_, _> = writes
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(n, v)| (n.to_ascii_lowercase(), v))
                    .collect();
                result.disks = mounts
                    .iter()
                    .map(|mount| {
                        let key = mount.trim_end_matches(['\\', '/']).to_ascii_lowercase();
                        DiskActivity {
                            mount_point: mount.clone(),
                            read_bytes_per_sec: reads.get(&key).copied(),
                            write_bytes_per_sec: writes.get(&key).copied(),
                        }
                    })
                    .collect();
                result.disk_status = if result
                    .disks
                    .iter()
                    .any(|d| d.read_bytes_per_sec.is_some() || d.write_bytes_per_sec.is_some())
                {
                    CounterStatus::Available
                } else {
                    CounterStatus::Unavailable
                };
            } else {
                result.disk_status = CounterStatus::Unavailable;
            }
            if let Some(rows) = self.gpu.and_then(read_array) {
                result.gpu_engines = gpu_engines(rows);
                result.gpu_status = if result.gpu_engines.is_empty() {
                    CounterStatus::Unavailable
                } else {
                    CounterStatus::Available
                };
            } else {
                result.gpu_status = CounterStatus::Unavailable;
            }
            result
        }
    }

    impl Drop for PerformanceCollector {
        fn drop(&mut self) {
            if self.query != 0 {
                unsafe {
                    PdhCloseQuery(self.query as *mut c_void);
                }
            }
        }
    }

    fn read_array(counter: usize) -> Option<Vec<(String, f64)>> {
        let mut bytes = 0;
        let mut count = 0;
        let status = unsafe {
            PdhGetFormattedCounterArrayW(
                counter as *mut c_void,
                PDH_FMT_DOUBLE,
                &mut bytes,
                &mut count,
                ptr::null_mut(),
            )
        };
        if status != PDH_MORE_DATA || bytes == 0 || bytes > 4 * 1024 * 1024 {
            return None;
        }
        // usize storage supplies the alignment required by PDH_FMT_COUNTERVALUE_ITEM_W.
        let mut buffer = vec![0usize; (bytes as usize).div_ceil(std::mem::size_of::<usize>())];
        let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        if unsafe {
            PdhGetFormattedCounterArrayW(
                counter as *mut c_void,
                PDH_FMT_DOUBLE,
                &mut bytes,
                &mut count,
                items,
            )
        } != 0
        {
            return None;
        }
        let mut rows = Vec::new();
        for item in unsafe { std::slice::from_raw_parts(items, count as usize) } {
            if item.szName.is_null()
                || !matches!(
                    item.FmtValue.CStatus,
                    PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA
                )
            {
                continue;
            }
            let value = unsafe { item.FmtValue.Anonymous.doubleValue };
            if !value.is_finite() || value < 0.0 {
                continue;
            }
            let mut len = 0;
            while len < 1024 && unsafe { *item.szName.add(len) } != 0 {
                len += 1;
            }
            if len == 1024 {
                continue;
            }
            let name =
                String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(item.szName, len) });
            rows.push((name, value));
        }
        Some(rows)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub struct PerformanceCollector;
    impl PerformanceCollector {
        pub fn new() -> Self {
            Self
        }
        pub fn sample(&mut self, _: &[String]) -> PerformanceInfo {
            PerformanceInfo {
                disk_status: CounterStatus::Unavailable,
                gpu_status: CounterStatus::Unavailable,
                ..Default::default()
            }
        }
    }
}

pub use platform::PerformanceCollector;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gpu_process_instances_are_combined_per_engine_not_across_engines() {
        let rows = vec![
            ("pid_1_luid_0x0_0x1_phys_0_eng_0_engtype_3D".into(), 30.0),
            ("pid_2_luid_0x0_0x1_phys_0_eng_0_engtype_3D".into(), 20.0),
            ("pid_1_luid_0x0_0x1_phys_0_eng_1_engtype_Copy".into(), 40.0),
        ];
        let result = gpu_engines(rows);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].usage_percent, 50.0);
        assert_eq!(result[1].usage_percent, 40.0);
    }
}
