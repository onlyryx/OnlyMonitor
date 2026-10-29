import type { HistoryEntry } from './bindings';
export type HistoryPoint = { time: number; cpu: number | null; memory?: number | null; download?: number | null; upload?: number | null };
export function isStale(sampledAtMs: number, now: number, intervalSecs: number): boolean {
  return now - sampledAtMs > Math.max(3500, 3 * intervalSecs * 1000);
}
export function toChartPoints(history: HistoryEntry[], selectedInterface: string | null): HistoryPoint[] {
  const points: HistoryPoint[] = [];
  const empty = (time: number): HistoryPoint => ({ time, cpu: null, memory: null, download: null, upload: null });
  for (const [index, entry] of history.entries()) {
    const previous = history[index - 1];
    if (previous && entry.sampledAtMs - previous.sampledAtMs > 2.5 * Math.max(previous.intervalSecs, entry.intervalSecs) * 1000) {
      points.push(empty(previous.sampledAtMs + 1));
    }
    const snapshot = entry.snapshot;
    if (!snapshot) { points.push(empty(entry.sampledAtMs)); continue; }
    const networks = selectedInterface == null ? snapshot.networks : snapshot.networks.filter(net => net.name === selectedInterface);
    const sum = (key: 'downloadBytesPerSec' | 'uploadBytesPerSec') => networks.length && networks.every(net => net[key] != null) ? networks.reduce((total, net) => total + net[key]!, 0) : null;
    points.push({ time: entry.sampledAtMs, cpu: snapshot.cpu.usagePercent, memory: snapshot.memory.usagePercent,
      download: sum('downloadBytesPerSec'), upload: sum('uploadBytesPerSec') });
  }
  return points;
}
export function formatBytes(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value) || value < 0) return '—';
  if (value === 0) return '0 B';
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];
  const power = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1);
  return `${(value / 1024 ** power).toFixed(power === 0 ? 0 : 1)} ${units[power]}`;
}
