import { expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke, isTauri: () => true }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
import { readAtaDeviceStats, readDiskHealth, readGpuTemperatures } from './api';

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

it('shares concurrent device reads and starts a fresh read after completion', async () => {
  let resolve!: (value: unknown) => void;
  invoke.mockImplementationOnce(() => new Promise(done => { resolve = done; }));
  const first = readGpuTemperatures();
  expect(readGpuTemperatures()).toBe(first);
  expect(invoke).toHaveBeenCalledTimes(1);
  resolve({ sampledAtMs: 1, devices: [] });
  await first;
  invoke.mockResolvedValueOnce({ sampledAtMs: 2, devices: [] });
  await readGpuTemperatures();
  expect(invoke).toHaveBeenCalledTimes(2);

  invoke.mockResolvedValueOnce({ sampledAtMs: 3, disks: [] });
  const disk = readDiskHealth();
  expect(readDiskHealth()).toBe(disk);
  await disk;
  expect(invoke).toHaveBeenCalledTimes(3);
});

it('serializes SATA statistics after a running disk health read', async () => {
  invoke.mockReset();
  const health = deferred<unknown>();
  const stats = deferred<unknown>();
  invoke.mockImplementation((command: string) => command === 'read_disk_health' ? health.promise : stats.promise);

  const healthRead = readDiskHealth(new AbortController().signal);
  const statsRead = readAtaDeviceStats();
  const callsBeforeHealthCompletes = invoke.mock.calls.map(([command]) => command);

  health.resolve({ sampledAtMs: 1, disks: [] });
  await healthRead;
  await new Promise(resolve => setTimeout(resolve, 0));
  stats.resolve({ sampledAtMs: 2, disks: [] });
  await statsRead;
  expect(callsBeforeHealthCompletes).toEqual(['read_disk_health']);
  expect(invoke.mock.calls.map(([command]) => command)).toEqual(['read_disk_health', 'read_ata_device_stats']);
});

it('removes a queued automatic health read when its caller is hidden or unmounted', async () => {
  invoke.mockReset();
  const stats = deferred<unknown>();
  invoke.mockReturnValueOnce(stats.promise);

  const statsRead = readAtaDeviceStats();
  const controller = new AbortController();
  const healthRead = readDiskHealth(controller.signal);
  controller.abort();
  await expect(healthRead).rejects.toBeInstanceOf(Error);

  stats.resolve({ sampledAtMs: 2, disks: [] });
  await statsRead;
  await new Promise(resolve => setTimeout(resolve, 0));
  expect(invoke.mock.calls.map(([command]) => command)).toEqual(['read_ata_device_stats']);
});
