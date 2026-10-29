import { useEffect, useRef, useState } from 'react';
import { readThermalZones } from './api';
import type { ThermalReport } from './bindings';

export function ThermalPanel() {
  const [busy, setBusy] = useState(false);
  const [slow, setSlow] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<ThermalReport | null>(null);
  const mounted = useRef(true);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; clearTimeout(timer.current); }; }, []);
  async function read() {
    if (busy) return;
    setBusy(true); setSlow(false); setError(null); setReport(null);
    timer.current = setTimeout(() => { if (mounted.current) setSlow(true); }, 8000);
    try { const next = await readThermalZones(); if (mounted.current) setReport(next); }
    catch (reason) { if (mounted.current) setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { clearTimeout(timer.current); if (mounted.current) { setBusy(false); setSlow(false); } }
  }
  return <><p className="settings-copy">按需读取 Windows 提供的通用温度热区。热区不一定对应 CPU 或 GPU，也可能因设备或权限限制不可用。</p>
    <button className="primary-button" type="button" disabled={busy} onClick={() => void read()}>{busy ? '正在读取…' : '读取系统热区'}</button>
    {slow && <p className="muted" role="status">系统响应较慢，读取仍在后台进行；你可以继续使用其他页面。</p>}
    {error && <p className="form-error" role="alert">{error}</p>}
    {report && <div className="hardware-list">{report.readings.map((reading, index) => <div className="hardware-row" key={`${reading.label}-${index}`}><div><strong>系统热区 {index + 1}</strong><span>{reading.label}</span></div><b>{reading.celsius.toFixed(1)} °C</b></div>)}<p className="muted">读取时间：{new Date(report.sampledAtMs).toLocaleTimeString('zh-CN', { hour12: false })}</p></div>}
  </>;
}
