import { useEffect, useRef, useState } from 'react';
import { readAtaDeviceStats, readDiskHealth, readGpuTemperatures } from './api';
import type { AtaDeviceStatsReport } from './bindings';
import { useDeviceReading } from './useDeviceReading';

const time = (value: number) => new Date(value).toLocaleTimeString('zh-CN', { hour12: false });
const commonSataAttributeNames: Record<number, string> = {
  5: '重映射扇区数',
  9: '通电时间',
  12: '通电次数',
};
const sataAttributeName = (id: number) => {
  const name = commonSataAttributeNames[id];
  return name ? `${name}（常见释义） · ID ${id}` : `厂商定义属性 · ID ${id}`;
};
const nvmeCriticalWarningBits = [
  [0x01, '可用备用空间低于阈值'],
  [0x02, '温度达到过高或过低阈值'],
  [0x04, '可靠性因显著的介质相关错误或影响可靠性的内部错误而下降'],
  [0x08, '存储介质已被设为只读'],
  [0x10, '易失性内存备用设备报告故障（仅适用于配备该设备的控制器）'],
] as const;
const nvmeCriticalWarning = (value: number) => {
  const warnings: string[] = nvmeCriticalWarningBits.filter(([bit]) => value & bit).map(([, text]) => text);
  const undecoded = value & ~0x1f;
  if (undecoded) warnings.push(`未解读标志 0x${undecoded.toString(16)}`);
  return `严重警告标志 0x${value.toString(16).padStart(2, '0')}：${warnings.length ? warnings.join('；') : '无'}`;
};
const nvmePercentageUsed = (value: number) => value === 255 ? '255% 或更高' : `${value}%`;

export function GpuTemperaturePanel() {
  const { data, error, busy, slow, autoPaused, refresh } = useDeviceReading(readGpuTemperatures, 10000);
  return <>
    <p className="settings-copy">读取 NVIDIA 驱动提供的 GPU 温度；AMD、Intel 显卡温度暂不可用。仅在此页面可见时自动更新。</p>
    {data && <div className="device-readings">{data.devices.map((device, index) => <div className="device-reading" key={`${device.name}-${index}`}><div><strong>{device.name}</strong><span>{device.error ?? 'GPU 核心温度'}</span></div><b>{device.celsius == null ? '—' : `${device.celsius} °C`}</b></div>)}</div>}
    {error && <p className="form-error" role="alert">{error}</p>}
    {slow && <p className="muted" role="status">读取或等待设备时响应较慢，正在等待结果；可继续使用其他页面。</p>}
    {autoPaused && <p className="form-error">自动刷新暂不可用，可手动读取。</p>}
    <div className="device-actions"><span>{data ? `读取时间：${time(data.sampledAtMs)}${busy ? ' · 正在更新' : ''}` : busy ? '正在读取…' : '等待有效数据'}</span><button className="text-button" type="button" disabled={busy} onClick={refresh}>刷新温度</button></div>
  </>;
}

export function DiskHealthPanel() {
  const { data, error, busy, slow, autoPaused, refresh } = useDeviceReading(readDiskHealth, 60000);
  const [ataStats, setAtaStats] = useState<AtaDeviceStatsReport | null>(null);
  const [ataStatsError, setAtaStatsError] = useState<string | null>(null);
  const [ataStatsBusy, setAtaStatsBusy] = useState(false);
  const ataStatsInFlight = useRef(false);
  const mounted = useRef(false);
  const label = { normal: 'Windows 报告正常', warning: 'Windows 提示警告', unhealthy: 'Windows 报告异常', unknown: '状态未知' } as const;
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  async function refreshAtaStats() {
    if (ataStatsInFlight.current) return;
    ataStatsInFlight.current = true;
    setAtaStatsBusy(true);
    setAtaStatsError(null);
    setAtaStats(null);
    try {
      const report = await readAtaDeviceStats();
      if (mounted.current) setAtaStats(report);
    } catch (reason) {
      if (mounted.current) setAtaStatsError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      ataStatsInFlight.current = false;
      if (mounted.current) setAtaStatsBusy(false);
    }
  }
  return <>
    <p className="settings-copy">Windows 报告的物理磁盘状态与设备故障预测并非完整的 S.M.A.R.T. 检查；状态正常或未预测故障也不能保证磁盘没有故障。</p>
    {data && <div className="device-readings">{data.disks.map((disk, index) => {
      const deviceRisk = disk.sataPredictFailure === true || (disk.nvmeHealth != null && disk.nvmeHealth.criticalWarning !== 0);
      return <div className="device-reading disk-health-reading" key={`${disk.name}-${index}`}><div><strong>{disk.name}</strong><span>物理磁盘</span>{deviceRisk && <span>{label[disk.status]}</span>}{disk.sataTemperatureCelsius != null && <span className="disk-temperature">设备温度（首个传感器）{disk.sataTemperatureCelsius} °C</span>}{disk.sataTemperatureError && <span>磁盘温度不可用：{disk.sataTemperatureError}</span>}{disk.sataPredictFailure != null && <span>设备故障预测：{disk.sataPredictFailure ? '预测发生故障' : '当前未预测故障'}</span>}{disk.sataPredictionError && <span>{!disk.sataSmartAttributes?.length ? `故障预测与 SATA 属性不可用：${disk.sataPredictionError}` : `设备故障预测不可用：${disk.sataPredictionError}`}</span>}{disk.nvmeHealth && <span>NVMe 健康日志 · 温度 {disk.nvmeHealth.temperatureCelsius == null ? '不可用' : `${disk.nvmeHealth.temperatureCelsius} °C`} · 可用备用空间 {disk.nvmeHealth.availableSpare}%（阈值 {disk.nvmeHealth.spareThreshold}%）· 已用寿命估计 {nvmePercentageUsed(disk.nvmeHealth.percentageUsed)} · {nvmeCriticalWarning(disk.nvmeHealth.criticalWarning)}</span>}{disk.nvmeError && <span>NVMe 健康日志不可用：{disk.nvmeError}</span>}</div><b className={deviceRisk ? 'health-warning' : `health-${disk.status}`}>{deviceRisk ? '磁盘提示风险' : label[disk.status]}</b></div>;
    })}</div>}
    {data?.disks.filter(disk => disk.nvmeHealth).map((disk, index) => <details className="thermal-details" key={`${disk.name}-nvme-${index}`}>
      <summary>{disk.name} · NVMe 累计统计</summary>
      <p className="muted detail-note">以下为设备健康日志报告的历史累计值，低功耗状态可能不计入通电时间。异常断电表示设备未收到关机通知即失去电源；介质与数据完整性错误是设备报告的未恢复错误。历史累计值不能单独判断当前磁盘健康。</p>
      <div className="device-readings"><div className="device-reading"><div>
        <span>通电时间：{disk.nvmeHealth!.powerOnHours} 小时</span>
        <span>通电次数：{disk.nvmeHealth!.powerCycles}</span>
        <span>异常断电次数：{disk.nvmeHealth!.unsafeShutdowns}</span>
        <span>介质与数据完整性错误：{disk.nvmeHealth!.mediaErrors}</span>
      </div></div></div>
    </details>)}
    {data?.disks.filter(disk => disk.sataSmartAttributes || disk.sataSmartError).map((disk, index) => <details className="thermal-details" key={`${disk.name}-smart-${index}`}>
      <summary>{disk.name} · SATA 属性{disk.sataSmartAttributes ? ` · ${disk.sataSmartAttributes.length} 项` : ''}</summary>
      {disk.sataSmartError ? <p className="muted">{disk.sataSmartError}</p> : <>
        <p className="muted detail-note">数据来自 Windows 故障预测接口附带的厂商专有区，仅在 ATA 属性结构和校验和通过时展示。少数名称是常见释义，厂商可能采用不同定义；当前值与历史最差值不是百分比，原始字段不推断单位。未读取阈值、错误日志或自检日志，因此不代表完整 S.M.A.R.T. 诊断。</p>
        <div className="device-readings">{disk.sataSmartAttributes?.map(attribute => <div className="device-reading" key={attribute.id}>
          <div><strong>{sataAttributeName(attribute.id)}</strong><span>厂商当前值 {attribute.current} · 历史最差 {attribute.worst}</span></div>
          <b>原始值 0x{attribute.rawHex}</b>
        </div>)}</div>
      </>}
    </details>)}
    {data?.disks.some(disk => disk.status === 'warning' || disk.status === 'unhealthy' || disk.sataPredictFailure === true || (disk.nvmeHealth != null && disk.nvmeHealth.criticalWarning !== 0)) && <p className="form-error" role="status">Windows 或磁盘报告提示可能存在问题。请尽快备份重要文件，并使用磁盘厂商提供的工具进一步检查。</p>}
    {data?.disks.some(disk => disk.status === 'unknown') && <p className="muted detail-note">状态未知不代表磁盘正常；Windows 没有提供足够的信息。</p>}
    {error && <p className="form-error" role="alert">{error}</p>}
    {slow && <p className="muted" role="status">读取或等待设备时响应较慢，正在等待结果；可继续使用其他页面。</p>}
    {autoPaused && <p className="form-error">自动刷新暂不可用，可手动读取。</p>}
    <div className="device-actions"><span>{data ? `读取时间：${time(data.sampledAtMs)}${busy ? ' · 正在更新' : ''}` : busy ? '正在读取…' : '等待有效数据'}</span><button className="text-button" type="button" disabled={busy || ataStatsBusy} onClick={refresh}>刷新状态</button></div>
    <p className="settings-copy">SATA 设备统计由设备统计日志提供，包含通电重置次数、通电时间和向系统报告的不可纠正错误。数据仅反映设备已报告的内容，不含设备后台未报告的错误；这不是完整 S.M.A.R.T. 诊断，也不用于推断磁盘状态。</p>
    <div className="device-actions"><span>{ataStats ? `统计读取时间：${time(ataStats.sampledAtMs)}` : ataStatsBusy ? '正在读取 SATA 设备统计…' : '按需读取；不会自动刷新'}</span><button className="text-button" type="button" disabled={ataStatsBusy} onClick={() => void refreshAtaStats()}>{ataStatsBusy ? '正在读取…' : '读取 SATA 设备统计'}</button></div>
    {ataStatsError && <p className="form-error" role="alert">读取 SATA 设备统计失败：{ataStatsError}</p>}
    {ataStats && <div className="device-readings">
      {ataStats.disks.length === 0 ? <p className="muted">没有可显示的 SATA 设备统计。</p> : ataStats.disks.map((disk, index) => <div className="device-reading" key={`${disk.name}-ata-stats-${index}`}>
        <div><strong>{disk.name}</strong>
          {disk.stats ? <>
            <span>通电重置次数：{disk.stats.powerCycles ?? '不可用'}</span>
            <span>通电时间：{disk.stats.powerOnHours == null ? '不可用' : `${disk.stats.powerOnHours} 小时`}</span>
            <span>向系统报告的不可纠正错误：{disk.stats.reportedUncorrectableErrors ?? '不可用'}</span>
          </> : <span>{disk.error ? '没有返回可用统计值。' : '设备不支持或数据无效，未提供可用统计值。'}</span>}
          {disk.error && <span>{disk.stats ? '部分统计读取失败' : '统计读取失败'}：{disk.error}</span>}
        </div>
      </div>)}
    </div>}
    <p className="muted detail-note">NVMe 数据来自设备健康日志，已用寿命为设备估计值；未读取 ATA 阈值、错误日志或自检日志，完整 S.M.A.R.T. 诊断尚未支持。物理磁盘与盘符不一定一一对应；仅在存储页面可见时每分钟更新。</p>
  </>;
}
