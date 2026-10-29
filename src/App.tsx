import { useEffect, useRef, useState, type ReactNode } from 'react';
import { motion, MotionConfig } from 'framer-motion';
import { Activity, ArrowDown, ArrowUp, ChevronRight, Cpu, Database, HardDrive, LayoutDashboard, LockKeyhole, MemoryStick, Network, Radio, Settings, ShieldCheck, Sparkles, Waves } from 'lucide-react';
import type { DiskInfo, NetworkInfo, Snapshot, Settings as MonitorSettings } from './bindings';
import { WindowTitlebar } from './WindowTitlebar';
import { formatBytes, isStale, toChartPoints } from './history';
import { useMonitor } from './useMonitor';
import { SettingsPanel } from './SettingsPanel';
import { StartupPanel } from './StartupPanel';
import { ThermalPanel } from './ThermalPanel';
import { DiskHealthPanel, GpuTemperaturePanel } from './DeviceHealthPanels';
import { TrendChart } from './charts';
import { NetworkQualityPanel } from './NetworkQualityPanel';

type Page = 'Overview' | 'Activity' | 'Hardware' | 'Network' | 'Storage' | 'Settings';
const pages = [
  { name: 'Overview', icon: LayoutDashboard, description: '你的系统，一目了然。' },
  { name: 'Activity', icon: Activity, description: '看看此刻哪些程序正在消耗资源。' },
  { name: 'Hardware', icon: Cpu, description: '查看设备实际提供的硬件信息。' },
  { name: 'Network', icon: Network, description: '每个接口的流量与实时传输。' },
  { name: 'Storage', icon: HardDrive, description: '本机卷的容量与可用空间。' },
  { name: 'Settings', icon: Settings, description: '简单、透明，始终在本机。' },
] as const;
const percent = (value: number | null | undefined) => value == null ? '—' : value.toFixed(1);
const time = (value: number) => new Date(value).toLocaleTimeString('zh-CN', { hour12: false });

function Card({ children, className = '' }: { children: ReactNode; className?: string }) {
  return <section className={`card ${className}`}>{children}</section>;
}
function Label({ icon, children, aside }: { icon: ReactNode; children: ReactNode; aside?: ReactNode }) {
  return <div className="card-label"><span>{icon}{children}</span>{aside}</div>;
}
function Bar({ value, color = 'forest', label }: { value: number | null | undefined; color?: string; label: string }) {
  return <div className={`bar ${color}`} role="meter" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={value ?? undefined} aria-valuetext={value == null ? '暂无数据' : `${percent(value)}%`}><div style={{ width: `${value ?? 0}%` }} /></div>;
}
function DiskRows({ disks }: { disks: DiskInfo[] }) {
  if (!disks.length) return <div className="empty">尚无可读取的磁盘数据</div>;
  return <div className="disk-rows">{disks.map(disk => <div className="disk-row" key={disk.mountPoint}>
    <div className="disk-title"><div className="icon-tile"><HardDrive size={18} /></div><div><strong>{disk.mountPoint}</strong><span>{disk.name || '本地卷'}</span></div><b>{percent(disk.usagePercent)}<small>%</small></b></div>
    <Bar value={disk.usagePercent} color={disk.usagePercent != null && disk.usagePercent > 90 ? 'amber' : 'forest'} label={`${disk.mountPoint} 使用率`} />
    <div className="row-note"><span>{disk.error ?? `${formatBytes(disk.availableBytes)} 可用`}</span><span>共 {disk.totalBytes ? formatBytes(disk.totalBytes) : '—'}</span></div>
  </div>)}</div>;
}
function NetworkRows({ networks }: { networks: NetworkInfo[] }) {
  if (!networks.length) return <div className="empty">尚无可读取的网络接口</div>;
  return <div className="network-rows">{networks.map(net => <div className="interface" key={net.name}>
    <div className="interface-name"><Network size={17} /><strong>{net.name}</strong></div>
    <div className="interface-rates"><span><ArrowDown size={13} />{formatBytes(net.downloadBytesPerSec)}<small>/s</small></span><span><ArrowUp size={13} />{formatBytes(net.uploadBytesPerSec)}<small>/s</small></span></div>
    <div className="row-note"><span>累计接收 {formatBytes(net.receivedBytes)}</span><span>累计发送 {formatBytes(net.transmittedBytes)}</span></div>
    {(net.downloadBytesPerSec == null || net.uploadBytesPerSec == null) && <p className="init-note">初始化或计数器重置，等待下一次采样</p>}
  </div>)}</div>;
}
function NetworkSummary({ snapshot, scope = '所有接口合计' }: { snapshot: Snapshot | null; scope?: string }) {
  const networks = snapshot?.networks ?? [];
  const sum = (key: 'downloadBytesPerSec' | 'uploadBytesPerSec') => networks.length && networks.every(n => n[key] != null) ? networks.reduce((s, n) => s + n[key]!, 0) : null;
  return <div className="transfer-grid">{([{ key: 'downloadBytesPerSec', title: '下载', icon: ArrowDown, color: 'mint' }, { key: 'uploadBytesPerSec', title: '上传', icon: ArrowUp, color: 'lime' }] as const).map(item => <div className="transfer" key={item.key}><span className={item.color}><item.icon size={16} />{item.title}</span><strong>{formatBytes(sum(item.key))}<small>/s</small></strong><span className="muted">{scope}</span></div>)}</div>;
}

export default function App() {
  const [page, setPage] = useState<Page>('Overview');
  const [focusPageHeading, setFocusPageHeading] = useState(false);
  const { state, snapshot, history, error, now, saveSettings, changeAutostart, clearEvents, saving } = useMonitor();
  const interval = state?.settings.samplingIntervalSecs ?? 1;
  const stale = snapshot != null && isStale(snapshot.sampledAtMs, now, interval);
  const [selectedInterface, setSelectedInterface] = useState<string | null>(null);
  const [settingsDraft, setSettingsDraft] = useState<MonitorSettings | null>(null);
  const [networkSaving, setNetworkSaving] = useState(false);
  const [networkMessage, setNetworkMessage] = useState<string | null>(null);
  const [clearingEvents, setClearingEvents] = useState(false);
  const [eventError, setEventError] = useState<string | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { setSelectedInterface(state?.settings.selectedNetworkInterface ?? null); }, [state?.settings.selectedNetworkInterface]);
  useEffect(() => { if (state && !settingsDraft) setSettingsDraft(state.settings); }, [state, settingsDraft]);
  useEffect(() => {
    window.scrollTo({ top: 0, behavior: 'instant' });
    if (focusPageHeading) heading.current?.focus({ preventScroll: true });
  }, [page, focusPageHeading]);
  function navigate(next: Page, keyboard = false) {
    if (next === page) return;
    setFocusPageHeading(keyboard);
    setPage(next);
  }
  const networkHistory = toChartPoints(state?.history ?? [], selectedInterface);
  const selectedSnapshot = snapshot && selectedInterface != null ? { ...snapshot, networks: snapshot.networks.filter(net => net.name === selectedInterface) } : snapshot;
  const effectiveNetwork = state?.settings.selectedNetworkInterface ?? null;
  const overviewSnapshot = snapshot && effectiveNetwork != null ? { ...snapshot, networks: snapshot.networks.filter(net => net.name === effectiveNetwork) } : snapshot;
  async function persist(settings: MonitorSettings) {
    if (!state) return;
    const saved = await saveSettings({ ...state.settings,
      samplingIntervalSecs: settings.samplingIntervalSecs,
      closeBehavior: settings.closeBehavior,
      hideOnAutostart: settings.hideOnAutostart,
      highLoadAlertEnabled: settings.highLoadAlertEnabled,
    });
    setSettingsDraft(saved);
  }
  async function persistInterface() {
    if (!state || networkSaving) return;
    setNetworkSaving(true); setNetworkMessage(null);
    try {
      const saved = await saveSettings({ ...state.settings, selectedNetworkInterface: selectedInterface });
      setSettingsDraft(current => current ? { ...current, selectedNetworkInterface: saved.selectedNetworkInterface } : saved);
      setNetworkMessage('接口选择已保存');
    }
    catch (reason) { setNetworkMessage(`保存失败：${reason instanceof Error ? reason.message : String(reason)}`); }
    finally { setNetworkSaving(false); }
  }
  async function clearEventHistory() {
    if (clearingEvents) return;
    setClearingEvents(true); setEventError(null);
    try { await clearEvents(); }
    catch (reason) { setEventError(`清空失败：${reason instanceof Error ? reason.message : String(reason)}`); }
    finally { setClearingEvents(false); }
  }
  const warming = !snapshot || snapshot.cpu.usagePercent == null;
  const status = error ? '采集失败' : stale ? '数据已过期' : snapshot?.warnings.length ? '部分数据不可用' : warming ? '初始化中' : '实时采集中';
  const fullDisks = snapshot?.disks.filter(disk => disk.usagePercent != null && disk.usagePercent >= 90) ?? [];
  const activeEvents = state?.loadEvents.filter(event => event.status === 'active') ?? [];
  const gpuAdapters = [...new Set(snapshot?.performance.gpuEngines.map(engine => engine.adapter) ?? [])].sort();
  const end = Math.max(snapshot?.sampledAtMs ?? 0, now);
  const currentPage = pages.find(p => p.name === page)!;
  return <MotionConfig reducedMotion="user"><><WindowTitlebar /><div className="app-shell">
    <aside className="sidebar">
      <div className="brand"><img src="/mark.svg" alt="" /><div>Only Monitor<span>SYSTEM MONITOR</span></div></div>
      <div className="nav-heading">WORKSPACE</div>
      <nav aria-label="主导航">{pages.map(item => <button key={item.name} aria-label={item.name} className={page === item.name ? 'nav-item active' : 'nav-item'} onClick={event => navigate(item.name, event.detail === 0)} aria-current={page === item.name ? 'page' : undefined}><item.icon size={19} /><span>{item.name}</span>{page === item.name && <span className="nav-dot" />}</button>)}</nav>
      <div className="sidebar-bottom"><div className="privacy-note"><ShieldCheck size={17} /><span>只在本机，安心运行<small>你的数据始终属于你</small></span></div><div className="version"><span>v0.7.0</span><span className="alpha">ALPHA</span></div></div>
    </aside>
    <main>
      <header className="topbar"><div className="breadcrumb">Workspace <ChevronRight size={13} /><span>{page}</span></div><span className="device"><span className="small-dot" />{snapshot?.hostname ?? '本机设备'}</span></header>
      <div className="content">
        <div className="page-heading"><div><div className="eyebrow">YOUR SYSTEM, IN FOCUS</div><h1 ref={heading} tabIndex={-1}>{page}<span>.</span></h1><p>{currentPage.description}</p></div><div className={`status-pill ${error || stale ? 'warning' : ''}`} role="status"><span className="small-dot" />{status}</div></div>
        {error && <div className="alert" role="alert"><Activity size={17} /><div>{error}{snapshot && <small>当前保留最后一次成功数据：{time(snapshot.sampledAtMs)}</small>}</div></div>}
        {stale && !error && <div className="alert" role="alert">超过 {Math.max(3.5, interval * 3)} 秒未收到新采样，当前显示上次数据。</div>}
        {state?.configError && <div className="alert" role="alert">设置：{state.configError}。当前使用页面中显示的有效配置。</div>}
        {state?.trayError && <div className="alert" role="alert">{state.trayError}</div>}
        {state?.settings.closeBehavior === 'hideToTray' && !state.trayError && <p className="resident-note"><ShieldCheck size={13} />关闭窗口后将继续在托盘监控；右键托盘图标可退出。</p>}
        {!!snapshot?.warnings.length && <div className="alert" role="alert">{snapshot.warnings.join('；')}</div>}
        <motion.div key={page} initial={{ opacity: 0, y: 8 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.22 }}>
          {page === 'Overview' && <>
            <section className="focus-brief" aria-label="本机关注">
              <div className="focus-symbol"><Sparkles size={20} /></div>
              <div className="focus-copy"><span>ONLY MONITOR · 本机关注</span><strong>{!snapshot ? '正在准备你的设备概览' : error || stale ? '采样需要确认' : snapshot.warnings.length || warming ? '部分数据尚未就绪' : fullDisks.length || activeEvents.length ? '有值得留意的变化' : '目前没有触发关注条件'}</strong><p>{!snapshot ? '等待第一份有效采样。' : error || stale ? '请先检查上方的数据状态，旧数据不用于判断当前情况。' : snapshot.warnings.length || warming ? '请查看采样提示，数据恢复后再判断设备状态。' : fullDisks.length || activeEvents.length ? `${fullDisks.length ? `${fullDisks.length} 个卷已使用至少 90%` : ''}${fullDisks.length && activeEvents.length ? ' · ' : ''}${activeEvents.length ? `${activeEvents.length} 个持续高负载事件进行中` : ''}` : '当前检查范围：磁盘空间和持续高负载事件，并非硬件健康诊断。'}</p></div>
              {!error && !stale && fullDisks.length > 0 && <button className="focus-action" onClick={event => navigate('Storage', event.detail === 0)}>查看存储 <ChevronRight size={16} /></button>}
              {!error && !stale && !fullDisks.length && activeEvents.length > 0 && <button className="focus-action" onClick={event => navigate('Activity', event.detail === 0)}>查看事件 <ChevronRight size={16} /></button>}
            </section>
            <div className="section-caption"><span><Radio size={13} /> SYSTEM OVERVIEW</span><span>{interval} 秒采样 <span className="caption-dot">·</span> 后台持续采集</span></div>
            <div className="bento">
              <Card className="cpu-card"><Label icon={<Cpu size={17} />} aside={<span className="tag">CPU</span>}>处理器 · 最近 60 秒</Label><div className="cpu-summary"><div className="cpu-reading"><div className="metric"><strong>{percent(snapshot?.cpu.usagePercent)}</strong><span>%<small>总利用率</small></span></div><p className="hardware-name">{snapshot?.cpu.brand || '等待处理器信息'}<span>{snapshot ? `${snapshot.cpu.logicalCores} 个逻辑核心` : '初始化中'}</span></p></div><TrendChart metric="cpu" history={history} end={end} /></div><div className="card-foot"><span className="legend-dot" />{warming ? '等待两次有效采样' : '最近 60 秒'}<span>最多 60 个采样点</span></div><button className="text-button" onClick={event => navigate('Activity', event.detail === 0)}>查看负载来源 <ChevronRight size={14} /></button></Card>
              <Card className="memory-card"><Label icon={<MemoryStick size={17} />} aside={<span className="tag blue">RAM</span>}>系统内存</Label><div className="memory-reading"><strong>{formatBytes(snapshot?.memory.usagePercent == null ? null : snapshot.memory.usedBytes)}</strong><span>已使用 · {percent(snapshot?.memory.usagePercent)}%</span></div><Bar value={snapshot?.memory.usagePercent} label="系统内存使用率" /><div className="memory-detail"><div><span>已使用</span><strong>{formatBytes(snapshot?.memory.usagePercent == null ? null : snapshot.memory.usedBytes)}</strong></div><div><span>总内存</span><strong>{formatBytes(snapshot?.memory.totalBytes || null)}</strong></div></div><TrendChart metric="memory" compact history={history} end={end} /></Card>
              <Card className="network-card"><Label icon={<Network size={17} />} aside={<span className="tag mint">I/O</span>}>网络流量</Label><NetworkSummary snapshot={overviewSnapshot} scope={effectiveNetwork ?? '所有接口合计'} /><div className="network-art"><Waves size={52} strokeWidth={1} /><span>{overviewSnapshot ? `${overviewSnapshot.networks.length} 个网络接口` : '等待网络接口'}<small>本机接口计数器 · 无外部测速请求</small></span></div><button className="text-button" onClick={event => navigate('Network', event.detail === 0)}>查看接口明细 <ChevronRight size={14} /></button></Card>
              <Card className="storage-card"><Label icon={<HardDrive size={17} />} aside={<span className="muted">{snapshot?.disks.length ?? '—'} 个卷</span>}>存储空间</Label><DiskRows disks={snapshot?.disks.slice(0, 3) ?? []} /><button className="text-button" onClick={event => navigate('Storage', event.detail === 0)}>查看全部 {snapshot?.disks.length ?? '—'} 个卷 <ChevronRight size={14} /></button></Card>
              <Card className="sampling-card"><Label icon={<Radio size={17} />}>采样状态</Label><div className={`sampling-state ${error || stale || snapshot?.warnings.length ? 'sampling-warning' : ''}`}><span className="small-dot" /><strong>{status}</strong></div><div className="sampling-details"><p>最后成功采样 <strong>{snapshot ? time(snapshot.sampledAtMs) : '等待采样'}</strong></p><p>采样间隔 <strong>{interval} 秒</strong></p><p>数据保留 <strong>仅在本机内存</strong></p></div></Card>
              <Card className="cores-card"><Label icon={<Activity size={17} />} aside={<span className="muted">LOGICAL CORES</span>}>核心活动</Label><div className="core-grid">{snapshot?.cpu.cores.map((core, index) => <div className="core" key={core.name}><div><span>#{String(index).padStart(2, '0')}</span><strong>{percent(core.usagePercent)}<small>%</small></strong></div><Bar value={core.usagePercent} label={`${core.name} 利用率`} /></div>) ?? <div className="empty">等待逻辑核心信息</div>}</div></Card>
            </div>
            <div className="future-strip"><div className="future-icon"><Sparkles size={20} /></div><div><strong>更多洞察，即将到来</strong><span>CPU 专属温度 · 完整 S.M.A.R.T. · 本地 AI 分析</span></div><span className="tag">COMING SOON</span></div>
          </>}
          {page === 'Activity' && <div className="detail-layout process-layout">
            <Card className="process-note"><Label icon={<Activity size={17} />} aside={<span className="muted">本次运行</span>}>近期高负载事件</Label>
              <p className="muted detail-note">CPU 或内存达到 90% 持续 30 秒后记录。仅保留最近 20 条，退出后清空；不保存到磁盘或上传。</p>
              {!state ? <div className="empty">等待采样状态</div> : !state.loadEvents.length ? <div className="empty">本次运行暂无持续高负载事件</div> : <div className="event-list">{[...state.loadEvents].reverse().map(event => <details className="event-row" key={event.id}><summary><strong>{event.metric === 'cpu' ? 'CPU' : '内存'}持续高负载</strong><span>{event.status === 'active' ? '进行中' : event.status === 'recovered' ? '已恢复' : '采样中断'}</span><span>{time(event.startedAtMs)}</span><span className="event-detail-action">详情 <ChevronRight size={14} aria-hidden="true" /></span></summary><div className="event-details"><p>开始 {new Date(event.startedAtMs).toLocaleString('zh-CN')} · 确认 {time(event.confirmedAtMs)} · 最近有效采样 {time(event.lastAtMs)}</p><p>已观察 {event.durationSecs} 秒（包括恢复观察） · 峰值 {event.peakPercent.toFixed(1)}%</p><p>确认时占用线索（不代表原因）：{event.processes.length ? event.processes.map(process => `${process.name || '未知进程'}（${event.metric === 'cpu' ? `CPU ${process.cpuUsagePercent == null ? '计算中' : `${process.cpuUsagePercent.toFixed(1)}%`}` : `驻留内存 ${formatBytes(process.residentMemoryBytes)}`}）`).join('、') : '进程数据不可用'}</p></div></details>)}</div>}
              <button className="text-button" type="button" disabled={!state?.loadEvents.length || clearingEvents} onClick={() => void clearEventHistory()}>{clearingEvents ? '正在清空…' : '清空事件'}</button>{eventError && <p className="form-error" role="alert">{eventError}</p>}
            </Card>
            {(['cpu', 'memory'] as const).map(metric => {
              const rows = metric === 'cpu' ? state?.processSnapshot?.topCpu : state?.processSnapshot?.topMemory;
              return <Card key={metric}><Label icon={metric === 'cpu' ? <Cpu size={17} /> : <MemoryStick size={17} />} aside={<span className="muted">TOP 8</span>}>{metric === 'cpu' ? 'CPU 占用较高' : '驻留内存较高'}</Label>
                {!rows?.length ? <div className="empty">{state?.processError ?? '等待进程采样'}</div> : <div className="process-rows">{rows.map(process => <div className="process-row" key={process.pid}><div><strong title={process.name}>{process.name || '未知进程'}</strong><span>PID {process.pid}</span></div><div className="process-stats"><span>CPU {process.cpuUsagePercent == null ? '计算中' : `${process.cpuUsagePercent.toFixed(1)}%`}</span><span>RAM {formatBytes(process.residentMemoryBytes)}</span></div></div>)}</div>}
              </Card>;
            })}
            <Card className="process-note"><Label icon={<LockKeyhole size={17} />}>数据说明</Label><p className="settings-copy">仅显示当前采样中 CPU 或驻留内存排名前 8 的进程。CPU 首次采样显示“计算中”，数值按整机总算力归一；系统保护或访问限制可能导致部分进程信息不可见。进程列表不会写入磁盘或上传；事件可在本次运行内保留最多 3 条进程线索。驻留内存是进程当前驻留集，不保证与系统内存用量相加相等。</p><p className="muted detail-note">进程采样时间：{state?.processSnapshot ? time(state.processSnapshot.sampledAtMs) : '等待采样'} · 随系统采样刷新</p></Card>
          </div>}
          {page === 'Hardware' && <div className="detail-layout hardware-layout">
            <Card><Label icon={<Cpu size={17} />}>显卡温度</Label><GpuTemperaturePanel /></Card>
            <Card><Label icon={<Activity size={17} />}>CPU 温度</Label><p className="sensor-unavailable">暂不可读取</p><p className="settings-copy">尚未接入已验证的 CPU 专属传感器来源；Windows 通用热区不一定对应 CPU。Only Monitor 仅按需查询系统热区，不安装传感器驱动或请求提权。</p><details className="thermal-details"><summary>查看 Windows 系统热区</summary><ThermalPanel /></details></Card>
            <Card className="hardware-wide"><Label icon={<Cpu size={17} />}>GPU 引擎活动</Label><p className="settings-copy">显示 Windows 性能计数器中最活跃的 GPU 引擎，按引擎合计进程的活动时间；它不等同于整张显卡的总利用率。</p>{!snapshot || snapshot.performance.gpuStatus === 'initializing' ? <p className="empty">正在等待两次有效采样</p> : snapshot.performance.gpuStatus === 'unavailable' ? <p className="empty">系统未提供可读取的 GPU 引擎计数器，或当前没有可读取的引擎。</p> : <div className="hardware-list">{snapshot.performance.gpuEngines.map(engine => <div className="hardware-row" key={`${engine.adapter}-${engine.engineIndex}`}><div><strong>{engine.engine} 引擎 #{engine.engineIndex}</strong><span title={engine.adapter}>设备 {gpuAdapters.indexOf(engine.adapter) + 1}</span></div><b>{percent(engine.usagePercent)}%</b></div>)}</div>}<p className="muted detail-note">同一引擎中不同进程计数器相加可能超过 100%；设备编号仅用于区分当前显示的适配器。</p></Card>
          </div>}
          {page === 'Network' && <div className="detail-layout"><Card><Label icon={<Network size={17} />}>实时吞吐与趋势</Label>
            <form className="interface-picker" onSubmit={event => { event.preventDefault(); void persistInterface(); }}><label htmlFor="network-interface">监控接口</label><select id="network-interface" value={selectedInterface ?? ''} disabled={networkSaving || saving || !state} onChange={event => { setSelectedInterface(event.target.value || null); setNetworkMessage(null); }}><option value="">所有接口</option>{snapshot?.networks.map(net => <option key={net.name} value={net.name}>{net.name}</option>)}{selectedInterface && !snapshot?.networks.some(net => net.name === selectedInterface) && <option value={selectedInterface}>{selectedInterface}（不可用）</option>}</select><button className="primary-button" disabled={networkSaving || saving || !state || selectedInterface === effectiveNetwork}>{networkSaving ? '正在保存…' : '保存接口选择'}</button>{selectedInterface !== effectiveNetwork && <span className="preview-state">预览，尚未保存</span>}</form>
            {networkMessage && <p className="muted" role="status">{networkMessage}</p>}
            {selectedInterface && !selectedSnapshot?.networks.length && <p className="form-error" role="status">所选接口当前不可用，等待接口重新出现。</p>}
            <NetworkSummary snapshot={selectedSnapshot} scope={selectedInterface ?? '所有接口合计'} /><TrendChart metric="network" history={networkHistory} end={end} />
            <p className="muted detail-note"><span className="mint">↓ 下载</span>　<span className="lime">↑ 上传</span> · 速率按累计字节差和实际时间计算；初始或中断时断线显示。所有接口合计可能重复计数。</p></Card><Card><Label icon={<Radio size={17} />}>网络接口</Label><NetworkRows networks={snapshot?.networks ?? []} /></Card><Card><Label icon={<Waves size={17} />}>连接质量</Label><NetworkQualityPanel /></Card></div>}
          {page === 'Storage' && <div className="detail-layout"><Card><Label icon={<HardDrive size={17} />}>物理磁盘状态</Label><DiskHealthPanel /></Card><Card><Label icon={<Database size={17} />} aside={<span className="muted">每 30 秒刷新</span>}>系统磁盘</Label><DiskRows disks={snapshot?.disks ?? []} /><p className="muted detail-note">容量采样：{snapshot ? time(snapshot.disksSampledAtMs) : '等待采样'}。同卷重复挂载合并，无法读取的卷显示错误。</p></Card><Card><Label icon={<Activity size={17} />}>磁盘读写</Label>{!snapshot || snapshot.performance.diskStatus === 'initializing' ? <p className="empty">正在等待两次有效采样</p> : snapshot.performance.diskStatus === 'unavailable' ? <p className="empty">系统未提供可读取的逻辑磁盘计数器。</p> : <div className="hardware-list">{snapshot.performance.disks.map(disk => <div className="hardware-row" key={disk.mountPoint}><div><strong>{disk.mountPoint}</strong><span>读取 {formatBytes(disk.readBytesPerSec)}/s · 写入 {formatBytes(disk.writeBytesPerSec)}/s</span></div></div>)}</div>}<p className="muted detail-note">来自 Windows 逻辑磁盘计数器；不支持或无法匹配的卷显示“—”。</p></Card></div>}
          {page === 'Settings' && <div className="detail-layout"><Card><Label icon={<Settings size={17} />}>常驻设置</Label><StartupPanel onChange={changeAutostart} disabled={saving} />{state && settingsDraft ? <SettingsPanel settings={state.settings} draft={settingsDraft} onDraftChange={setSettingsDraft} onReset={() => setSettingsDraft(state.settings)} onSave={persist} disabled={saving} /> : <p className="empty">等待读取有效设置</p>}{state?.alertError && <p className="form-error" role="alert">Windows 通知发送失败：{state.alertError}</p>}</Card><Card><Label icon={<LockKeyhole size={17} />}>本地优先</Label><p className="settings-copy">监控历史仅在进程内存中保留，退出程序即清空。只将偏好写入本机配置目录，不保存系统指标。应用不执行 Shell 命令，不需要管理员权限。</p><div className="settings-row"><span>Ollama Provider</span><strong>Coming Soon · 未启用</strong></div><p className="muted detail-note">仅预留接口，不加载模型，不发起 AI 请求。</p></Card><Card><Label icon={<Activity size={17} />}>关于 Only Monitor</Label><p className="settings-copy">Only Monitor <span className="alpha">v0.7.0 ALPHA</span></p><p className="muted">{snapshot?.os ?? 'Windows 桌面系统监控'}</p><p className="muted detail-note">未来官网 monitor.onlyryx.com · 本次未部署</p></Card></div>}
        </motion.div>
        <footer><span><LockKeyhole size={12} /> LOCAL FIRST <span className="caption-dot">/</span> 不上传监控数据</span><span>采样时间：{snapshot ? time(snapshot.sampledAtMs) : '等待采样'} <span className="caption-dot">·</span> v0.7.0 Alpha</span></footer>
      </div>
    </main>
  </div></></MotionConfig>;
}
