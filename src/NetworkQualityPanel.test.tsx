import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { listenVisibility, pingOnce, resolvePingTarget } from './api';
import { NetworkQualityPanel } from './NetworkQualityPanel';
import type { PingSample } from './networkQuality';

vi.mock('./api', () => ({ listenVisibility: vi.fn(), pingOnce: vi.fn(), resolvePingTarget: vi.fn() }));

const response = (rttMs: number): PingSample => ({ sampledAtMs: Date.now(), outcome: 'reply', rttMs, detail: null });
let setWindowVisible: (visible: boolean) => void;
const documentVisibility = Object.getOwnPropertyDescriptor(document, 'visibilityState');

beforeEach(() => {
  vi.useFakeTimers();
  vi.mocked(listenVisibility).mockImplementation(async handler => { setWindowVisible = handler; return () => {}; });
  vi.mocked(resolvePingTarget).mockResolvedValue('192.0.2.7');
  vi.mocked(pingOnce).mockResolvedValue(response(12));
});
afterEach(() => {
  cleanup(); vi.clearAllMocks(); vi.useRealTimers();
  if (documentVisibility) Object.defineProperty(document, 'visibilityState', documentVisibility);
  else Reflect.deleteProperty(document, 'visibilityState');
});

async function ready() {
  await act(async () => { await Promise.resolve(); });
}

function enterTarget() {
  fireEvent.change(screen.getByLabelText('IPv4 地址或域名'), { target: { value: 'example.test' } });
}

it('does not resolve or probe until the user enters a target and starts', async () => {
  render(<NetworkQualityPanel />);
  await ready();

  const targetInput = screen.getByLabelText('IPv4 地址或域名');
  expect(targetInput.getAttribute('aria-describedby')).toBe('network-quality-disclosure');
  expect(screen.getByText(/不会自动探测。点击“开始探测”后才会使用系统 DNS 解析域名并向目标发送 ICMP 请求；目标可以是外部主机/)).toBeTruthy();
  expect(resolvePingTarget).not.toHaveBeenCalled();
  expect(pingOnce).not.toHaveBeenCalled();
  expect((screen.getByRole('button', { name: '开始探测' }) as HTMLButtonElement).disabled).toBe(true);

  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();

  expect(resolvePingTarget).toHaveBeenCalledWith('example.test', expect.any(Object));
  expect(pingOnce).toHaveBeenCalledWith('192.0.2.7', expect.any(Object));
  expect(screen.getByRole('status').textContent).toContain('192.0.2.7');
  expect(screen.getByText('12 ms', { selector: '.quality-latest' })).toBeTruthy();
  expect(screen.getByText(new RegExp(`采样时间 ${new Date(Date.now()).toLocaleTimeString('zh-CN', { hour12: false })}`))).toBeTruthy();
});

it('ignores a late DNS result after Stop and never starts the probe', async () => {
  let finishResolution!: (ip: string) => void;
  vi.mocked(resolvePingTarget).mockImplementationOnce(() => new Promise(resolve => { finishResolution = resolve; }));
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  expect(resolvePingTarget).toHaveBeenCalledOnce();

  fireEvent.click(screen.getByRole('button', { name: '停止探测' }));
  await act(async () => { finishResolution('192.0.2.7'); });

  expect(pingOnce).not.toHaveBeenCalled();
  expect(screen.getByRole('status').textContent).toContain('探测已停止');
});

it('discards an in-flight reply after Stop and sends no later probe', async () => {
  let finishPing!: (sample: PingSample) => void;
  vi.mocked(pingOnce).mockImplementationOnce(() => new Promise(resolve => { finishPing = resolve; }));
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();
  expect(pingOnce).toHaveBeenCalledOnce();

  fireEvent.click(screen.getByRole('button', { name: '停止探测' }));
  await act(async () => { finishPing(response(99)); });
  await act(async () => { await vi.advanceTimersByTimeAsync(5000); });

  expect(pingOnce).toHaveBeenCalledOnce();
  expect(screen.getByText('—', { selector: '.quality-latest' })).toBeTruthy();
  expect(screen.getByRole('status').textContent).toContain('探测已停止');
});

it('shows when a restarted Ping is waiting for the previous native request', async () => {
  let finishPing!: (sample: PingSample) => void;
  vi.mocked(pingOnce).mockImplementationOnce((_ip, options) => {
    options?.onWaiting?.();
    return new Promise(resolve => { finishPing = resolve; });
  });
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();

  expect(screen.getByRole('status').textContent).toContain('正在等待上一次 Ping 请求结束');
  fireEvent.click(screen.getByRole('button', { name: '停止探测' }));
  await act(async () => { finishPing(response(12)); });
  expect(screen.getByRole('status').textContent).toContain('探测已停止');
});

it('pauses on native window hide and does not count a late result', async () => {
  let finishPing!: (sample: PingSample) => void;
  vi.mocked(pingOnce).mockImplementationOnce(() => new Promise(resolve => { finishPing = resolve; }));
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();
  act(() => setWindowVisible(false));
  await act(async () => { finishPing(response(99)); });

  expect(screen.getByRole('status').textContent).toContain('窗口不可见，探测已暂停');
  expect(screen.getByText('0/0', { selector: '.quality-no-reply-count' })).toBeTruthy();
});

it('waits one second after each completed request and keeps only a rolling 60 samples', async () => {
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();
  expect(pingOnce).toHaveBeenCalledTimes(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(999); });
  expect(pingOnce).toHaveBeenCalledTimes(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(pingOnce).toHaveBeenCalledTimes(2);

  for (let i = 0; i < 59; i++) await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(pingOnce).toHaveBeenCalledTimes(61);
  expect(screen.getByRole('status').textContent).toContain('正在探测');
  expect(screen.getByRole('list', { name: '最近 60 次探测结果' }).children).toHaveLength(60);
});

it('does not add local invoke errors to the ICMP no-reply denominator', async () => {
  vi.mocked(pingOnce).mockRejectedValueOnce(new Error('系统 Ping 调用失败'));
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();

  expect(screen.getByRole('alert').textContent).toContain('系统 Ping 调用失败');
  expect(screen.getByText('0/0', { selector: '.quality-no-reply-count' })).toBeTruthy();
});

it('clears prior results when the target changes after a stop', async () => {
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();
  fireEvent.click(screen.getByRole('button', { name: '停止探测' }));
  fireEvent.change(screen.getByLabelText('IPv4 地址或域名'), { target: { value: 'other.example' } });

  expect(screen.getByText('—', { selector: '.quality-latest' })).toBeTruthy();
  expect(screen.getByText('0/0', { selector: '.quality-no-reply-count' })).toBeTruthy();
  expect(screen.getByRole('status').textContent).not.toContain('192.0.2.7');
});

it('stops with an interruption message when a Ping call takes over ten seconds', async () => {
  let finishPing!: (sample: PingSample) => void;
  vi.mocked(pingOnce).mockImplementationOnce(() => new Promise(resolve => { finishPing = resolve; }));
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();
  await act(async () => {
    vi.setSystemTime(Date.now() + 10_001);
    finishPing(response(99));
  });

  expect(screen.getByRole('status').textContent).toContain('采样中断，请重新开始');
  expect(screen.getByText('0/0', { selector: '.quality-no-reply-count' })).toBeTruthy();
  expect(pingOnce).toHaveBeenCalledOnce();
});

it('does not immediately probe again when the one-second delay wakes late', async () => {
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();

  await act(async () => {
    vi.setSystemTime(Date.now() + 10_001);
    await vi.advanceTimersByTimeAsync(1000);
  });

  expect(screen.getByRole('status').textContent).toContain('采样中断，请重新开始');
  expect(pingOnce).toHaveBeenCalledOnce();
});

it('stops without counting a reply when the system clock moves backwards', async () => {
  let finishPing!: (sample: PingSample) => void;
  vi.mocked(pingOnce).mockImplementationOnce(() => new Promise(resolve => { finishPing = resolve; }));
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();
  await act(async () => { vi.setSystemTime(Date.now() - 1); finishPing(response(99)); });

  expect(screen.getByRole('alert').textContent).toContain('系统时钟变化，采样已停止');
  expect(screen.getByText('0/0', { selector: '.quality-no-reply-count' })).toBeTruthy();
});

it('does not start another Ping after the wall clock moves backwards during the delay', async () => {
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();
  await act(async () => {
    vi.setSystemTime(Date.now() - 10_001);
    await vi.advanceTimersByTimeAsync(1000);
  });

  expect(screen.getByRole('alert').textContent).toContain('系统时钟变化，采样已停止');
  expect(pingOnce).toHaveBeenCalledOnce();
});

it('does not probe again after the document becomes hidden', async () => {
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();

  Object.defineProperty(document, 'visibilityState', { configurable: true, value: 'hidden' });
  fireEvent(document, new Event('visibilitychange'));
  await act(async () => { await vi.advanceTimersByTimeAsync(5000); });

  expect(pingOnce).toHaveBeenCalledOnce();
  expect(screen.getByRole('status').textContent).toContain('窗口不可见，探测已暂停');
});

it('does not count a DNS failure as an ICMP request', async () => {
  vi.mocked(resolvePingTarget).mockRejectedValueOnce(new Error('域名解析失败'));
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();

  expect(screen.getByRole('alert').textContent).toContain('域名解析失败');
  expect(pingOnce).not.toHaveBeenCalled();
  expect(screen.getByText('0/0', { selector: '.quality-no-reply-count' })).toBeTruthy();
});

it('reports a slow DNS resolution without starting or counting a Ping', async () => {
  let finishResolution!: (ip: string) => void;
  vi.mocked(resolvePingTarget).mockImplementationOnce(() => new Promise(resolve => { finishResolution = resolve; }));
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await act(async () => {
    vi.setSystemTime(Date.now() + 10_001);
    finishResolution('192.0.2.7');
  });

  expect(screen.getByRole('alert').textContent).toContain('目标解析耗时过长，请重新开始');
  expect(pingOnce).not.toHaveBeenCalled();
  expect(screen.getByText('0/0', { selector: '.quality-no-reply-count' })).toBeTruthy();
});

it('disables probing when native window visibility cannot be observed', async () => {
  vi.mocked(listenVisibility).mockRejectedValueOnce(new Error('visibility listener failed'));
  render(<NetworkQualityPanel />);
  await ready();
  enterTarget();

  expect(screen.getByRole('alert').textContent).toContain('visibility listener failed');
  expect((screen.getByRole('button', { name: '重新开始' }) as HTMLButtonElement).disabled).toBe(true);
  expect(resolvePingTarget).not.toHaveBeenCalled();
});

it('stops after the page unmounts while a Ping call is in flight', async () => {
  let finishPing!: (sample: PingSample) => void;
  vi.mocked(pingOnce).mockImplementationOnce(() => new Promise(resolve => { finishPing = resolve; }));
  const view = render(<NetworkQualityPanel />);
  await ready();
  enterTarget();
  fireEvent.click(screen.getByRole('button', { name: '开始探测' }));
  await ready();
  expect(pingOnce).toHaveBeenCalledOnce();

  view.unmount();
  await act(async () => { finishPing(response(99)); await vi.advanceTimersByTimeAsync(5000); });

  expect(pingOnce).toHaveBeenCalledOnce();
});
