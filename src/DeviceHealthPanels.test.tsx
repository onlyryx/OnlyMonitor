import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { DiskHealthPanel, GpuTemperaturePanel } from './DeviceHealthPanels';
import { readAtaDeviceStats, readDiskHealth, readGpuTemperatures } from './api';

vi.mock('./api', () => ({
  readAtaDeviceStats: vi.fn(), readDiskHealth: vi.fn(), readGpuTemperatures: vi.fn(),
  listenVisibility: vi.fn().mockResolvedValue(() => {}),
}));
afterEach(() => { cleanup(); vi.clearAllMocks(); vi.useRealTimers(); });

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>(done => { resolve = done; });
  return { promise, resolve };
}

it('shows a GPU temperature and removes the old reading after a failed refresh', async () => {
  vi.mocked(readGpuTemperatures).mockResolvedValueOnce({ sampledAtMs: 1000, devices: [{ name: 'NVIDIA GPU', celsius: 51, error: null }] })
    .mockRejectedValueOnce(new Error('驱动不可用'));
  render(<GpuTemperaturePanel />);
  expect(await screen.findByText('51 °C')).toBeTruthy();
  fireEvent.click(screen.getByRole('button', { name: '刷新温度' }));
  await waitFor(() => expect(screen.getByRole('alert').textContent).toContain('驱动不可用'));
  expect(screen.queryByText('51 °C')).toBeNull();
});

it('warns when the SATA device predicts failure even if Windows reports normal', async () => {
  vi.mocked(readDiskHealth).mockResolvedValueOnce({ sampledAtMs: 1000, disks: [
    { name: 'Drive', status: 'normal', nvmeHealth: null, nvmeError: null, sataTemperatureCelsius: null, sataTemperatureError: null, sataPredictFailure: true, sataPredictionError: null, sataSmartAttributes: null, sataSmartError: null },
  ] });
  render(<DiskHealthPanel />);
  expect(await screen.findByText(/尽快备份重要文件/)).toBeTruthy();
  const row = screen.getByText('Drive').closest('.device-reading') as HTMLElement;
  expect(within(row).getByText('磁盘提示风险')).toBeTruthy();
  expect(within(row).getByText('Windows 报告正常')).toBeTruthy();
});

it('explains NVMe warning bits while preserving disk report values and limits', async () => {
  vi.mocked(readDiskHealth).mockResolvedValueOnce({ sampledAtMs: 1000, disks: [
    { name: 'Drive A', status: 'normal', nvmeHealth: { criticalWarning: 255, temperatureCelsius: 56, availableSpare: 100, spareThreshold: 10, percentageUsed: 26, powerCycles: '4', powerOnHours: '6831', unsafeShutdowns: '322', mediaErrors: '340282366920938463463374607431768211455' }, nvmeError: null, sataTemperatureCelsius: null, sataTemperatureError: null, sataPredictFailure: null, sataPredictionError: null, sataSmartAttributes: null, sataSmartError: null },
    { name: 'Drive B', status: 'unknown', nvmeHealth: { criticalWarning: 0, temperatureCelsius: null, availableSpare: 100, spareThreshold: 10, percentageUsed: 0, powerCycles: '0', powerOnHours: '0', unsafeShutdowns: '0', mediaErrors: '0' }, nvmeError: null, sataTemperatureCelsius: 36, sataTemperatureError: null, sataPredictFailure: true, sataPredictionError: null, sataSmartAttributes: [{ id: 5, current: 100, worst: 99, rawHex: '060504030201' }, { id: 196, current: 98, worst: 90, rawHex: '000000000011' }], sataSmartError: null }, { name: 'Drive C', status: 'warning', nvmeHealth: null, nvmeError: null, sataTemperatureCelsius: null, sataTemperatureError: '设备不支持', sataPredictFailure: null, sataPredictionError: '设备不支持', sataSmartAttributes: null, sataSmartError: null }, { name: 'Drive D', status: 'normal', nvmeHealth: null, nvmeError: null, sataTemperatureCelsius: null, sataTemperatureError: null, sataPredictFailure: null, sataPredictionError: '无法可靠匹配 Windows 磁盘编号', sataSmartAttributes: null, sataSmartError: null },
  ] });
  render(<DiskHealthPanel />);
  expect((await screen.findAllByText('Windows 报告正常')).length).toBeGreaterThan(0);
  const nvmeRow = screen.getByText('Drive A').closest('.device-reading') as HTMLElement;
  expect(within(nvmeRow).getByText('磁盘提示风险')).toBeTruthy();
  expect(screen.getByText('状态未知')).toBeTruthy();
  expect(screen.getByText('Windows 提示警告')).toBeTruthy();
  expect(screen.getByText(/不能保证磁盘没有故障/)).toBeTruthy();
  expect(screen.getByText(/尽快备份重要文件/)).toBeTruthy();
  expect(screen.getByText(/状态未知不代表磁盘正常/)).toBeTruthy();
  expect(screen.getByText(/温度 56 °C/)).toBeTruthy();
  expect(screen.getByText(/严重警告标志 0xff/)).toBeTruthy();
  expect(screen.getByText(/可用备用空间低于阈值/)).toBeTruthy();
  expect(screen.getByText(/温度达到过高或过低阈值/)).toBeTruthy();
  expect(screen.getByText(/可靠性因显著的介质相关错误或影响可靠性的内部错误而下降/)).toBeTruthy();
  expect(screen.getByText(/存储介质已被设为只读/)).toBeTruthy();
  expect(screen.getByText(/易失性内存备用设备报告故障/)).toBeTruthy();
  expect(screen.getByText(/未解读标志 0xe0/)).toBeTruthy();
  expect(screen.getByText(/严重警告标志 0x00：无/)).toBeTruthy();
  expect(screen.getByText(/已用寿命估计 26%/)).toBeTruthy();
  fireEvent.click(screen.getByText('Drive A · NVMe 累计统计'));
  expect(screen.getByText('通电时间：6831 小时')).toBeTruthy();
  expect(screen.getByText('通电次数：4')).toBeTruthy();
  expect(screen.getByText('异常断电次数：322')).toBeTruthy();
  expect(screen.getByText('介质与数据完整性错误：340282366920938463463374607431768211455')).toBeTruthy();
  expect(within(screen.getByText('Drive A · NVMe 累计统计').closest('details')!).getByText(/历史累计值不能单独判断当前磁盘健康/)).toBeTruthy();
  expect(screen.getByText('设备温度（首个传感器）36 °C')).toBeTruthy();
  expect(screen.getByText(/故障预测与 SATA 属性不可用：无法可靠匹配 Windows 磁盘编号/)).toBeTruthy();
  expect(screen.getByText('磁盘温度不可用：设备不支持')).toBeTruthy();
  expect(screen.getByText('设备故障预测：预测发生故障')).toBeTruthy();
  expect(screen.getByText('故障预测与 SATA 属性不可用：设备不支持')).toBeTruthy();
  fireEvent.click(screen.getByText('Drive B · SATA 属性 · 2 项'));
  expect(screen.getByText('重映射扇区数（常见释义） · ID 5')).toBeTruthy();
  expect(screen.getByText('厂商定义属性 · ID 196')).toBeTruthy();
  expect(screen.getByText('厂商当前值 100 · 历史最差 99')).toBeTruthy();
  expect(screen.getByText('原始值 0x060504030201')).toBeTruthy();
  expect(screen.getByText(/原始字段不推断单位/)).toBeTruthy();
  expect(screen.getByText(/不代表完整 S\.M\.A\.R\.T\. 诊断/)).toBeTruthy();
});

it('shows 255 percent NVMe wear as 255 percent or higher', async () => {
  vi.mocked(readDiskHealth).mockResolvedValueOnce({ sampledAtMs: 1000, disks: [
    { name: 'Drive', status: 'normal', nvmeHealth: { criticalWarning: 0, temperatureCelsius: 36, availableSpare: 100, spareThreshold: 10, percentageUsed: 255, powerCycles: '1', powerOnHours: '1', unsafeShutdowns: '0', mediaErrors: '0' }, nvmeError: null, sataTemperatureCelsius: null, sataTemperatureError: null, sataPredictFailure: null, sataPredictionError: null, sataSmartAttributes: null, sataSmartError: null },
  ] });
  render(<DiskHealthPanel />);

  expect(await screen.findByText(/已用寿命估计 255% 或更高/)).toBeTruthy();
});

it('keeps an on-demand SATA statistics read visibly pending beside a health refresh', async () => {
  const health = deferred<Awaited<ReturnType<typeof readDiskHealth>>>();
  vi.mocked(readDiskHealth).mockReturnValueOnce(health.promise);
  const stats = deferred<Awaited<ReturnType<typeof readAtaDeviceStats>>>();
  vi.mocked(readAtaDeviceStats).mockReturnValueOnce(stats.promise);
  render(<DiskHealthPanel />);

  await waitFor(() => expect(readDiskHealth).toHaveBeenCalledOnce());
  fireEvent.click(screen.getByRole('button', { name: '读取 SATA 设备统计' }));
  expect(readAtaDeviceStats).toHaveBeenCalledOnce();
  expect(screen.getByText('正在读取 SATA 设备统计…')).toBeTruthy();
  expect((screen.getByRole('button', { name: '正在读取…' }) as HTMLButtonElement).disabled).toBe(true);

  await act(async () => { health.resolve({ sampledAtMs: 1000, disks: [] }); });
  expect(screen.queryByRole('alert')).toBeNull();
  await act(async () => { stats.resolve({ sampledAtMs: 2000, disks: [
    { name: 'ATA SSD', stats: { powerOnHours: '12', powerCycles: '3', reportedUncorrectableErrors: '0' }, error: null },
  ] }); });

  expect(await screen.findByText('通电时间：12 小时')).toBeTruthy();
});

it('reads SATA device statistics only after an explicit request', async () => {
  vi.mocked(readDiskHealth).mockResolvedValue({ sampledAtMs: 1000, disks: [] });
  vi.mocked(readAtaDeviceStats).mockResolvedValueOnce({ sampledAtMs: 2000, disks: [
    { name: 'ATA SSD', stats: { powerOnHours: '4294967295', powerCycles: '9007199254740993', reportedUncorrectableErrors: '18446744073709551615' }, error: null },
  ] });
  render(<DiskHealthPanel />);

  await waitFor(() => expect(readDiskHealth).toHaveBeenCalledOnce());
  expect(readAtaDeviceStats).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole('button', { name: '刷新状态' }));
  await waitFor(() => expect(readDiskHealth).toHaveBeenCalledTimes(2));
  expect(readAtaDeviceStats).not.toHaveBeenCalled();

  fireEvent.click(screen.getByRole('button', { name: '读取 SATA 设备统计' }));
  expect(await screen.findByText('通电时间：4294967295 小时')).toBeTruthy();
  expect(screen.getByText('通电重置次数：9007199254740993')).toBeTruthy();
  expect(screen.getByText('向系统报告的不可纠正错误：18446744073709551615')).toBeTruthy();
  expect(readAtaDeviceStats).toHaveBeenCalledOnce();
});

it('does not request SATA statistics during automatic disk health polling', async () => {
  vi.useFakeTimers();
  vi.mocked(readDiskHealth).mockResolvedValue({ sampledAtMs: 1000, disks: [] });
  render(<DiskHealthPanel />);

  await act(async () => {});
  expect(readDiskHealth).toHaveBeenCalledOnce();
  await act(async () => { await vi.advanceTimersByTimeAsync(60000); });

  expect(readDiskHealth).toHaveBeenCalledTimes(2);
  expect(readAtaDeviceStats).not.toHaveBeenCalled();
});

it('shows unavailable fields and per-device errors without treating them as health status', async () => {
  vi.mocked(readDiskHealth).mockResolvedValueOnce({ sampledAtMs: 1000, disks: [
    { name: 'Partial Drive', status: 'normal', nvmeHealth: null, nvmeError: null, sataTemperatureCelsius: null, sataTemperatureError: null, sataPredictFailure: false, sataPredictionError: null, sataSmartAttributes: null, sataSmartError: null },
  ] });
  vi.mocked(readAtaDeviceStats).mockResolvedValueOnce({ sampledAtMs: 2000, disks: [
    { name: 'Unsupported Drive', stats: null, error: null },
    { name: 'Unreadable Drive', stats: null, error: 'ATA 统计不可用' },
    { name: 'Partial Drive', stats: { powerOnHours: null, powerCycles: '12', reportedUncorrectableErrors: '900' }, error: '部分日志不可用' },
  ] });
  render(<DiskHealthPanel />);

  fireEvent.click(screen.getByRole('button', { name: '读取 SATA 设备统计' }));

  expect(await screen.findByText('设备不支持或数据无效，未提供可用统计值。')).toBeTruthy();
  expect(screen.getByText('没有返回可用统计值。')).toBeTruthy();
  expect(screen.getByText('统计读取失败：ATA 统计不可用')).toBeTruthy();
  expect(screen.getByText('通电时间：不可用')).toBeTruthy();
  expect(screen.getByText('通电重置次数：12')).toBeTruthy();
  expect(screen.getByText('向系统报告的不可纠正错误：900')).toBeTruthy();
  expect(screen.getByText('部分统计读取失败：部分日志不可用')).toBeTruthy();
  expect(screen.getByText('Windows 报告正常')).toBeTruthy();
  expect(screen.queryByText('磁盘提示风险')).toBeNull();
});

it('shows a command error when SATA device statistics cannot be read', async () => {
  vi.mocked(readDiskHealth).mockResolvedValueOnce({ sampledAtMs: 1000, disks: [] });
  vi.mocked(readAtaDeviceStats).mockRejectedValueOnce(new Error('读取失败'));
  render(<DiskHealthPanel />);

  fireEvent.click(screen.getByRole('button', { name: '读取 SATA 设备统计' }));

  expect((await screen.findByRole('alert')).textContent).toContain('读取失败');
});

it('prevents overlapping manual SATA statistics requests', async () => {
  vi.mocked(readDiskHealth).mockResolvedValueOnce({ sampledAtMs: 1000, disks: [] });
  vi.mocked(readAtaDeviceStats).mockReturnValueOnce(new Promise(() => {}));
  render(<DiskHealthPanel />);

  const button = screen.getByRole('button', { name: '读取 SATA 设备统计' });
  fireEvent.click(button);
  fireEvent.click(button);

  expect((button as HTMLButtonElement).disabled).toBe(true);
  expect(readAtaDeviceStats).toHaveBeenCalledOnce();
});
