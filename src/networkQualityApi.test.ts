import { beforeEach, expect, it, vi } from 'vitest';

const runtime = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: true }));
vi.mock('@tauri-apps/api/core', () => ({ invoke: runtime.invoke, isTauri: () => runtime.isTauri }));
import { pingOnce, resolvePingTarget } from './api';

beforeEach(() => { runtime.isTauri = true; runtime.invoke.mockReset(); });

it('sends only the explicit host and resolved IP to the Ping commands', async () => {
  const response = { sampledAtMs: 12, outcome: 'timeout', rttMs: null, detail: null } as const;
  runtime.invoke.mockResolvedValueOnce('192.0.2.7').mockResolvedValueOnce(response);

  await expect(resolvePingTarget('example.test')).resolves.toBe('192.0.2.7');
  await expect(pingOnce('192.0.2.7')).resolves.toEqual(response);

  expect(runtime.invoke).toHaveBeenNthCalledWith(1, 'resolve_ping_target', { target: 'example.test' });
  expect(runtime.invoke).toHaveBeenNthCalledWith(2, 'ping_once', { ip: '192.0.2.7' });
});

it('waits for an in-flight Ping before starting the next page session', async () => {
  const response = { sampledAtMs: 12, outcome: 'timeout', rttMs: null, detail: null } as const;
  let finishFirst!: (value: unknown) => void;
  runtime.invoke.mockImplementationOnce(() => new Promise(resolve => { finishFirst = resolve; }))
    .mockResolvedValueOnce(response);

  const first = pingOnce('192.0.2.7');
  await Promise.resolve();
  const onWaiting = vi.fn();
  const restartedPage = pingOnce('192.0.2.8', { onWaiting });

  expect(runtime.invoke).toHaveBeenCalledTimes(1);
  expect(onWaiting).toHaveBeenCalledOnce();
  finishFirst(response);
  await expect(first).resolves.toEqual(response);
  await expect(restartedPage).resolves.toEqual(response);
  expect(runtime.invoke).toHaveBeenCalledTimes(2);
  expect(runtime.invoke).toHaveBeenNthCalledWith(2, 'ping_once', { ip: '192.0.2.8' });
});

it('does not start a queued Ping after its session stops', async () => {
  const response = { sampledAtMs: 12, outcome: 'timeout', rttMs: null, detail: null } as const;
  let finishFirst!: (value: unknown) => void;
  runtime.invoke.mockImplementationOnce(() => new Promise(resolve => { finishFirst = resolve; }));
  const first = pingOnce('192.0.2.7');
  await Promise.resolve();
  const controller = new AbortController();
  const queued = pingOnce('192.0.2.8', { signal: controller.signal });

  controller.abort();
  await expect(queued).rejects.toThrow('网络探测已取消');
  finishFirst(response);

  await expect(first).resolves.toEqual(response);
  expect(runtime.invoke).toHaveBeenCalledTimes(1);
});

it('queues another DNS lookup but lets a direct IPv4 target bypass a stuck lookup', async () => {
  let finishDns!: (value: string) => void;
  runtime.invoke.mockImplementationOnce(() => new Promise(resolve => { finishDns = resolve; }))
    .mockResolvedValueOnce('192.0.2.8')
    .mockResolvedValueOnce('192.0.2.9');

  const first = resolvePingTarget('slow.example');
  await Promise.resolve();
  const queuedDomain = resolvePingTarget('next.example');
  const directIp = resolvePingTarget('192.0.2.8');

  await expect(directIp).resolves.toBe('192.0.2.8');
  expect(runtime.invoke).toHaveBeenCalledTimes(2);
  expect(runtime.invoke).toHaveBeenNthCalledWith(2, 'resolve_ping_target', { target: '192.0.2.8' });
  finishDns('192.0.2.7');
  await expect(first).resolves.toBe('192.0.2.7');
  await expect(queuedDomain).resolves.toBe('192.0.2.9');
  expect(runtime.invoke).toHaveBeenNthCalledWith(3, 'resolve_ping_target', { target: 'next.example' });
});

it('removes cancelled DNS lookups before the active resolver completes', async () => {
  let finishDns!: (value: string) => void;
  runtime.invoke.mockImplementationOnce(() => new Promise(resolve => { finishDns = resolve; }))
    .mockResolvedValueOnce('192.0.2.9');

  const first = resolvePingTarget('slow.example');
  await Promise.resolve();
  const controller = new AbortController();
  const queued = resolvePingTarget('stale.example', { signal: controller.signal });
  controller.abort();

  await expect(queued).rejects.toThrow('网络探测已取消');
  expect(runtime.invoke).toHaveBeenCalledTimes(1);
  finishDns('192.0.2.7');
  await expect(first).resolves.toBe('192.0.2.7');
  await expect(resolvePingTarget('fresh.example')).resolves.toBe('192.0.2.9');
  expect(runtime.invoke).toHaveBeenNthCalledWith(2, 'resolve_ping_target', { target: 'fresh.example' });
});

it('starts the next Ping after the current one rejects', async () => {
  const response = { sampledAtMs: 12, outcome: 'timeout', rttMs: null, detail: null } as const;
  runtime.invoke.mockRejectedValueOnce(new Error('native request failed')).mockResolvedValueOnce(response);

  const first = pingOnce('192.0.2.7');
  const next = pingOnce('192.0.2.8');

  await expect(first).rejects.toThrow('native request failed');
  await expect(next).resolves.toEqual(response);
  expect(runtime.invoke).toHaveBeenCalledTimes(2);
});

it('explains that real Ping is unavailable in browser preview', async () => {
  runtime.isTauri = false;

  await expect(resolvePingTarget('example.test')).rejects.toThrow('浏览器预览');
  await expect(pingOnce('192.0.2.7')).rejects.toThrow('浏览器预览');
  expect(runtime.invoke).not.toHaveBeenCalled();
});
