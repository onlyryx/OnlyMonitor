import type { Settings } from './bindings';
import { useState } from 'react';

export function SettingsPanel({ settings, draft, onDraftChange, onReset, onSave, disabled = false }: { settings: Settings; draft: Settings; onDraftChange: (settings: Settings) => void; onReset: () => void; onSave: (settings: Settings) => Promise<void>; disabled?: boolean }) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const dirty = draft.samplingIntervalSecs !== settings.samplingIntervalSecs || draft.closeBehavior !== settings.closeBehavior || draft.hideOnAutostart !== settings.hideOnAutostart || draft.highLoadAlertEnabled !== settings.highLoadAlertEnabled;
  async function submit() {
    if (saving || !dirty) return;
    setSaving(true); setError(null);
    try { await onSave(draft); }
    catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
    finally { setSaving(false); }
  }
  return <form onSubmit={event => { event.preventDefault(); void submit(); }}>
    <fieldset disabled={saving || disabled}>
      <div className="settings-row"><label htmlFor="sampling-interval">采样间隔</label><select id="sampling-interval" value={draft.samplingIntervalSecs} onChange={event => { setError(null); onDraftChange({ ...draft, samplingIntervalSecs: Number(event.target.value) }); }}>{[1, 2, 5].map(value => <option key={value} value={value}>{value} 秒</option>)}</select></div>
      <div className="settings-row"><label htmlFor="close-behavior">关闭主窗口时</label><select id="close-behavior" value={draft.closeBehavior} onChange={event => { setError(null); onDraftChange({ ...draft, closeBehavior: event.target.value as Settings['closeBehavior'] }); }}><option value="hideToTray">隐藏到托盘，继续监控</option><option value="exit">退出程序</option></select></div>
      <div className="settings-row"><span>历史窗口</span><strong>最近 60 秒 / 最多 60 点</strong></div>
      <div className="settings-row"><label htmlFor="hide-on-autostart">登录启动时隐藏到托盘</label><input id="hide-on-autostart" type="checkbox" checked={draft.hideOnAutostart} onChange={event => { setError(null); onDraftChange({ ...draft, hideOnAutostart: event.target.checked }); }} /></div>
      <p className="muted detail-note">隐藏偏好随下方按钮保存；仅在登录启动时使用。手动打开总会显示窗口，托盘不可用时也会显示。</p>
      <div className="settings-row"><span>磁盘刷新</span><strong>30 秒</strong></div>
      <div className="settings-row"><label htmlFor="high-load-alert">持续高负载提醒</label><input id="high-load-alert" type="checkbox" checked={draft.highLoadAlertEnabled} onChange={event => { setError(null); onDraftChange({ ...draft, highLoadAlertEnabled: event.target.checked }); }} /></div>
      <p className="muted detail-note">默认关闭。开启后，CPU 或内存达到 90% 并持续 30 秒时发送一次 Windows 通知；降至 80% 以下持续 60 秒后重新布防。</p>
      <p className="muted detail-note">当前生效：{settings.samplingIntervalSecs} 秒；{settings.closeBehavior === 'hideToTray' ? '关闭后隐藏到托盘' : '关闭后退出'}。使用托盘菜单可随时退出。</p>
      <div className="draft-actions"><span className={dirty ? 'draft-dirty' : 'form-success'} role="status">{dirty ? '有未保存更改' : '设置已保存并生效'}</span><button className="text-button" type="button" disabled={!dirty} onClick={onReset}>恢复当前生效设置</button><button className="primary-button" type="submit" disabled={!dirty}>{saving ? '正在保存…' : '保存设置'}</button></div>
      {dirty && <p className="muted draft-note">草稿仅在应用运行期间保留；退出应用后会丢弃。</p>}
    </fieldset>
    {error && <p className="form-error" role="alert">{error}</p>}
  </form>;
}
