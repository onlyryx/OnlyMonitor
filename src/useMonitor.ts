import { useCallback, useEffect, useRef, useState } from 'react';
import type { MonitorState } from './bindings';
import { clearLoadEvents, getMonitorState, listenVisibility, saveSettings as writeSettings, setAutostart } from './api';
import { toChartPoints } from './history';

export function useMonitor() {
  const [state, setState] = useState<MonitorState | null>(null);
  const [transportError, setTransportError] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());
  const [saving, setSaving] = useState(false);
  const savingRef = useRef(false);
  const settingsRevision = useRef(0);
  const effectiveState = useRef<MonitorState | null>(null);
  const mounted = useRef(false);
  const requestRef = useRef<() => void>(() => {});
  const refresh = useCallback(() => requestRef.current(), []);
  const clearEvents = useCallback(async () => {
    await clearLoadEvents();
    refresh();
  }, [refresh]);
  const saveSettings = useCallback(async (settings: MonitorState['settings']) => {
    if (savingRef.current) throw new Error('已有设置正在保存，请稍后再试');
    if (!effectiveState.current) throw new Error('有效设置尚未读取');
    savingRef.current = true; settingsRevision.current++; setSaving(true);
    try {
      const saved = await writeSettings(settings);
      if (mounted.current && effectiveState.current) {
        const next = { ...effectiveState.current, settings: saved, configError: null };
        effectiveState.current = next; setState(next);
      }
      return saved;
    } finally {
      savingRef.current = false;
      if (mounted.current) { setSaving(false); refresh(); }
    }
  }, [refresh]);
  const changeAutostart = useCallback(async (enabled: boolean) => {
    if (savingRef.current) throw new Error('已有设置正在保存，请稍后再试');
    savingRef.current = true; setSaving(true);
    try { return await setAutostart(enabled); }
    finally {
      savingRef.current = false;
      if (mounted.current) { setSaving(false); refresh(); }
    }
  }, [refresh]);
  useEffect(() => {
    mounted.current = true;
    let stopped = false;
    let visible = true;
    let running = false;
    let queued = false;
    let epoch = 0;
    let timeout: ReturnType<typeof setTimeout> | undefined;
    let clock: ReturnType<typeof setInterval> | undefined;
    let unlisten: (() => void) | undefined;
    const active = () => !stopped && visible && document.visibilityState !== 'hidden';
    function request() {
      clearTimeout(timeout);
      if (!active() || savingRef.current) return;
      if (running) { queued = true; return; }
      void read();
    }
    async function read() {
      running = true;
      const readEpoch = epoch;
      const readSettingsRevision = settingsRevision.current;
      try {
        const next = await getMonitorState();
        if (!active() || readEpoch !== epoch || readSettingsRevision !== settingsRevision.current) return;
        effectiveState.current = next;
        setState(next);
        setTransportError(null);
        setNow(Date.now());
        if (!next.windowVisible) { visible = false; activity(); }
      } catch (reason) {
        if (active() && readEpoch === epoch) setTransportError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        running = false;
        if (active()) {
          if (queued) { queued = false; request(); }
          else timeout = setTimeout(request, 1000);
        }
      }
    }
    function activity() {
      epoch++;
      clearTimeout(timeout); clearInterval(clock);
      queued = false;
      if (active()) { setNow(Date.now()); clock = setInterval(() => setNow(Date.now()), 1000); request(); }
    }
    requestRef.current = request;
    document.addEventListener('visibilitychange', activity);
    void listenVisibility(next => { visible = next; activity(); }).then(off => {
      if (stopped) { off(); return; }
      unlisten = off; activity();
    }).catch(reason => { if (!stopped) { setTransportError(String(reason)); activity(); } });
    return () => {
      stopped = true; mounted.current = false; requestRef.current = () => {};
      clearTimeout(timeout); clearInterval(clock); unlisten?.();
      document.removeEventListener('visibilitychange', activity);
    };
  }, []);
  return { state, snapshot: state?.latest ?? null,
    history: toChartPoints(state?.history ?? [], state?.settings.selectedNetworkInterface ?? null),
    error: transportError ?? state?.lastError ?? null, now, refresh,
    saveSettings, changeAutostart, clearEvents, saving };
}
