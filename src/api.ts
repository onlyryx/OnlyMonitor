import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { AtaDeviceStatsReport, DiskHealthReport, GpuTemperatureReport, MonitorState, PingSample, Settings, StartupStatus, ThermalReport } from './bindings';

let pending: Promise<MonitorState> | null = null;
let pendingGpu: Promise<GpuTemperatureReport> | null = null;
let pendingDiskHealth: { signal?: AbortSignal; promise: Promise<DiskHealthReport> } | null = null;
let pendingAtaDeviceStats: Promise<AtaDeviceStatsReport> | null = null;
type QueuedOperation = { start: () => void };
type OperationQueue = { running: boolean; waiting: Set<QueuedOperation> };
type ProbeRequestOptions = { signal?: AbortSignal; onWaiting?: () => void; cancelMessage?: string };
const pingQueue: OperationQueue = { running: false, waiting: new Set() };
const dnsQueue: OperationQueue = { running: false, waiting: new Set() };
const diskQueue: OperationQueue = { running: false, waiting: new Set() };

function runNext(queue: OperationQueue) {
  const next = queue.waiting.values().next().value as QueuedOperation | undefined;
  if (!next) { queue.running = false; return; }
  queue.waiting.delete(next);
  queue.running = true;
  next.start();
}

function runAfter<T>(queue: OperationQueue, { signal, onWaiting, cancelMessage = '请求已取消' }: ProbeRequestOptions, operation: () => Promise<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) { reject(new Error(cancelMessage)); return; }
    let entry: QueuedOperation;
    const onAbort = () => {
      if (queue.waiting.delete(entry)) {
        signal?.removeEventListener('abort', onAbort);
        reject(new Error(cancelMessage));
      }
    };
    entry = {
      start: () => {
        signal?.removeEventListener('abort', onAbort);
        if (signal?.aborted) {
          reject(new Error(cancelMessage));
          queue.running = false;
          runNext(queue);
          return;
        }
        let request: Promise<T>;
        try { request = operation(); }
        catch (reason) {
          reject(reason);
          queue.running = false;
          runNext(queue);
          return;
        }
        void request.then(resolve, reject).finally(() => {
          queue.running = false;
          runNext(queue);
        });
      },
    };
    queue.waiting.add(entry);
    signal?.addEventListener('abort', onAbort, { once: true });
    if (queue.running) onWaiting?.();
    else runNext(queue);
  });
}

export function getMonitorState(): Promise<MonitorState> {
  if (!isTauri()) return Promise.reject(new Error('当前为浏览器预览。请通过 Tauri 桌面应用读取真实系统指标。'));
  pending ??= invoke<MonitorState>('get_monitor_state').finally(() => { pending = null; });
  return pending;
}

export function resolvePingTarget(target: string, options: ProbeRequestOptions = {}): Promise<string> {
  if (!isTauri()) return Promise.reject(new Error('当前为浏览器预览。Ping 仅在 Tauri 桌面应用中可用。'));
  const resolve = () => invoke<string>('resolve_ping_target', { target });
  return /^\d+(?:\.\d+){3}$/.test(target)
    ? options.signal?.aborted ? Promise.reject(new Error('网络探测已取消')) : resolve()
    : runAfter(dnsQueue, { ...options, cancelMessage: '网络探测已取消' }, resolve);
}

export function pingOnce(ip: string, options: ProbeRequestOptions = {}): Promise<PingSample> {
  if (!isTauri()) return Promise.reject(new Error('当前为浏览器预览。Ping 仅在 Tauri 桌面应用中可用。'));
  return runAfter(pingQueue, { ...options, cancelMessage: '网络探测已取消' }, () => invoke<PingSample>('ping_once', { ip }));
}

export function saveSettings(settings: Settings): Promise<Settings> {
  return invoke<Settings>('save_settings', { settings });
}
export function clearLoadEvents(): Promise<void> {
  return invoke<void>('clear_load_events');
}
export function readThermalZones(): Promise<ThermalReport> {
  if (!isTauri()) return Promise.reject(new Error('请在桌面应用中读取温度'));
  return invoke<ThermalReport>('read_thermal_zones');
}
export function readGpuTemperatures(): Promise<GpuTemperatureReport> {
  if (!isTauri()) return Promise.reject(new Error('请在桌面应用中读取显卡温度'));
  pendingGpu ??= invoke<GpuTemperatureReport>('read_gpu_temperatures').finally(() => { pendingGpu = null; });
  return pendingGpu;
}
export function readDiskHealth(signal?: AbortSignal): Promise<DiskHealthReport> {
  if (!isTauri()) return Promise.reject(new Error('请在桌面应用中读取磁盘状态'));
  if (pendingDiskHealth && pendingDiskHealth.signal === signal) return pendingDiskHealth.promise;
  const promise = runAfter(diskQueue, { signal, cancelMessage: '磁盘读取已取消' }, () => invoke<DiskHealthReport>('read_disk_health')).finally(() => {
    if (pendingDiskHealth?.promise === promise) pendingDiskHealth = null;
  });
  pendingDiskHealth = { signal, promise };
  return promise;
}
export function readAtaDeviceStats(): Promise<AtaDeviceStatsReport> {
  if (!isTauri()) return Promise.reject(new Error('请在桌面应用中读取 SATA 设备统计'));
  pendingAtaDeviceStats ??= runAfter(diskQueue, {}, () => invoke<AtaDeviceStatsReport>('read_ata_device_stats')).finally(() => { pendingAtaDeviceStats = null; });
  return pendingAtaDeviceStats;
}
export function getStartupStatus(): Promise<StartupStatus> {
  if (!isTauri()) return Promise.reject(new Error('请在桌面应用中读取登录启动状态'));
  return invoke<StartupStatus>('get_startup_status');
}
export function setAutostart(enabled: boolean): Promise<StartupStatus> {
  return invoke<StartupStatus>('set_autostart', { enabled });
}
export function listenVisibility(handler: (visible: boolean) => void): Promise<() => void> {
  if (!isTauri()) return Promise.resolve(() => {});
  return listen<boolean>('monitor-visibility-changed', event => handler(event.payload));
}
