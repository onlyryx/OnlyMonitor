import { act, renderHook, cleanup } from '@testing-library/react';
import { StrictMode } from 'react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { MonitorState } from './bindings';
import { useMonitor } from './useMonitor';
import * as api from './api';
import { monitorState } from './testFixtures';

let visibility: (visible: boolean) => void;
const unlisten = vi.fn();
beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(api, 'listenVisibility').mockImplementation(async handler => { visibility = handler; return unlisten; });
});
afterEach(() => { cleanup(); vi.useRealTimers(); vi.clearAllMocks(); });

it('does not overlap slow reads or schedule work after unmount', async () => {
  let resolve!: (value: MonitorState) => void;
  const request = vi.spyOn(api, 'getMonitorState').mockImplementation(() => new Promise(r => { resolve = r; }));
  const hook = renderHook(() => useMonitor());
  await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
  expect(request).toHaveBeenCalledTimes(1);
  hook.unmount();
  await act(async () => { resolve(monitorState()); await vi.advanceTimersByTimeAsync(5000); });
  expect(request).toHaveBeenCalledTimes(1);
  expect(unlisten).toHaveBeenCalledTimes(1);
});

it('reports collection transport errors then recovers', async () => {
  const state = monitorState();
  vi.spyOn(api, 'getMonitorState').mockRejectedValueOnce(new Error('采集失败')).mockResolvedValue(state);
  const { result } = renderHook(() => useMonitor());
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  expect(result.current.error).toBe('采集失败');
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(result.current.error).toBeNull();
  expect(result.current.snapshot).toBe(state.latest);
  expect(result.current.history).toHaveLength(1);
});

it('stops reads and the UI clock while hidden, then immediately fetches backend history', async () => {
  const request = vi.spyOn(api, 'getMonitorState').mockResolvedValue(monitorState());
  const { result } = renderHook(() => useMonitor());
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  act(() => visibility(false));
  const hiddenAt = result.current.now;
  await act(async () => { await vi.advanceTimersByTimeAsync(5000); });
  expect(request).toHaveBeenCalledTimes(1);
  expect(result.current.now).toBe(hiddenAt);
  await act(async () => { visibility(true); await vi.advanceTimersByTimeAsync(0); });
  expect(request).toHaveBeenCalledTimes(2);
});

it('cleans asynchronous listeners under StrictMode', async () => {
  const request = vi.spyOn(api, 'getMonitorState').mockResolvedValue(monitorState());
  const hook = renderHook(() => useMonitor(), { wrapper: StrictMode });
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  expect(request).toHaveBeenCalledTimes(1);
  hook.unmount();
  expect(unlisten).toHaveBeenCalledTimes(2);
});

it('uses one shared save gate and immediately publishes returned effective settings', async () => {
  const base = monitorState();
  const read = vi.spyOn(api, 'getMonitorState').mockResolvedValue(base);
  let resolveSave!: (value: typeof base.settings) => void;
  const save = vi.spyOn(api, 'saveSettings').mockImplementation(() => new Promise(r => { resolveSave = r; }));
  const { result } = renderHook(() => useMonitor());
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  let pending!: Promise<MonitorState['settings']>;
  act(() => { pending = result.current.saveSettings({ ...base.settings, samplingIntervalSecs: 5 }); });
  await expect(result.current.saveSettings({ ...base.settings, selectedNetworkInterface: 'Ethernet' })).rejects.toThrow('正在保存');
  expect(save).toHaveBeenCalledTimes(1);
  read.mockResolvedValue({ ...base, settings: { ...base.settings, samplingIntervalSecs: 5 } });
  await act(async () => { resolveSave({ ...base.settings, samplingIntervalSecs: 5 }); await pending; });
  expect(result.current.state?.settings.samplingIntervalSecs).toBe(5);
});

it('does not let a pre-save read roll back effective settings', async () => {
  const base = monitorState();
  let resolveRead!: (value: MonitorState) => void;
  vi.spyOn(api, 'getMonitorState').mockResolvedValueOnce(base)
    .mockImplementationOnce(() => new Promise(r => { resolveRead = r; }))
    .mockImplementation(() => new Promise(() => {}));
  vi.spyOn(api, 'saveSettings').mockResolvedValue({ ...base.settings, samplingIntervalSecs: 5 });
  const { result } = renderHook(() => useMonitor());
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  await act(async () => { await result.current.saveSettings({ ...base.settings, samplingIntervalSecs: 5 }); });
  await act(async () => { resolveRead(base); });
  expect(result.current.state?.settings.samplingIntervalSecs).toBe(5);
});

it('shares the gate with startup changes without submitting or replacing settings', async () => {
  const base = monitorState();
  vi.spyOn(api, 'getMonitorState').mockResolvedValue(base);
  const save = vi.spyOn(api, 'saveSettings');
  let resolve!: (value: { registered: boolean; error: null }) => void;
  const change = vi.spyOn(api, 'setAutostart').mockImplementation(() => new Promise(r => { resolve = r; }));
  const { result } = renderHook(() => useMonitor());
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  let pending!: ReturnType<typeof result.current.changeAutostart>;
  act(() => { pending = result.current.changeAutostart(true); });
  await expect(result.current.saveSettings({ ...base.settings, samplingIntervalSecs: 5 })).rejects.toThrow('正在保存');
  await expect(result.current.changeAutostart(false)).rejects.toThrow('正在保存');
  expect(change).toHaveBeenCalledTimes(1);
  expect(save).not.toHaveBeenCalled();
  await act(async () => { resolve({ registered: true, error: null }); await pending; });
  expect(result.current.saving).toBe(false);
  expect(result.current.state?.settings).toEqual(base.settings);
});

it.each(['resolve', 'reject'])('resumes polling after a slow startup operation: %s', async outcome => {
  const base = monitorState();
  const read = vi.spyOn(api, 'getMonitorState').mockResolvedValue(base);
  let finish!: () => void;
  vi.spyOn(api, 'setAutostart').mockImplementation(() => new Promise((resolve, reject) => {
    finish = () => outcome === 'resolve' ? resolve({ registered: true, error: null }) : reject(new Error('denied'));
  }));
  const { result } = renderHook(() => useMonitor());
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  const before = read.mock.calls.length;
  let pending!: Promise<unknown>;
  act(() => { pending = result.current.changeAutostart(true).catch(() => {}); });
  await act(async () => { await vi.advanceTimersByTimeAsync(3000); });
  expect(read.mock.calls.length).toBe(before);
  await act(async () => { finish(); await pending; });
  expect(read.mock.calls.length).toBe(before + 1);
  await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
  expect(read.mock.calls.length).toBe(before + 3);
});
