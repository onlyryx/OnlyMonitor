import { describe, expect, it } from 'vitest';
import { formatBytes, isStale, toChartPoints } from './history';
import { sample } from './testFixtures';

describe('bounded monitor history', () => {
  it('scales stale thresholds with the configured interval', () => {
    expect(isStale(0, 10000, 5)).toBe(false);
    expect(isStale(0, 15001, 5)).toBe(true);
    expect(isStale(0, 3501, 1)).toBe(true);
  });
  it('preserves missing interfaces and errors as null, and breaks sampling gaps', () => {
    const value = sample(1000);
    value.networks = [{ name: 'Ethernet', receivedBytes: 100, transmittedBytes: 50, downloadBytesPerSec: 12, uploadBytesPerSec: null }];
    const history = [{ sampledAtMs: 1000, intervalSecs: 1, snapshot: value }, { sampledAtMs: 5000, intervalSecs: 1, snapshot: null }];
    const points = toChartPoints(history, 'Ethernet');
    expect(points[0]).toMatchObject({ time: 1000, cpu: 20, memory: 50, download: 12, upload: null });
    expect(points.at(-1)?.cpu).toBeNull();
    expect(points).toHaveLength(3);
    expect(toChartPoints(history, 'Removed')[0].download).toBeNull();
  });
  it('formats zero and binary byte units', () => {
    expect(formatBytes(0)).toBe('0 B');
    expect(formatBytes(1024)).toBe('1.0 KiB');
    expect(formatBytes(null)).toBe('—');
  });
});
