use crate::model::{PingOutcome, PingSample};
use std::net::{Ipv4Addr, SocketAddr, ToSocketAddrs};
#[cfg(windows)]
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(windows)]
use std::sync::Mutex;

const MAX_DOMAIN_NAME_LEN: usize = 253;
const MAX_PRESENTATION_NAME_LEN: usize = MAX_DOMAIN_NAME_LEN + 1;
const PAYLOAD: [u8; 32] = [0; 32];
const PING_TIMEOUT_MS: u32 = 1_000;
const SLEEP_TOLERANCE_MS: u64 = 100;
const SLEEP_ERROR: &str = "检测到系统休眠或计时中断，探测已停止，请重新开始";

#[cfg(windows)]
static DNS_BUSY: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static PING_BUSY: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static SESSION_GENERATION: AtomicU64 = AtomicU64::new(0);
#[cfg(windows)]
static SESSION: Mutex<PingSession> = Mutex::new(PingSession {
    generation: 0,
    target_ip: None,
    last_sample: None,
});

#[cfg(windows)]
#[derive(Clone, Copy)]
struct ClockSnapshot {
    tick_ms: u64,
    unbiased_100ns: u64,
}

#[cfg(windows)]
struct PingSession {
    generation: u64,
    target_ip: Option<Ipv4Addr>,
    last_sample: Option<ClockSnapshot>,
}

#[cfg(windows)]
fn sleep_gap_ms(previous: ClockSnapshot, current: ClockSnapshot) -> Option<u64> {
    let tick_elapsed = current.tick_ms.checked_sub(previous.tick_ms)?;
    let unbiased_elapsed_ms = current
        .unbiased_100ns
        .checked_sub(previous.unbiased_100ns)?
        / 10_000;
    Some(tick_elapsed.saturating_sub(unbiased_elapsed_ms))
}

#[cfg(windows)]
fn contains_sleep(previous: ClockSnapshot, current: ClockSnapshot) -> Result<bool, String> {
    sleep_gap_ms(previous, current)
        .map(|gap| gap > SLEEP_TOLERANCE_MS)
        .ok_or_else(|| "系统计时器发生回退".to_string())
}

#[cfg(windows)]
fn next_session_generation() -> u64 {
    SESSION_GENERATION
        .fetch_add(1, Ordering::AcqRel)
        .wrapping_add(1)
}

#[cfg(windows)]
fn is_current_generation(generation: u64) -> bool {
    SESSION_GENERATION.load(Ordering::Acquire) == generation
}

#[cfg(windows)]
fn start_session(generation: u64) -> Result<(), String> {
    let mut session = SESSION
        .lock()
        .map_err(|_| "Ping 会话状态不可用".to_string())?;
    if !is_current_generation(generation) {
        return Err("目标解析已被新的请求取代".into());
    }
    session.generation = generation;
    session.target_ip = None;
    session.last_sample = None;
    Ok(())
}

#[cfg(windows)]
fn set_resolved_session(
    generation: u64,
    target_ip: Ipv4Addr,
    sample: ClockSnapshot,
) -> Result<(), String> {
    let mut session = SESSION
        .lock()
        .map_err(|_| "Ping 会话状态不可用".to_string())?;
    if !is_current_generation(generation) || session.generation != generation {
        return Err("目标解析已被新的请求取代".into());
    }
    session.target_ip = Some(target_ip);
    session.last_sample = Some(sample);
    Ok(())
}

#[cfg(windows)]
fn set_session_baseline(generation: u64, sample: ClockSnapshot) -> Result<(), String> {
    let mut session = SESSION
        .lock()
        .map_err(|_| "Ping 会话状态不可用".to_string())?;
    if !is_current_generation(generation) || session.generation != generation {
        return Err("目标解析已被新的请求取代".into());
    }
    session.last_sample = Some(sample);
    Ok(())
}

#[cfg(windows)]
fn invalidate_session(generation: u64) {
    if let Ok(mut session) = SESSION.lock() {
        if is_current_generation(generation) && session.generation == generation {
            session.last_sample = None;
        }
    }
}

#[cfg(windows)]
fn session_baseline(target_ip: Ipv4Addr) -> Result<(u64, ClockSnapshot), String> {
    let session = SESSION
        .lock()
        .map_err(|_| "Ping 会话状态不可用".to_string())?;
    if !is_current_generation(session.generation) {
        return Err("Ping 会话已被新的目标解析取代".into());
    }
    if session.target_ip != Some(target_ip) {
        return Err("Ping 目标与当前解析会话不匹配".into());
    }
    let baseline = session
        .last_sample
        .ok_or_else(|| "请先成功解析目标，再开始 Ping".to_string())?;
    Ok((session.generation, baseline))
}

#[derive(Debug, PartialEq, Eq)]
enum TargetInput {
    Ipv4(Ipv4Addr),
    Domain(String),
}

#[cfg(windows)]
struct OperationGuard<'a>(&'a AtomicBool);

#[cfg(windows)]
impl<'a> OperationGuard<'a> {
    fn try_acquire(busy: &'a AtomicBool) -> Option<Self> {
        busy.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| Self(busy))
    }
}

#[cfg(windows)]
impl Drop for OperationGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn validate_unicast_ipv4(ip: Ipv4Addr) -> Result<Ipv4Addr, String> {
    if ip.is_unspecified() || ip.is_multicast() || ip == Ipv4Addr::BROADCAST {
        return Err(format!("不支持广播、组播或未指定地址：{ip}"));
    }
    Ok(ip)
}

fn valid_domain_name(target: &str) -> bool {
    let name = target.strip_suffix('.').unwrap_or(target);
    if name.is_empty() || name.len() > MAX_DOMAIN_NAME_LEN {
        return false;
    }

    name.split('.').all(|label| {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        let mut chars = label.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        first.is_alphanumeric()
            && label.chars().last().is_some_and(char::is_alphanumeric)
            && label
                .chars()
                .all(|character| character.is_alphanumeric() || character == '-')
    })
}

fn validate_target(target: &str) -> Result<TargetInput, String> {
    if target.is_empty() {
        return Err("目标不能为空".into());
    }
    if target.len() > MAX_PRESENTATION_NAME_LEN {
        return Err("目标名称过长".into());
    }
    if target.trim() != target {
        return Err("目标不能包含首尾空格".into());
    }

    if let Ok(ip) = target.parse::<Ipv4Addr>() {
        return validate_unicast_ipv4(ip).map(TargetInput::Ipv4);
    }
    if target.parse::<std::net::Ipv6Addr>().is_ok() {
        return Err("仅支持 IPv4 地址和域名".into());
    }
    if target.contains(':') || target.contains('/') || target.contains('?') || target.contains('#')
    {
        return Err("请输入 IPv4 地址或域名，不要包含端口或 URL".into());
    }
    if target
        .chars()
        .all(|character| character.is_ascii_digit() || character == '.')
    {
        return Err("无效的 IPv4 地址".into());
    }
    if !valid_domain_name(target) {
        return Err("无效的域名".into());
    }

    Ok(TargetInput::Domain(target.to_owned()))
}

fn select_ipv4(addresses: impl IntoIterator<Item = SocketAddr>) -> Option<Ipv4Addr> {
    addresses
        .into_iter()
        .find_map(|address| match address.ip() {
            std::net::IpAddr::V4(ip) if validate_unicast_ipv4(ip).is_ok() => Some(ip),
            _ => None,
        })
}

fn resolve_target(target: &str) -> Result<Ipv4Addr, String> {
    match validate_target(target)? {
        TargetInput::Ipv4(ip) => Ok(ip),
        TargetInput::Domain(domain) => {
            let addresses = (domain.as_str(), 0)
                .to_socket_addrs()
                .map_err(|error| format!("无法解析域名 {domain}：{error}"))?;
            select_ipv4(addresses).ok_or_else(|| format!("域名 {domain} 没有可用的 IPv4 单播地址"))
        }
    }
}

fn ip_addr_to_u32(ip: Ipv4Addr) -> u32 {
    u32::from_ne_bytes(ip.octets())
}

#[cfg(windows)]
use std::ffi::c_void;
#[cfg(windows)]
use std::mem::{align_of, size_of};
#[cfg(windows)]
use std::ptr;
#[cfg(windows)]
use std::time::{SystemTime, UNIX_EPOCH};
#[cfg(windows)]
use windows_sys::Win32::Foundation::{GetLastError, INVALID_HANDLE_VALUE};
#[cfg(windows)]
use windows_sys::Win32::NetworkManagement::IpHelper::{
    IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY, IP_BAD_ROUTE,
    IP_DEST_HOST_UNREACHABLE, IP_DEST_NET_UNREACHABLE, IP_DEST_PORT_UNREACHABLE,
    IP_DEST_PROT_UNREACHABLE, IP_DEST_SCOPE_MISMATCH, IP_DEST_UNREACHABLE, IP_GENERAL_FAILURE,
    IP_ICMP_ERROR, IP_PACKET_TOO_BIG, IP_REQ_TIMED_OUT, IP_STATUS_BASE, IP_TIME_EXCEEDED,
    IP_TTL_EXPIRED_REASSEM, IP_TTL_EXPIRED_TRANSIT,
};
#[cfg(windows)]
use windows_sys::Win32::System::SystemInformation::GetTickCount64;
#[cfg(windows)]
use windows_sys::Win32::System::WindowsProgramming::QueryUnbiasedInterruptTime;

#[cfg(windows)]
impl ClockSnapshot {
    fn read() -> Result<Self, String> {
        let tick_ms = unsafe { GetTickCount64() };
        let mut unbiased_100ns = 0;
        if unsafe { QueryUnbiasedInterruptTime(&mut unbiased_100ns) } == 0 {
            let error = unsafe { GetLastError() };
            return Err(format!("无法读取系统计时器，Windows 错误码 {error}"));
        }
        Ok(Self {
            tick_ms,
            unbiased_100ns,
        })
    }
}

#[cfg(windows)]
type IcmpEchoReply = ICMP_ECHO_REPLY;

#[cfg(windows)]
struct IcmpHandle(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for IcmpHandle {
    fn drop(&mut self) {
        unsafe {
            IcmpCloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
fn classify_status(status: u32) -> Result<PingOutcome, String> {
    match status {
        0 => Ok(PingOutcome::Reply),
        IP_REQ_TIMED_OUT => Ok(PingOutcome::Timeout),
        IP_DEST_NET_UNREACHABLE
        | IP_DEST_HOST_UNREACHABLE
        | IP_DEST_PROT_UNREACHABLE
        | IP_DEST_PORT_UNREACHABLE
        | IP_DEST_UNREACHABLE
        | IP_DEST_SCOPE_MISMATCH
        | IP_BAD_ROUTE => Ok(PingOutcome::Unreachable),
        IP_PACKET_TOO_BIG
        | IP_TTL_EXPIRED_TRANSIT
        | IP_TTL_EXPIRED_REASSEM
        | IP_TIME_EXCEEDED
        | IP_ICMP_ERROR => Ok(PingOutcome::Other),
        _ => Err(format!("ICMP API 返回本地或未知 IP 状态码 {status}")),
    }
}

#[cfg(windows)]
fn classify_zero_return(error: u32) -> Result<PingOutcome, String> {
    if !(IP_STATUS_BASE..=IP_GENERAL_FAILURE).contains(&error) {
        return Err(format!("ICMP API 失败，Windows 错误码 {error}"));
    }
    classify_status(error)
}

#[cfg(windows)]
fn status_detail(status: u32) -> Option<String> {
    Some(match status {
        IP_REQ_TIMED_OUT => "请求超时".into(),
        IP_DEST_NET_UNREACHABLE => "目标网络不可达".into(),
        IP_DEST_HOST_UNREACHABLE => "目标主机不可达".into(),
        IP_DEST_PROT_UNREACHABLE => "目标协议不可达".into(),
        IP_DEST_PORT_UNREACHABLE => "目标端口不可达".into(),
        IP_DEST_UNREACHABLE => "目标不可达".into(),
        IP_DEST_SCOPE_MISMATCH => "目标地址范围不匹配".into(),
        IP_BAD_ROUTE => "没有可用路由".into(),
        IP_PACKET_TOO_BIG => "数据包超过网络路径允许的大小".into(),
        IP_TTL_EXPIRED_TRANSIT => "数据包转发时 TTL 已过期".into(),
        IP_TTL_EXPIRED_REASSEM => "数据包重组时 TTL 已过期".into(),
        IP_TIME_EXCEEDED => "网络转发时间已超过限制".into(),
        IP_ICMP_ERROR => "收到 ICMP 错误响应".into(),
        _ => format!("IP 状态码 {status}"),
    })
}

#[cfg(windows)]
fn sampled_at_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "系统时钟早于 Unix 纪元".to_string())
        .and_then(|duration| {
            u64::try_from(duration.as_millis()).map_err(|_| "系统时间超出可表示范围".to_string())
        })
}

#[cfg(windows)]
fn sample(
    outcome: PingOutcome,
    rtt_ms: Option<u32>,
    detail: Option<String>,
) -> Result<PingSample, String> {
    Ok(PingSample {
        sampled_at_ms: sampled_at_ms()?,
        outcome,
        rtt_ms,
        detail,
    })
}

#[cfg(windows)]
fn aligned_reply_buffer(size: usize) -> Vec<usize> {
    vec![0; size.div_ceil(size_of::<usize>())]
}

#[cfg(windows)]
fn reply_data_is_in_bounds(
    buffer: *const u8,
    buffer_size: usize,
    header_size: usize,
    data: *const c_void,
    data_size: usize,
) -> bool {
    let base = buffer as usize;
    let Some(min_data) = base.checked_add(header_size) else {
        return false;
    };
    let Some(end) = base.checked_add(buffer_size) else {
        return false;
    };
    let data = data as usize;
    data >= min_data
        && data
            .checked_add(data_size)
            .is_some_and(|data_end| data_end <= end)
}

#[cfg(windows)]
enum PingAttemptError {
    Sleep,
    SessionChanged,
    Failure(String),
}

#[cfg(windows)]
fn ping_ipv4_attempt(
    destination: Ipv4Addr,
    baseline: ClockSnapshot,
    generation: Option<u64>,
) -> Result<(Result<PingSample, String>, ClockSnapshot), PingAttemptError> {
    let destination = validate_unicast_ipv4(destination).map_err(PingAttemptError::Failure)?;
    let handle = unsafe { IcmpCreateFile() };
    if handle == INVALID_HANDLE_VALUE {
        let error = unsafe { GetLastError() };
        return Err(PingAttemptError::Failure(format!(
            "无法创建 ICMP 句柄，Windows 错误码 {error}"
        )));
    }
    let _handle = IcmpHandle(handle);

    let reply_size = size_of::<IcmpEchoReply>() + PAYLOAD.len() + 8;
    let mut reply_buffer = aligned_reply_buffer(reply_size);
    let reply_ptr = reply_buffer.as_mut_ptr().cast::<c_void>();
    let before_send = ClockSnapshot::read().map_err(PingAttemptError::Failure)?;
    if contains_sleep(baseline, before_send).map_err(PingAttemptError::Failure)? {
        return Err(PingAttemptError::Sleep);
    }
    if generation.is_some_and(|value| !is_current_generation(value)) {
        return Err(PingAttemptError::SessionChanged);
    }

    let reply_count = unsafe {
        IcmpSendEcho(
            handle,
            ip_addr_to_u32(destination),
            PAYLOAD.as_ptr().cast::<c_void>(),
            PAYLOAD.len() as u16,
            ptr::null(),
            reply_ptr,
            reply_size as u32,
            PING_TIMEOUT_MS,
        )
    };
    let api_error = if reply_count == 0 {
        unsafe { GetLastError() }
    } else {
        0
    };
    let after_send = ClockSnapshot::read().map_err(PingAttemptError::Failure)?;
    if contains_sleep(before_send, after_send).map_err(PingAttemptError::Failure)? {
        return Err(PingAttemptError::Sleep);
    }

    if reply_count == 0 {
        let outcome = classify_zero_return(api_error);
        return Ok((
            outcome.and_then(|outcome| sample(outcome, None, status_detail(api_error))),
            after_send,
        ));
    }

    if reply_count != 1 {
        return Ok((Err("ICMP 回复数量或长度超出缓冲区".into()), after_send));
    }
    if !(reply_buffer.as_ptr() as usize).is_multiple_of(align_of::<IcmpEchoReply>()) {
        return Ok((Err("ICMP 回复缓冲区未正确对齐".into()), after_send));
    }

    let reply = unsafe { ptr::read(reply_buffer.as_ptr().cast::<IcmpEchoReply>()) };
    let result = classify_status(reply.Status).and_then(|outcome| {
        if outcome == PingOutcome::Reply {
            if reply.DataSize as usize != PAYLOAD.len()
                || !reply_data_is_in_bounds(
                    reply_buffer.as_ptr().cast(),
                    reply_size,
                    size_of::<IcmpEchoReply>(),
                    reply.Data,
                    reply.DataSize as usize,
                )
            {
                return Err("ICMP 回复数据长度无效".into());
            }
            sample(outcome, Some(reply.RoundTripTime), None)
        } else {
            sample(outcome, None, status_detail(reply.Status))
        }
    });
    Ok((result, after_send))
}

#[cfg(all(test, windows))]
fn ping_ipv4(destination: Ipv4Addr) -> Result<PingSample, String> {
    let baseline = ClockSnapshot::read()?;
    match ping_ipv4_attempt(destination, baseline, None) {
        Ok((result, _)) => result,
        Err(PingAttemptError::Sleep) => Err(SLEEP_ERROR.into()),
        Err(PingAttemptError::SessionChanged) => Err("Ping 会话已重启".into()),
        Err(PingAttemptError::Failure(error)) => Err(error),
    }
}

#[tauri::command]
pub async fn resolve_ping_target(target: String) -> Result<String, String> {
    #[cfg(not(windows))]
    {
        let _ = target;
        Err("Ping 仅支持 Windows".into())
    }
    #[cfg(windows)]
    {
        let domain = match validate_target(&target)? {
            TargetInput::Ipv4(ip) => {
                let generation = next_session_generation();
                start_session(generation)?;
                set_resolved_session(generation, ip, ClockSnapshot::read()?)?;
                return Ok(ip.to_string());
            }
            TargetInput::Domain(domain) => domain,
        };
        let guard = OperationGuard::try_acquire(&DNS_BUSY)
            .ok_or_else(|| "已有目标解析任务正在运行".to_string())?;
        let generation = next_session_generation();
        start_session(generation)?;
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = guard;
            let before_dns = ClockSnapshot::read()?;
            if !is_current_generation(generation) {
                return Err("目标解析已被新的请求取代".into());
            }
            let ip = resolve_target(&domain)?;
            let after_dns = ClockSnapshot::read()?;
            if !is_current_generation(generation) {
                return Err("目标解析已被新的请求取代".into());
            }
            if contains_sleep(before_dns, after_dns)? {
                invalidate_session(generation);
                return Err(SLEEP_ERROR.into());
            }
            set_resolved_session(generation, ip, after_dns)?;
            Ok(ip.to_string())
        })
        .await
        .map_err(|error| format!("目标解析任务失败：{error}"))?
    }
}

#[tauri::command]
pub async fn ping_once(ip: String) -> Result<PingSample, String> {
    #[cfg(not(windows))]
    {
        let _ = ip;
        Err("Ping 仅支持 Windows".into())
    }
    #[cfg(windows)]
    {
        let destination = validate_unicast_ipv4(
            ip.parse::<Ipv4Addr>()
                .map_err(|_| "目标必须是有效的 IPv4 地址".to_string())?,
        )?;
        let guard = OperationGuard::try_acquire(&PING_BUSY)
            .ok_or_else(|| "已有 Ping 请求正在运行".to_string())?;
        let (generation, baseline) = session_baseline(destination)?;
        tauri::async_runtime::spawn_blocking(move || {
            let _guard = guard;
            match ping_ipv4_attempt(destination, baseline, Some(generation)) {
                Ok((sample, after_ping)) => {
                    set_session_baseline(generation, after_ping)?;
                    sample
                }
                Err(PingAttemptError::Sleep) => {
                    invalidate_session(generation);
                    Err(SLEEP_ERROR.into())
                }
                Err(PingAttemptError::SessionChanged) => {
                    Err("Ping 会话已重启，旧探测结果已丢弃".into())
                }
                Err(PingAttemptError::Failure(error)) => match ClockSnapshot::read() {
                    Ok(now) => match contains_sleep(baseline, now) {
                        Ok(true) => {
                            invalidate_session(generation);
                            Err(SLEEP_ERROR.into())
                        }
                        Ok(false) => {
                            set_session_baseline(generation, now)?;
                            Err(error)
                        }
                        Err(clock_error) => {
                            invalidate_session(generation);
                            Err(clock_error)
                        }
                    },
                    Err(clock_error) => {
                        invalidate_session(generation);
                        Err(clock_error)
                    }
                },
            }
        })
        .await
        .map_err(|error| format!("Ping 任务失败：{error}"))?
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::sync::atomic::AtomicBool;
    use windows_sys::Win32::NetworkManagement::IpHelper::{IP_BAD_OPTION, IP_NO_RESOURCES};

    fn clock(tick_ms: u64, unbiased_ms: u64) -> ClockSnapshot {
        ClockSnapshot {
            tick_ms,
            unbiased_100ns: unbiased_ms * 10_000,
        }
    }

    #[test]
    fn sleep_gap_ignores_clock_skew_and_sub_threshold_pause() {
        let baseline = clock(1_000, 1_000);
        let normal_clock_skew = clock(2_000, 1_995);
        assert_eq!(sleep_gap_ms(baseline, normal_clock_skew), Some(5));
        assert_eq!(contains_sleep(baseline, normal_clock_skew), Ok(false));

        let short_pause = clock(2_075, 1_995);
        assert_eq!(sleep_gap_ms(baseline, short_pause), Some(80));
        assert_eq!(contains_sleep(baseline, short_pause), Ok(false));

        let at_tolerance = clock(2_095, 1_995);
        assert_eq!(sleep_gap_ms(baseline, at_tolerance), Some(100));
        assert_eq!(contains_sleep(baseline, at_tolerance), Ok(false));
    }

    #[test]
    fn sleep_gap_detects_a_pause_over_the_tolerance() {
        let baseline = clock(1_000, 1_000);
        let paused = clock(2_130, 2_000);
        assert_eq!(sleep_gap_ms(baseline, paused), Some(130));
        assert_eq!(contains_sleep(baseline, paused), Ok(true));
    }

    #[test]
    fn sleep_gap_rejects_backward_clock_values() {
        assert_eq!(sleep_gap_ms(clock(1, 1), clock(0, 2)), None);
    }

    #[test]
    fn accepts_ipv4_and_domain_inputs() {
        assert_eq!(
            validate_target("192.0.2.7"),
            Ok(TargetInput::Ipv4(Ipv4Addr::new(192, 0, 2, 7)))
        );
        assert_eq!(
            validate_target("example.com"),
            Ok(TargetInput::Domain("example.com".into()))
        );
    }

    #[test]
    fn rejects_invalid_or_non_unicast_targets() {
        for target in [
            "",
            "example.com:443",
            "https://example.com",
            "::1",
            "255.255.255.255",
            "224.0.0.1",
            "0.0.0.0",
            "a..example",
        ] {
            assert!(validate_target(target).is_err(), "accepted {target:?}");
        }
        assert!(validate_target(&"a".repeat(254)).is_err());
    }

    #[test]
    fn picks_ipv4_from_system_resolution_and_rejects_ipv6_only() {
        let addresses = [
            SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 9)), 0),
        ];
        assert_eq!(select_ipv4(addresses), Some(Ipv4Addr::new(192, 0, 2, 9)));
        assert_eq!(
            select_ipv4([SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), 0)]),
            None
        );
    }

    #[test]
    fn converts_ipv4_to_windows_network_order() {
        let ip = Ipv4Addr::new(192, 0, 2, 7);
        assert_eq!(ip_addr_to_u32(ip).to_ne_bytes(), ip.octets());
    }

    #[test]
    fn classifies_timeout_and_unreachable_statuses() {
        assert_eq!(classify_status(IP_REQ_TIMED_OUT), Ok(PingOutcome::Timeout));
        assert_eq!(
            classify_status(IP_DEST_NET_UNREACHABLE),
            Ok(PingOutcome::Unreachable)
        );
        assert_eq!(
            classify_status(IP_TTL_EXPIRED_TRANSIT),
            Ok(PingOutcome::Other)
        );
        assert_eq!(classify_status(0), Ok(PingOutcome::Reply));
    }

    #[test]
    fn distinguishes_ip_statuses_from_local_api_errors() {
        assert_eq!(
            classify_zero_return(IP_REQ_TIMED_OUT),
            Ok(PingOutcome::Timeout)
        );
        assert_eq!(
            classify_zero_return(IP_DEST_HOST_UNREACHABLE),
            Ok(PingOutcome::Unreachable)
        );
        assert!(classify_zero_return(IP_NO_RESOURCES).is_err());
        assert!(classify_zero_return(IP_BAD_OPTION).is_err());
        assert!(classify_zero_return(IP_GENERAL_FAILURE).is_err());
        assert!(classify_zero_return(11001).is_err());
        assert!(classify_zero_return(0).is_err());
        assert!(classify_zero_return(87).is_err());
    }

    #[test]
    fn operation_guard_rejects_a_second_concurrent_request() {
        let busy = AtomicBool::new(false);
        let first = OperationGuard::try_acquire(&busy).unwrap();
        assert!(OperationGuard::try_acquire(&busy).is_none());
        drop(first);
        assert!(OperationGuard::try_acquire(&busy).is_some());
    }

    #[test]
    fn newer_session_rejects_stale_resolution_and_old_target() {
        let old = next_session_generation();
        start_session(old).unwrap();
        set_resolved_session(old, Ipv4Addr::new(192, 0, 2, 1), clock(1000, 1000)).unwrap();
        let new = next_session_generation();
        start_session(new).unwrap();
        set_resolved_session(new, Ipv4Addr::LOCALHOST, clock(2000, 2000)).unwrap();
        assert!(set_resolved_session(old, Ipv4Addr::new(192, 0, 2, 1), clock(3000, 3000)).is_err());
        assert!(session_baseline(Ipv4Addr::new(192, 0, 2, 1)).is_err());
        assert_eq!(session_baseline(Ipv4Addr::LOCALHOST).unwrap().0, new);
    }

    #[test]
    fn reply_buffer_has_required_alignment_and_capacity() {
        let required = size_of::<IcmpEchoReply>() + PAYLOAD.len() + 8;
        let buffer = aligned_reply_buffer(required);
        assert_eq!((buffer.as_ptr() as usize) % align_of::<IcmpEchoReply>(), 0);
        assert!(buffer.len() * size_of::<usize>() >= required);
    }

    #[test]
    fn native_reply_layout_matches_the_windows_sdk_definition() {
        let expected = if size_of::<usize>() == 8 { 40 } else { 28 };
        assert_eq!(size_of::<IcmpEchoReply>(), expected);
    }

    #[test]
    fn reply_payload_must_fit_inside_the_reply_buffer() {
        let buffer = [0u8; 80];
        assert!(reply_data_is_in_bounds(
            buffer.as_ptr(),
            buffer.len(),
            40,
            buffer[40..].as_ptr().cast(),
            32,
        ));
        assert!(!reply_data_is_in_bounds(
            buffer.as_ptr(),
            buffer.len(),
            40,
            buffer[56..].as_ptr().cast(),
            32,
        ));
    }

    #[test]
    #[ignore = "explicit loopback-only ICMP smoke test; never part of cargo test by default"]
    fn loopback_ping_smoke() {
        let sample = ping_ipv4(Ipv4Addr::LOCALHOST).unwrap();
        assert_eq!(sample.outcome, PingOutcome::Reply);
        assert!(sample.rtt_ms.is_some());
    }

    #[test]
    #[ignore = "explicit loopback-only command smoke test; never part of cargo test by default"]
    fn loopback_commands_smoke() {
        tauri::async_runtime::block_on(async {
            let ip = resolve_ping_target("127.0.0.1".into()).await.unwrap();
            let sample = ping_once(ip).await.unwrap();
            assert_eq!(sample.outcome, PingOutcome::Reply);
            assert!(sample.rtt_ms.is_some());
        });
    }
}
