import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import type { Settings } from './bindings';
import { monitorState } from './testFixtures';

const useMonitor = vi.hoisted(() => vi.fn());
vi.mock('./useMonitor', () => ({ useMonitor }));
vi.mock('./charts', () => ({ TrendChart: () => <div /> }));
vi.mock('./api', () => ({ getStartupStatus: vi.fn().mockResolvedValue({ registered: false, error: null }), listenVisibility: vi.fn().mockResolvedValue(() => {}), readGpuTemperatures: vi.fn().mockRejectedValue(new Error('无显卡接口')), readDiskHealth: vi.fn().mockRejectedValue(new Error('无磁盘状态')) }));
import App from './App';

beforeEach(() => vi.spyOn(window, 'scrollTo').mockImplementation(() => {}));
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

function setup(settings: Settings = monitorState().settings) {
  const state = { ...monitorState(), settings, latest: { ...monitorState().latest!, networks: [
    { name: 'Ethernet', receivedBytes: 100, transmittedBytes: 200, downloadBytesPerSec: 10, uploadBytesPerSec: 20 },
    { name: 'Wi-Fi', receivedBytes: 300, transmittedBytes: 400, downloadBytesPerSec: 30, uploadBytesPerSec: 40 },
  ] } };
  const saveSettings = vi.fn(async (next: Settings) => next);
  useMonitor.mockReturnValue({ state, snapshot: state.latest, history: [], error: null, now: Date.now(), saveSettings, changeAutostart: vi.fn(), saving: false });
  return { saveSettings };
}

it('returns to the top and focuses the destination heading when navigating', async () => {
  setup();
  const scrollTo = window.scrollTo;
  render(<App />);
  fireEvent.click(screen.getByRole('button', { name: 'Network' }));
  await waitFor(() => expect(document.activeElement).toBe(screen.getByRole('heading', { name: 'Network.' })));
  expect(scrollTo).toHaveBeenCalledWith({ top: 0, behavior: 'instant' });
});

it('shows only available disk and GPU counter readings', async () => {
  const state = monitorState();
  const latest = { ...state.latest!, performance: { diskStatus: 'available' as const, disks: [{ mountPoint: 'C:\\', readBytesPerSec: null, writeBytesPerSec: 1024 }], gpuStatus: 'available' as const, gpuEngines: [{ adapter: 'adapter-a', engine: '3D', engineIndex: '0', usagePercent: 12.5 }] } };
  useMonitor.mockReturnValue({ state: { ...state, latest }, snapshot: latest, history: [], error: null, now: Date.now(), saveSettings: vi.fn(), changeAutostart: vi.fn(), saving: false });
  render(<App />);
  fireEvent.click(screen.getByRole('button', { name: 'Storage' }));
  expect(screen.getByText(/读取 —\/s · 写入 1.0 KiB\/s/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: 'Hardware' }));
  expect(screen.getByText('12.5%')).toBeTruthy();
  expect(screen.getByText(/不等同于整张显卡/)).toBeTruthy();
});

it('keeps unsaved settings when leaving the page and can restore effective values', () => {
  setup();
  render(<App />);
  fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
  fireEvent.change(screen.getByLabelText('采样间隔'), { target: { value: '5' } });
  fireEvent.click(screen.getByLabelText('持续高负载提醒'));
  fireEvent.click(screen.getByRole('button', { name: 'Overview' }));
  fireEvent.click(screen.getByRole('button', { name: 'Settings' }));
  expect((screen.getByLabelText('采样间隔') as HTMLSelectElement).value).toBe('5');
  expect(screen.getByText('有未保存更改')).toBeTruthy();
  expect(screen.getByText(/退出应用后会丢弃/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '恢复当前生效设置' }));
  expect((screen.getByLabelText('采样间隔') as HTMLSelectElement).value).toBe('1');
  expect((screen.getByLabelText('持续高负载提醒') as HTMLInputElement).checked).toBe(false);
  expect(screen.queryByText('有未保存更改')).toBeNull();
});

it('uses the saved interface in Overview and marks an unsaved Network choice as a preview', () => {
  const { saveSettings } = setup({ ...monitorState().settings, selectedNetworkInterface: 'Ethernet' });
  render(<App />);
  expect(screen.getAllByText('Ethernet')).toHaveLength(2);
  expect(screen.getByText('10 B')).toBeTruthy();
  expect(screen.queryByText('40 B')).toBeNull();
  fireEvent.click(screen.getByRole('button', { name: 'Network' }));
  fireEvent.change(screen.getByLabelText('监控接口'), { target: { value: 'Wi-Fi' } });
  expect(screen.getByText('预览，尚未保存')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '保存接口选择' }));
  expect(saveSettings).toHaveBeenCalledWith({ ...monitorState().settings, selectedNetworkInterface: 'Wi-Fi' });
});

it('shows bounded current CPU and resident-memory process rankings without process controls', () => {
  const base = monitorState();
  const processes = Array.from({ length: 9 }, (_, i) => ({ pid: i + 1, name: `worker-${i + 1}`, cpuUsagePercent: 90 - i, residentMemoryBytes: (i + 1) * 1024 }));
  const state = { ...base, processSnapshot: { sampledAtMs: Date.now(), topCpu: processes.slice(0, 8), topMemory: [...processes].reverse().slice(0, 8) } };
  useMonitor.mockReturnValue({ state, snapshot: state.latest, history: [], error: null, now: Date.now(), saveSettings: vi.fn(), changeAutostart: vi.fn(), saving: false });
  render(<App />);
  fireEvent.click(screen.getByRole('button', { name: /查看负载来源/ }));
  expect(screen.getByText('CPU 占用较高')).toBeTruthy();
  expect(screen.getByText('驻留内存较高')).toBeTruthy();
  expect(screen.getAllByText(/worker-/).length).toBe(16);
  expect(screen.queryByRole('button', { name: /结束|启动进程/ })).toBeNull();
  expect(screen.getByText(/进程列表不会写入磁盘或上传/)).toBeTruthy();
});

it('shows a warming label and an explicit empty state for process readings', () => {
  const state = monitorState();
  state.processSnapshot = { sampledAtMs: Date.now(), topCpu: [{ pid: 42, name: 'new-worker', cpuUsagePercent: null, residentMemoryBytes: 2048 }], topMemory: [] };
  useMonitor.mockReturnValue({ state, snapshot: state.latest, history: [], error: null, now: Date.now(), saveSettings: vi.fn(), changeAutostart: vi.fn(), saving: false });
  render(<App />);
  fireEvent.click(screen.getByRole('button', { name: 'Activity' }));
  expect(screen.getByText('CPU 计算中')).toBeTruthy();
  expect(screen.getAllByText('等待进程采样')).toHaveLength(1);
});

it('shows local load events and clears them on request', async () => {
  const base = monitorState();
  const state = { ...base, loadEvents: [{ id: 1, metric: 'cpu', status: 'interrupted', startedAtMs: 1000, confirmedAtMs: 31000, lastAtMs: 41000, durationSecs: 40, peakPercent: 98, processes: [{ name: 'worker', cpuUsagePercent: 24, residentMemoryBytes: 4096 }] }] };
  const clearEvents = vi.fn().mockResolvedValue(undefined);
  useMonitor.mockReturnValue({ state, snapshot: state.latest, history: [], error: null, now: Date.now(), saveSettings: vi.fn(), changeAutostart: vi.fn(), saving: false, clearEvents });
  render(<App />);
  fireEvent.click(screen.getByRole('button', { name: 'Activity' }));
  expect(screen.getByText('采样中断')).toBeTruthy();
  fireEvent.click(screen.getByText('采样中断'));
  expect(screen.getByText(/worker/)).toBeTruthy();
  expect(screen.getByText(/退出后清空/)).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '清空事件' }));
  await waitFor(() => expect(clearEvents).toHaveBeenCalledOnce());
});

it('shows an empty event state and reports a clear failure', async () => {
  setup();
  const { unmount } = render(<App />);
  fireEvent.click(screen.getByRole('button', { name: 'Activity' }));
  expect(screen.getByText('本次运行暂无持续高负载事件')).toBeTruthy();
  expect((screen.getByRole('button', { name: '清空事件' }) as HTMLButtonElement).disabled).toBe(true);
  unmount();
  const base = monitorState();
  const state = { ...base, loadEvents: [{ id: 1, metric: 'memory', status: 'active', startedAtMs: 1000, confirmedAtMs: 31000, lastAtMs: 31000, durationSecs: 30, peakPercent: 91, processes: [] }] };
  useMonitor.mockReturnValue({ state, snapshot: state.latest, history: [], error: null, now: Date.now(), saveSettings: vi.fn(), changeAutostart: vi.fn(), saving: false, clearEvents: vi.fn().mockRejectedValue(new Error('读取失败')) });
  render(<App />);
  fireEvent.click(screen.getByRole('button', { name: 'Activity' }));
  fireEvent.click(screen.getByRole('button', { name: '清空事件' }));
  expect((await screen.findByRole('alert')).textContent).toContain('清空失败');
});
