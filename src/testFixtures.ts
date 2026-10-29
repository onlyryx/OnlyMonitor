import type { MonitorState, Snapshot } from './bindings';

export function sample(at = Date.now()): Snapshot {
  return { sampledAtMs: at, disksSampledAtMs: at, hostname: 'Test', os: 'Windows',
    cpu: { brand: 'CPU', logicalCores: 1, usagePercent: 20, cores: [{ name: 'cpu0', usagePercent: 20 }] },
    memory: { totalBytes: 100, usedBytes: 50, usagePercent: 50 }, disks: [], networks: [], performance: { diskStatus: 'initializing', disks: [], gpuStatus: 'initializing', gpuEngines: [] }, warnings: [] };
}
export function monitorState(): MonitorState {
  const latest = sample();
  return { latest, history: [{ sampledAtMs: latest.sampledAtMs, intervalSecs: 1, snapshot: latest }], processSnapshot: null, loadEvents: [], processError: null, alertError: null,
    lastError: null, lastAttemptAtMs: latest.sampledAtMs, configError: null, trayError: null, windowVisible: true,
    settings: { schemaVersion: 3, samplingIntervalSecs: 1, closeBehavior: 'hideToTray', selectedNetworkInterface: null, hideOnAutostart: true, highLoadAlertEnabled: false } };
}
