use crate::model::{
    AtaDeviceStats, AtaDeviceStatsDisk, AtaDeviceStatsReport, DiskHealthReport, DiskHealthStatus,
    GpuTemperatureReport, NvmeHealth, SataSmartAttribute,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static GPU_READING: AtomicBool = AtomicBool::new(false);
static DISK_READING: AtomicBool = AtomicBool::new(false);

struct ReadGuard(&'static AtomicBool);
impl Drop for ReadGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

async fn guarded<T: Send + 'static>(
    flag: &'static AtomicBool,
    read: fn() -> Result<T, String>,
) -> Result<T, String> {
    if flag
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err("上一次读取仍在进行中，请稍后再试".into());
    }
    let guard = ReadGuard(flag);
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        read()
    })
    .await
    .map_err(|error| format!("读取任务失败：{error}"))?
}

fn timestamp() -> Result<u64, String> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("系统时钟无效：{error}"))?
        .as_millis() as u64)
}

fn health_status(value: Option<u16>) -> DiskHealthStatus {
    match value {
        Some(0) => DiskHealthStatus::Normal,
        Some(1) => DiskHealthStatus::Warning,
        Some(2) => DiskHealthStatus::Unhealthy,
        _ => DiskHealthStatus::Unknown,
    }
}

fn parse_nvme_health(log: &[u8]) -> Result<NvmeHealth, String> {
    if log.len() < 512 {
        return Err("NVMe 健康日志不完整".into());
    }
    if log[3] > 100 || log[4] > 100 {
        return Err("NVMe 健康日志包含无效的备用空间数值".into());
    }
    let kelvin = u16::from_le_bytes([log[1], log[2]]);
    let celsius = i32::from(kelvin) - 273;
    let counter = |offset: usize| {
        u128::from_le_bytes(log[offset..offset + 16].try_into().unwrap()).to_string()
    };
    Ok(NvmeHealth {
        critical_warning: log[0],
        temperature_celsius: (-50..=200).contains(&celsius).then_some(celsius),
        available_spare: log[3],
        spare_threshold: log[4],
        percentage_used: log[5],
        power_cycles: counter(112),
        power_on_hours: counter(128),
        unsafe_shutdowns: counter(144),
        media_errors: counter(160),
    })
}

fn parse_storage_temperature(data: &[u8]) -> Result<i16, String> {
    if data.len() < 40 {
        return Err("磁盘温度数据不完整".into());
    }
    let size = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
    let count = u16::from_le_bytes(data[12..14].try_into().unwrap()) as usize;
    if count == 0 || size > data.len() || size < 24 + count * 16 {
        return Err("磁盘温度数据不完整".into());
    }
    let value = i16::from_le_bytes([data[26], data[27]]);
    if !(-50..=150).contains(&value) {
        return Err("磁盘温度不可用".into());
    }
    Ok(value)
}

fn parse_failure_prediction(data: &[u8]) -> Result<bool, String> {
    if data.len() < 516 {
        return Err("故障预测数据不完整".into());
    }
    Ok(u32::from_le_bytes(data[..4].try_into().unwrap()) != 0)
}

fn validate_ata_stat_header(data: &[u8], page: u8) -> Result<(), String> {
    if data.len() != 512 {
        return Err("ATA 设备统计日志格式不受支持".into());
    }
    let revision = u16::from_le_bytes([data[0], data[1]]);
    if !(revision == 1 || (page == 1 && revision == 2)) || data[2] != page {
        return Err("ATA 设备统计日志格式不受支持".into());
    }
    Ok(())
}

fn parse_ata_supported_pages(data: &[u8]) -> Result<(bool, bool), String> {
    validate_ata_stat_header(data, 0)?;
    let count = usize::from(data[8]);
    if count == 0 || !data[9..9 + count].contains(&0) {
        return Err("ATA 设备统计页面目录无效".into());
    }
    let pages = &data[9..9 + count];
    Ok((pages.contains(&1), pages.contains(&4)))
}

fn parse_ata_stat_page(
    data: &[u8],
    page: u8,
    offsets: &[usize],
) -> Result<Vec<Option<String>>, String> {
    validate_ata_stat_header(data, page)?;
    offsets
        .iter()
        .map(|&offset| {
            let item = data
                .get(offset..offset + 8)
                .ok_or("ATA 设备统计字段不完整")?;
            if item[7] & 0xe0 != 0xc0 || item[4..7] != [0, 0, 0] {
                return Ok(None);
            }
            Ok(Some(
                u32::from_le_bytes(item[..4].try_into().unwrap()).to_string(),
            ))
        })
        .collect()
}

fn parse_ata_protocol_response(data: &[u8]) -> Result<&[u8], String> {
    if data.len() < 560 {
        return Err("ATA 日志响应不完整".into());
    }
    let word = |offset| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    if word(0) != 48
        || word(4) != 48
        || word(8) != 2
        || word(12) != 2
        || word(24) != 40
        || word(28) != 512
    {
        return Err("ATA 日志响应格式不受支持".into());
    }
    Ok(&data[48..560])
}

fn parse_ata_log_directory(data: &[u8]) -> Result<u16, String> {
    if data.len() != 512 || u16::from_le_bytes([data[0], data[1]]) != 1 {
        return Err("ATA 日志目录格式不受支持".into());
    }
    let count = u16::from_le_bytes([data[8], data[9]]);
    if count == 0 {
        return Err("设备未报告 ATA 设备统计日志".into());
    }
    Ok(count)
}

fn parse_sata_attributes(data: &[u8]) -> Result<Vec<SataSmartAttribute>, String> {
    if data.len() != 512
        || data.iter().fold(0u8, |sum, value| sum.wrapping_add(*value)) != 0
        || u16::from_le_bytes([data[0], data[1]]) == 0
    {
        return Err("设备未提供可校验的 SATA 属性数据".into());
    }
    let mut attributes = Vec::new();
    for item in data[2..362].as_chunks::<12>().0 {
        if item[0] == 0 {
            continue;
        }
        if attributes
            .iter()
            .any(|attribute: &SataSmartAttribute| attribute.id == item[0])
        {
            return Err("SATA 属性编号重复".into());
        }
        attributes.push(SataSmartAttribute {
            id: item[0],
            current: item[3],
            worst: item[4],
            raw_hex: item[5..11]
                .iter()
                .rev()
                .map(|value| format!("{value:02X}"))
                .collect(),
        });
    }
    if attributes.is_empty() {
        return Err("设备未提供 SATA 属性".into());
    }
    Ok(attributes)
}

fn comparable_disk_identifier(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty()
        || !value
            .chars()
            .any(|character| character.is_ascii_alphanumeric() && character != '0')
    {
        return None;
    }
    Some(value.to_ascii_uppercase())
}

fn unique_disk_number<'a>(
    target: Option<&str>,
    physical_values: impl Iterator<Item = Option<&'a str>>,
    os_values: impl Iterator<Item = (Option<u32>, Option<&'a str>)>,
) -> Option<u32> {
    let target = comparable_disk_identifier(target)?;
    let matches_identifier =
        |value: Option<&str>| comparable_disk_identifier(value).as_deref() == Some(target.as_str());
    if physical_values
        .filter(|value| matches_identifier(*value))
        .count()
        != 1
    {
        return None;
    }
    let matches: Vec<_> = os_values
        .filter(|(_, value)| matches_identifier(*value))
        .map(|(number, _)| number)
        .collect();
    if matches.len() == 1 {
        matches[0]
    } else {
        None
    }
}

#[tauri::command]
pub async fn read_gpu_temperatures() -> Result<GpuTemperatureReport, String> {
    guarded(&GPU_READING, platform::gpu_temperatures).await
}

#[tauri::command]
pub async fn read_disk_health() -> Result<DiskHealthReport, String> {
    guarded(&DISK_READING, platform::disk_health).await
}

#[tauri::command]
pub async fn read_ata_device_stats() -> Result<AtaDeviceStatsReport, String> {
    guarded(&DISK_READING, platform::ata_device_stats).await
}

#[cfg(windows)]
mod platform {
    use super::*;
    use crate::model::{GpuTemperature, PhysicalDiskHealth};
    use serde::Deserialize;
    use std::{
        ffi::{c_char, c_void, CStr, OsString},
        os::windows::ffi::OsStringExt,
        path::PathBuf,
        ptr,
    };
    use windows_sys::Win32::System::{
        Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
        SystemInformation::GetSystemDirectoryW,
        IO::DeviceIoControl,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING},
    };

    fn system_nvml_path() -> Result<PathBuf, String> {
        let mut buffer = vec![0u16; 32768];
        let length =
            unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
        if length == 0 || length >= buffer.len() {
            return Err("无法定位 Windows 系统目录".into());
        }
        buffer.truncate(length);
        Ok(PathBuf::from(OsString::from_wide(&buffer)).join("nvml.dll"))
    }

    type Init = unsafe extern "C" fn() -> u32;
    type Shutdown = unsafe extern "C" fn() -> u32;
    type Count = unsafe extern "C" fn(*mut u32) -> u32;
    type Handle = unsafe extern "C" fn(u32, *mut *mut c_void) -> u32;
    type Name = unsafe extern "C" fn(*mut c_void, *mut c_char, u32) -> u32;
    type Temperature = unsafe extern "C" fn(*mut c_void, u32, *mut u32) -> u32;

    struct NvmlGuard(Shutdown);
    impl Drop for NvmlGuard {
        fn drop(&mut self) {
            unsafe {
                (self.0)();
            }
        }
    }

    fn nvml_error(code: u32) -> String {
        match code {
            3 => "该设备不支持读取温度".into(),
            4 => "驱动拒绝读取温度".into(),
            9 => "NVIDIA 驱动未加载".into(),
            15 => "显卡当前不可访问".into(),
            _ => format!("驱动读取失败（代码 {code}）"),
        }
    }

    pub fn gpu_temperatures() -> Result<GpuTemperatureReport, String> {
        let path = system_nvml_path()?;
        // The absolute Windows system directory prevents loading a DLL from the working directory.
        let library = unsafe { libloading::Library::new(path) }
            .map_err(|_| "系统中没有可用的 NVIDIA 温度接口；其他厂商暂不支持".to_string())?;
        unsafe {
            let init: Init = *library
                .get(b"nvmlInit_v2\0")
                .map_err(|_| "NVIDIA 驱动接口不兼容")?;
            let shutdown: Shutdown = *library
                .get(b"nvmlShutdown\0")
                .map_err(|_| "NVIDIA 驱动接口不兼容")?;
            let count: Count = *library
                .get(b"nvmlDeviceGetCount_v2\0")
                .map_err(|_| "NVIDIA 驱动接口不兼容")?;
            let handle: Handle = *library
                .get(b"nvmlDeviceGetHandleByIndex_v2\0")
                .map_err(|_| "NVIDIA 驱动接口不兼容")?;
            let name: Name = *library
                .get(b"nvmlDeviceGetName\0")
                .map_err(|_| "NVIDIA 驱动接口不兼容")?;
            let temperature: Temperature = *library
                .get(b"nvmlDeviceGetTemperature\0")
                .map_err(|_| "NVIDIA 驱动接口不兼容")?;
            let code = init();
            if code != 0 {
                return Err(nvml_error(code));
            }
            let _guard = NvmlGuard(shutdown);
            let mut total = 0;
            let code = count(&mut total);
            if code != 0 {
                return Err(nvml_error(code));
            }
            if total == 0 {
                return Err("未发现可读取的 NVIDIA 显卡".into());
            }
            if total > 32 {
                return Err("显卡数量超出当前读取上限".into());
            }
            let mut devices = Vec::new();
            for index in 0..total {
                let mut device = ptr::null_mut();
                let code = handle(index, &mut device);
                let mut device_name = format!("NVIDIA GPU {}", index + 1);
                if code != 0 || device.is_null() {
                    devices.push(GpuTemperature {
                        name: device_name,
                        celsius: None,
                        error: Some(if code == 0 {
                            "无法读取显卡".into()
                        } else {
                            nvml_error(code)
                        }),
                    });
                    continue;
                }
                let mut buffer = [0u8; 128];
                if name(device, buffer.as_mut_ptr().cast(), buffer.len() as u32) == 0 {
                    if let Ok(value) = CStr::from_bytes_until_nul(&buffer) {
                        let text = value.to_string_lossy().trim().to_owned();
                        if !text.is_empty() {
                            device_name = text;
                        }
                    }
                }
                let mut value = 0;
                let code = temperature(device, 0, &mut value);
                let (celsius, error) = if code != 0 {
                    (None, Some(nvml_error(code)))
                } else if value > 150 {
                    (None, Some("驱动返回的温度超出可显示范围".into()))
                } else {
                    (Some(value), None)
                };
                devices.push(GpuTemperature {
                    name: device_name,
                    celsius,
                    error,
                });
            }
            Ok(GpuTemperatureReport {
                sampled_at_ms: timestamp()?,
                devices,
            })
        }
    }

    struct ComGuard;
    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct WmiDisk {
        friendly_name: Option<String>,
        health_status: Option<u16>,
        bus_type: Option<u16>,
        unique_id: Option<String>,
        serial_number: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct WmiOsDisk {
        number: Option<u32>,
        unique_id: Option<String>,
        bus_type: Option<u16>,
        serial_number: Option<String>,
    }

    fn sata_disk_number(row: &WmiDisk, rows: &[WmiDisk], disks: &[WmiOsDisk]) -> Option<u32> {
        let physical = || rows.iter().filter(|disk| disk.bus_type == Some(11));
        let os = || disks.iter().filter(|disk| disk.bus_type == Some(11));
        unique_disk_number(
            row.unique_id.as_deref(),
            physical().map(|disk| disk.unique_id.as_deref()),
            os().map(|disk| (disk.number, disk.unique_id.as_deref())),
        )
        .or_else(|| {
            unique_disk_number(
                row.serial_number.as_deref(),
                physical().map(|disk| disk.serial_number.as_deref()),
                os().map(|disk| (disk.number, disk.serial_number.as_deref())),
            )
        })
    }

    struct DiskHandle(windows_sys::Win32::Foundation::HANDLE);
    impl Drop for DiskHandle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    fn ata_log_page(handle: &DiskHandle, address: u32, page: u32) -> Result<[u8; 512], String> {
        let mut buffer = [0u8; 560];
        for (index, value) in [50u32, 0, 2, 2, address, page, 40, 512, 0, 0, 0, 0]
            .iter()
            .enumerate()
        {
            buffer[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle.0,
                0x2d1400,
                buffer.as_ptr().cast(),
                buffer.len() as u32,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut returned,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err("设备或驱动未提供所需 ATA 日志页".into());
        }
        let data = &buffer[..(returned as usize).min(buffer.len())];
        Ok(parse_ata_protocol_response(data)?.try_into().unwrap())
    }

    fn ata_disk_stats(number: u32) -> Result<(AtaDeviceStats, Option<String>), String> {
        let path: Vec<u16> = format!(r"\\.\PhysicalDrive{number}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let raw = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if raw == INVALID_HANDLE_VALUE {
            return Err("无法访问磁盘设备统计".into());
        }
        let handle = DiskHandle(raw);
        let count = parse_ata_log_directory(&ata_log_page(&handle, 0, 0)?)?;
        let supported =
            ata_log_page(&handle, 4, 0).and_then(|page| parse_ata_supported_pages(&page))?;
        let mut stats = AtaDeviceStats {
            power_on_hours: None,
            power_cycles: None,
            reported_uncorrectable_errors: None,
        };
        let mut errors = Vec::new();
        if supported.0 {
            let result = if count > 1 {
                ata_log_page(&handle, 4, 1).and_then(|page| parse_ata_stat_page(&page, 1, &[8, 16]))
            } else {
                Err("设备统计页面超出日志目录范围".into())
            };
            match result {
                Ok(values) => {
                    stats.power_cycles = values[0].clone();
                    stats.power_on_hours = values[1].clone();
                }
                Err(error) => errors.push(format!("通用统计：{error}")),
            }
        }
        if supported.1 {
            let result = if count > 4 {
                ata_log_page(&handle, 4, 4).and_then(|page| parse_ata_stat_page(&page, 4, &[8]))
            } else {
                Err("设备统计页面超出日志目录范围".into())
            };
            match result {
                Ok(values) => stats.reported_uncorrectable_errors = values[0].clone(),
                Err(error) => errors.push(format!("错误统计：{error}")),
            }
        }
        if !supported.0 && !supported.1 {
            return Err("设备未提供所需统计页面".into());
        }
        Ok((stats, (!errors.is_empty()).then(|| errors.join("；"))))
    }

    fn sata_failure_prediction(
        number: u32,
    ) -> Result<(bool, Result<Vec<SataSmartAttribute>, String>), String> {
        let path: Vec<u16> = format!(r"\\.\PhysicalDrive{number}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err("无法访问磁盘故障预测".into());
        }
        let mut buffer = [0u8; 516];
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                0x2d1100,
                ptr::null(),
                0,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut returned,
                ptr::null_mut(),
            )
        };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            return Err("设备或驱动未提供故障预测".into());
        }
        let data = &buffer[..(returned as usize).min(buffer.len())];
        Ok((
            parse_failure_prediction(data)?,
            parse_sata_attributes(&data[4..]),
        ))
    }

    fn sata_temperature(number: u32) -> Result<i16, String> {
        let path: Vec<u16> = format!(r"\\.\PhysicalDrive{number}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err("无法访问磁盘温度".into());
        }
        let mut buffer = [0u8; 1024];
        buffer[..4].copy_from_slice(&52u32.to_le_bytes());
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                0x2d1400,
                buffer.as_ptr().cast(),
                buffer.len() as u32,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut returned,
                ptr::null_mut(),
            )
        };
        unsafe {
            CloseHandle(handle);
        }
        if ok == 0 {
            return Err("设备或驱动未提供磁盘温度".into());
        }
        parse_storage_temperature(&buffer[..(returned as usize).min(buffer.len())])
    }

    fn nvme_health(id: &str) -> Result<NvmeHealth, String> {
        let number: u32 = id.parse().map_err(|_| "无法识别物理磁盘编号")?;
        let path: Vec<u16> = format!(r"\\.\PhysicalDrive{number}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                0,
                ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return Err(format!(
                "无法访问 NVMe 健康日志：{}",
                std::io::Error::last_os_error()
            ));
        }
        let mut buffer = [0u8; 560];
        for (i, value) in [49u32, 0, 3, 2, 2, 0, 40, 512, 0, 0, 0, 0]
            .iter()
            .enumerate()
        {
            buffer[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                0x2d1400,
                buffer.as_ptr().cast(),
                buffer.len() as u32,
                buffer.as_mut_ptr().cast(),
                buffer.len() as u32,
                &mut returned,
                ptr::null_mut(),
            )
        };
        let error = if ok == 0 {
            Some(std::io::Error::last_os_error())
        } else {
            None
        };
        unsafe {
            CloseHandle(handle);
        }
        if let Some(error) = error {
            return Err(format!("NVMe 健康日志不可用：{error}"));
        }
        if returned < 560
            || u32::from_le_bytes(buffer[4..8].try_into().unwrap()) < 48
            || u32::from_le_bytes(buffer[24..28].try_into().unwrap()) != 40
            || u32::from_le_bytes(buffer[28..32].try_into().unwrap()) < 512
        {
            return Err("NVMe 健康日志格式不受支持".into());
        }
        parse_nvme_health(&buffer[48..560])
    }

    pub fn ata_device_stats() -> Result<AtaDeviceStatsReport, String> {
        let status = unsafe { CoInitializeEx(ptr::null(), COINIT_MULTITHREADED as u32) };
        if status < 0 {
            return Err("无法初始化 Windows 存储查询".into());
        }
        let _guard = ComGuard;
        let connection =
            wmi::WMIConnection::with_namespace_path("ROOT\\Microsoft\\Windows\\Storage")
                .map_err(|error| format!("无法连接 Windows 存储信息：{error}"))?;
        let rows: Vec<WmiDisk> = connection.raw_query(
            "SELECT FriendlyName, HealthStatus, UniqueId, SerialNumber, BusType FROM MSFT_PhysicalDisk",
        ).map_err(|error| format!("无法读取物理磁盘状态：{error}"))?;
        if rows.is_empty() {
            return Err("Windows 未提供可读取的物理磁盘状态".into());
        }
        if rows.len() > 128 {
            return Err("物理磁盘数量超出当前读取上限".into());
        }
        let os_disks: Vec<WmiOsDisk> = connection
            .raw_query("SELECT Number, UniqueId, SerialNumber, BusType FROM MSFT_Disk")
            .unwrap_or_default();
        let disks = rows
            .iter()
            .filter(|row| row.bus_type == Some(11))
            .map(|row| {
                let result = sata_disk_number(row, &rows, &os_disks)
                    .ok_or_else(|| "无法使用唯一标识或唯一序列号匹配磁盘编号".to_string())
                    .and_then(ata_disk_stats);
                let (stats, error) = match result {
                    Ok((stats, error)) => (Some(stats), error),
                    Err(error) => (None, Some(error)),
                };
                AtaDeviceStatsDisk {
                    name: row
                        .friendly_name
                        .clone()
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| "未命名磁盘".into()),
                    stats,
                    error,
                }
            })
            .collect();
        Ok(AtaDeviceStatsReport {
            sampled_at_ms: timestamp()?,
            disks,
        })
    }

    pub fn disk_health() -> Result<DiskHealthReport, String> {
        let status = unsafe { CoInitializeEx(ptr::null(), COINIT_MULTITHREADED as u32) };
        if status < 0 {
            return Err(format!(
                "无法初始化 Windows 存储查询（代码 0x{:08X}）",
                status as u32
            ));
        }
        let _guard = ComGuard;
        let connection =
            wmi::WMIConnection::with_namespace_path("ROOT\\Microsoft\\Windows\\Storage")
                .map_err(|error| format!("无法连接 Windows 存储信息：{error}"))?;
        let rows: Vec<WmiDisk> = connection
            .raw_query(
                "SELECT FriendlyName, HealthStatus, UniqueId, SerialNumber, BusType FROM MSFT_PhysicalDisk",
            )
            .map_err(|error| format!("无法读取物理磁盘状态：{error}"))?;
        if rows.is_empty() {
            return Err("Windows 未提供可读取的物理磁盘状态".into());
        }
        if rows.len() > 128 {
            return Err("物理磁盘数量超出当前读取上限".into());
        }
        let os_disks: Vec<WmiOsDisk> = connection
            .raw_query("SELECT Number, UniqueId, SerialNumber, BusType FROM MSFT_Disk")
            .unwrap_or_default();
        let physical_ids: Vec<_> = rows
            .iter()
            .filter_map(|row| row.unique_id.clone())
            .collect();
        let disks = rows
            .iter()
            .map(|row| {
                let sata_number = if row.bus_type == Some(11) {
                    let number = sata_disk_number(row, &rows, &os_disks);
                    Some(
                        number
                            .ok_or_else(|| "无法使用唯一标识或唯一序列号匹配磁盘编号".to_string()),
                    )
                } else {
                    None
                };
                let sata_temperature = sata_number
                    .as_ref()
                    .map(|number| number.clone().and_then(sata_temperature));
                let sata_prediction =
                    sata_number.map(|number| number.and_then(sata_failure_prediction));
                let (sata_temperature_celsius, sata_temperature_error) = match sata_temperature {
                    Some(Ok(value)) => (Some(value), None),
                    Some(Err(error)) => (None, Some(error)),
                    None => (None, None),
                };
                let (
                    sata_predict_failure,
                    sata_prediction_error,
                    sata_smart_attributes,
                    sata_smart_error,
                ) = match sata_prediction {
                    Some(Ok((value, Ok(attributes)))) => {
                        (Some(value), None, Some(attributes), None)
                    }
                    Some(Ok((value, Err(error)))) => (Some(value), None, None, Some(error)),
                    Some(Err(error)) => (None, Some(error), None, None),
                    None => (None, None, None, None),
                };
                let health = if row.bus_type == Some(17) {
                    let id = row
                        .unique_id
                        .as_deref()
                        .filter(|id| comparable_disk_identifier(Some(id)).is_some());
                    let matches: Vec<_> = os_disks
                        .iter()
                        .filter(|disk| {
                            disk.bus_type == Some(17)
                                && id.is_some()
                                && disk.unique_id.as_deref() == id
                        })
                        .collect();
                    Some(
                        if matches.len() == 1
                            && physical_ids
                                .iter()
                                .filter(|other| Some(other.as_str()) == id)
                                .count()
                                == 1
                        {
                            matches[0]
                                .number
                                .ok_or_else(|| "Windows 未提供磁盘编号".to_string())
                                .and_then(|number| nvme_health(&number.to_string()))
                        } else {
                            Err("无法可靠匹配 Windows 磁盘编号".to_string())
                        },
                    )
                } else {
                    None
                };
                let (nvme_health, nvme_error) = match health {
                    Some(Ok(value)) => (Some(value), None),
                    Some(Err(error)) => (None, Some(error)),
                    None => (None, None),
                };
                PhysicalDiskHealth {
                    name: row
                        .friendly_name
                        .clone()
                        .filter(|name| !name.trim().is_empty())
                        .unwrap_or_else(|| "未命名磁盘".into()),
                    status: health_status(row.health_status),
                    nvme_health,
                    nvme_error,
                    sata_temperature_celsius,
                    sata_temperature_error,
                    sata_predict_failure,
                    sata_prediction_error,
                    sata_smart_attributes,
                    sata_smart_error,
                }
            })
            .collect();
        Ok(DiskHealthReport {
            sampled_at_ms: timestamp()?,
            disks,
        })
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    pub fn gpu_temperatures() -> Result<GpuTemperatureReport, String> {
        Err("当前平台不支持读取 NVIDIA 温度".into())
    }
    pub fn disk_health() -> Result<DiskHealthReport, String> {
        Err("当前平台不支持读取 Windows 磁盘状态".into())
    }
    pub fn ata_device_stats() -> Result<AtaDeviceStatsReport, String> {
        Err("当前平台不支持读取 ATA 设备统计".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ata_device_statistics_validate_page_and_counter_flags() {
        let mut page = [0u8; 512];
        page[0] = 1;
        page[2] = 1;
        page[8..12].copy_from_slice(&12u32.to_le_bytes());
        page[15] = 0xc0;
        page[16..20].copy_from_slice(&0u32.to_le_bytes());
        page[23] = 0xc0;
        assert_eq!(
            parse_ata_stat_page(&page, 1, &[8, 16]).unwrap(),
            vec![Some("12".into()), Some("0".into())]
        );
        page[15] = 0x80;
        page[23] = 0xe0;
        assert_eq!(
            parse_ata_stat_page(&page, 1, &[8, 16]).unwrap(),
            vec![None, None]
        );
        page[15] = 0xc0;
        page[12] = 1;
        assert_eq!(parse_ata_stat_page(&page, 1, &[8]).unwrap(), vec![None]);
        assert!(parse_ata_stat_page(&page[..64], 1, &[8]).is_err());
        assert!(parse_ata_stat_page(&page, 4, &[8]).is_err());
        page[0] = 2;
        assert!(parse_ata_stat_page(&page, 1, &[8]).is_ok());
        page[0] = 3;
        assert!(parse_ata_stat_page(&page, 1, &[8]).is_err());
        page[0] = 0;
        assert!(parse_ata_stat_page(&page, 1, &[8]).is_err());
    }

    #[test]
    fn ata_device_statistics_supported_pages_require_valid_header() {
        let mut page = [0u8; 512];
        page[0] = 1;
        page[8] = 3;
        page[9..12].copy_from_slice(&[0, 1, 4]);
        assert_eq!(parse_ata_supported_pages(&page).unwrap(), (true, true));
        page[2] = 1;
        assert!(parse_ata_supported_pages(&page).is_err());
        assert!(parse_ata_supported_pages(&page[..20]).is_err());
    }

    #[test]
    fn ata_protocol_response_rejects_wrong_header_and_truncation() {
        let mut response = [0u8; 560];
        for (index, value) in [48u32, 48, 2, 2, 0, 0, 40, 512].iter().enumerate() {
            response[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        response[48] = 1;
        assert_eq!(parse_ata_protocol_response(&response).unwrap()[0], 1);
        assert!(parse_ata_protocol_response(&response[..559]).is_err());
        response[8] = 3;
        assert!(parse_ata_protocol_response(&response).is_err());
        response[8] = 2;
        response[24] = 0;
        assert!(parse_ata_protocol_response(&response).is_err());
        response[24] = 40;
        response[28..32].copy_from_slice(&513u32.to_le_bytes());
        assert!(parse_ata_protocol_response(&response).is_err());
    }

    #[test]
    fn ata_log_directory_requires_revision_and_declared_pages() {
        let mut directory = [0u8; 512];
        assert!(parse_ata_log_directory(&directory).is_err());
        directory[0] = 1;
        assert!(parse_ata_log_directory(&directory).is_err());
        directory[8] = 5;
        assert_eq!(parse_ata_log_directory(&directory).unwrap(), 5);
        assert!(parse_ata_log_directory(&directory[..16]).is_err());
    }
    #[test]
    fn sata_attributes_require_valid_checksum_and_preserve_raw_bytes() {
        let mut data = [0u8; 512];
        assert!(parse_sata_attributes(&data).is_err());
        data[0] = 10;
        data[2] = 5;
        data[5] = 100;
        data[6] = 99;
        data[7..13].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
        data[511] = 0u8.wrapping_sub(data.iter().fold(0u8, |sum, v| sum.wrapping_add(*v)));
        let attributes = parse_sata_attributes(&data).unwrap();
        assert_eq!(attributes[0].id, 5);
        assert_eq!(attributes[0].current, 100);
        assert_eq!(attributes[0].raw_hex, "060504030201");
        data[5] ^= 1;
        assert!(parse_sata_attributes(&data).is_err());
    }
    #[test]
    fn failure_prediction_requires_complete_result_and_preserves_nonzero() {
        assert!(parse_failure_prediction(&[0; 4]).is_err());
        let mut data = [0u8; 516];
        assert!(!parse_failure_prediction(&data).unwrap());
        data[0] = 2;
        assert!(parse_failure_prediction(&data).unwrap());
    }
    #[test]
    fn sata_disk_matching_falls_back_to_a_unique_serial_without_exposing_it() {
        let physical_ids = [Some("00000000")];
        let os_ids = [(Some(1), Some("00000000"))];
        let physical_serials = [Some("  TEST-SSD-1234  ")];
        let os_serials = [(Some(1), Some("TEST-SSD-1234"))];

        assert_eq!(
            unique_disk_number(
                physical_ids[0],
                physical_ids.into_iter(),
                os_ids.into_iter()
            ),
            None
        );
        assert_eq!(
            unique_disk_number(
                physical_serials[0],
                physical_serials.into_iter(),
                os_serials.into_iter()
            ),
            Some(1)
        );

        let duplicate_serials = [
            (Some(1), Some("TEST-SSD-1234")),
            (Some(2), Some("TEST-SSD-1234")),
        ];
        assert_eq!(
            unique_disk_number(
                Some("TEST-SSD-1234"),
                physical_serials.into_iter(),
                duplicate_serials.into_iter()
            ),
            None
        );
    }
    #[test]
    fn sata_disk_matching_returns_none_when_no_os_disk_matches() {
        let physical_serials = [Some("TEST-SSD-1234")];
        let no_os_disks: [(Option<u32>, Option<&str>); 0] = [];
        assert_eq!(
            unique_disk_number(
                physical_serials[0],
                physical_serials.into_iter(),
                no_os_disks.into_iter()
            ),
            None
        );
    }
    #[test]
    fn storage_temperature_rejects_missing_invalid_or_truncated_data() {
        let mut data = [0u8; 40];
        assert!(parse_storage_temperature(&data).is_err());
        data[4..8].copy_from_slice(&40u32.to_le_bytes());
        data[12..14].copy_from_slice(&1u16.to_le_bytes());
        data[26..28].copy_from_slice(&36i16.to_le_bytes());
        assert_eq!(parse_storage_temperature(&data).unwrap(), 36);
        data[12..14].copy_from_slice(&2u16.to_le_bytes());
        assert!(parse_storage_temperature(&data).is_err());
        data[12..14].copy_from_slice(&1u16.to_le_bytes());
        data[4..8].copy_from_slice(&56u32.to_le_bytes());
        assert!(parse_storage_temperature(&data).is_err());
        data[4..8].copy_from_slice(&40u32.to_le_bytes());
        let mut multiple = [0u8; 56];
        multiple[..40].copy_from_slice(&data);
        multiple[4..8].copy_from_slice(&56u32.to_le_bytes());
        multiple[12..14].copy_from_slice(&2u16.to_le_bytes());
        assert_eq!(parse_storage_temperature(&multiple).unwrap(), 36);
        data[26..28].copy_from_slice(&i16::MIN.to_le_bytes());
        assert!(parse_storage_temperature(&data).is_err());
        assert!(parse_storage_temperature(&data[..30]).is_err());
    }
    #[test]
    fn nvme_health_log_preserves_warnings_and_rejects_short_data() {
        assert!(parse_nvme_health(&[0; 10]).is_err());
        let mut log = [0u8; 512];
        log[0] = 4;
        log[1..3].copy_from_slice(&329u16.to_le_bytes());
        log[3] = 100;
        log[4] = 10;
        log[5] = 26;
        log[112..128].copy_from_slice(&1234u128.to_le_bytes());
        log[128..144].copy_from_slice(&9876u128.to_le_bytes());
        log[144..160].copy_from_slice(&7u128.to_le_bytes());
        log[160..176].copy_from_slice(&u128::MAX.to_le_bytes());
        let health = parse_nvme_health(&log).unwrap();
        assert_eq!(health.critical_warning, 4);
        assert_eq!(health.temperature_celsius, Some(56));
        assert_eq!(health.available_spare, 100);
        assert_eq!(health.percentage_used, 26);
        assert_eq!(health.power_cycles, "1234");
        assert_eq!(health.power_on_hours, "9876");
        assert_eq!(health.unsafe_shutdowns, "7");
        assert_eq!(health.media_errors, u128::MAX.to_string());
        log[3] = 255;
        assert!(parse_nvme_health(&log).is_err());
    }
    #[test]
    fn unknown_disk_health_never_becomes_normal() {
        assert_eq!(health_status(Some(0)), DiskHealthStatus::Normal);
        assert_eq!(health_status(Some(1)), DiskHealthStatus::Warning);
        assert_eq!(health_status(Some(2)), DiskHealthStatus::Unhealthy);
        assert_eq!(health_status(Some(5)), DiskHealthStatus::Unknown);
        assert_eq!(health_status(None), DiskHealthStatus::Unknown);
        assert_eq!(health_status(Some(99)), DiskHealthStatus::Unknown);
    }
}
