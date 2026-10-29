import { useEffect, useRef, useState } from 'react';
import type { StartupStatus } from './bindings';
import { getStartupStatus, listenVisibility } from './api';

export function StartupPanel({ onChange, disabled }: { onChange: (enabled: boolean) => Promise<StartupStatus>; disabled: boolean }) {
  const [status, setStatus] = useState<StartupStatus>({ registered: null, error: null });
  const [busy, setBusy] = useState(false);
  const alive = useRef(false);
  const changing = useRef(false);
  const revision = useRef(0);
  async function refresh() {
    if (changing.current) return;
    const current = ++revision.current;
    try {
      const next = await getStartupStatus();
      if (alive.current && current === revision.current) setStatus(next);
    } catch (reason) {
      if (alive.current && current === revision.current) setStatus({ registered: null, error: String(reason) });
    }
  }
  useEffect(() => {
    alive.current = true;
    let stopped = false;
    let off: (() => void) | undefined;
    const restored = () => { if (document.visibilityState !== 'hidden') void refresh(); };
    void refresh();
    document.addEventListener('visibilitychange', restored);
    void listenVisibility(visible => { if (visible && !stopped) restored(); }).then(unlisten => {
      if (stopped) unlisten(); else off = unlisten;
    }).catch(reason => {
      if (!stopped) setStatus(previous => ({ ...previous, error: `恢复监听失败，可手动刷新：${String(reason)}` }));
    });
    return () => {
      stopped = true; alive.current = false; revision.current++;
      off?.(); document.removeEventListener('visibilitychange', restored);
    };
  }, []);
  async function change() {
    if (changing.current || disabled) return;
    changing.current = true; setBusy(true);
    const current = ++revision.current;
    try {
      const next = await onChange(status.registered !== true);
      if (alive.current && current === revision.current) setStatus(next);
    } catch (reason) {
      if (alive.current && current === revision.current) setStatus({ registered: null, error: String(reason) });
    } finally {
      changing.current = false;
      if (alive.current) setBusy(false);
    }
  }
  return <div>
    <div className="settings-row"><span>登录 Windows 时启动</span><strong aria-live="polite">{status.registered === null ? '无法读取登记状态' : status.registered ? '已注册' : '未注册'}</strong></div>
    <p className="muted detail-note">启动登记立即生效，与下方设置保存分开操作。仅管理当前用户；Windows 启动应用开关或系统策略仍可能阻止启动，本程序不会覆盖系统批准状态。</p>
    <div className="startup-actions"><button className="primary-button" type="button" disabled={disabled || busy} onClick={() => { void change(); }}>{busy ? '正在更新…' : status.registered ? '关闭登录启动' : '启用登录启动'}</button><button className="text-button" type="button" disabled={disabled || busy} onClick={() => { void refresh(); }}>刷新启动状态</button></div>
    {status.error && <p className="form-error" role="alert">{status.error}</p>}
  </div>;
}
