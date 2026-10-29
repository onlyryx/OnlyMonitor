import { act, renderHook } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
import { listenVisibility } from './api';
import { useDeviceReading } from './useDeviceReading';

vi.mock('./api', () => ({ listenVisibility: vi.fn() }));

it('pauses polling while hidden and stops it after unmount', async () => {
  vi.useFakeTimers();
  let onVisibility: (visible: boolean) => void = () => {};
  const unlisten = vi.fn();
  vi.mocked(listenVisibility).mockImplementation(async callback => { onVisibility = callback; return unlisten; });
  const read = vi.fn().mockResolvedValue('ok');
  const { unmount } = renderHook(() => useDeviceReading(read, 10_000));
  await act(async () => {});
  expect(read).toHaveBeenCalledTimes(1);
  act(() => onVisibility(false));
  await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
  expect(read).toHaveBeenCalledTimes(1);
  await act(async () => { onVisibility(true); });
  expect(read).toHaveBeenCalledTimes(2);
  unmount();
  await act(async () => { await vi.advanceTimersByTimeAsync(30_000); });
  expect(read).toHaveBeenCalledTimes(2);
  expect(unlisten).toHaveBeenCalledOnce();
  vi.useRealTimers();
});

it('aborts a queued read when hidden and unmounted', async () => {
  vi.useFakeTimers();
  let onVisibility: (visible: boolean) => void = () => {};
  vi.mocked(listenVisibility).mockImplementation(async callback => { onVisibility = callback; return () => {}; });
  const signals: AbortSignal[] = [];
  const read = vi.fn((signal?: AbortSignal) => new Promise<string>((_, reject) => {
    if (!signal) return;
    signals.push(signal);
    signal.addEventListener('abort', () => reject(new Error('cancelled')), { once: true });
  }));
  const { unmount } = renderHook(() => useDeviceReading(read, 10_000));
  await act(async () => {});
  expect(signals).toHaveLength(1);

  act(() => onVisibility(false));
  expect(signals[0].aborted).toBe(true);
  await act(async () => {});
  act(() => onVisibility(true));
  await act(async () => {});
  expect(signals).toHaveLength(2);

  unmount();
  expect(signals[1].aborted).toBe(true);
  await act(async () => {});
  vi.useRealTimers();
});

it('shows the slow-reading notice after visibility resumes with a read still pending', async () => {
  vi.useFakeTimers();
  let onVisibility: (visible: boolean) => void = () => {};
  vi.mocked(listenVisibility).mockImplementation(async callback => { onVisibility = callback; return () => {}; });
  let finish!: (value: string) => void;
  const pending = new Promise<string>(resolve => { finish = resolve; });
  const read = vi.fn(() => pending);
  const { result, unmount } = renderHook(() => useDeviceReading(read, 10_000));
  await act(async () => {});
  expect(result.current.busy).toBe(true);

  act(() => onVisibility(false));
  act(() => onVisibility(true));
  await act(async () => { await vi.advanceTimersByTimeAsync(8000); });
  const resumedSlow = result.current.slow;

  await act(async () => { finish('ok'); });
  unmount();
  expect(resumedSlow).toBe(true);
  vi.useRealTimers();
});
