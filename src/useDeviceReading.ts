import { useCallback, useEffect, useRef, useState } from 'react';
import { listenVisibility } from './api';

export function useDeviceReading<T>(read: (signal: AbortSignal) => Promise<T>, intervalMs: number) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [slow, setSlow] = useState(false);
  const [autoPaused, setAutoPaused] = useState(false);
  const requestRef = useRef<() => void>(() => {});
  const refresh = useCallback(() => requestRef.current(), []);
  useEffect(() => {
    let stopped = false, nativeVisible = true, running = false, queued = false, epoch = 0, automatic = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let slowTimer: ReturnType<typeof setTimeout> | undefined;
    let unlisten: (() => void) | undefined;
    let controller: AbortController | undefined;
    const active = () => !stopped && nativeVisible && document.visibilityState !== 'hidden';
    function scheduleSlow() {
      clearTimeout(slowTimer);
      slowTimer = setTimeout(() => { if (active() && running) setSlow(true); }, 8000);
    }
    function request() {
      clearTimeout(timer);
      if (!active()) return;
      if (running) { queued = true; return; }
      void run();
    }
    async function run() {
      running = true; const current = epoch;
      const requestController = new AbortController();
      controller = requestController;
      setBusy(true); setSlow(false);
      scheduleSlow();
      try { const next = await read(requestController.signal); if (active() && current === epoch) { setData(next); setError(null); } }
      catch (reason) { if (active() && current === epoch) { setData(null); setError(reason instanceof Error ? reason.message : String(reason)); } }
      finally {
        if (controller === requestController) controller = undefined;
        clearTimeout(slowTimer); running = false;
        if (active()) {
          setBusy(false); setSlow(false);
          if (queued) { queued = false; request(); }
          else if (automatic) timer = setTimeout(request, intervalMs);
        }
      }
    }
    function activity() {
      epoch++; clearTimeout(timer); clearTimeout(slowTimer); queued = false;
      controller?.abort();
      setSlow(false);
      if (active() && running) scheduleSlow();
      if (active()) request();
    }
    requestRef.current = request;
    document.addEventListener('visibilitychange', activity);
    void listenVisibility(next => { nativeVisible = next; activity(); }).then(off => {
      if (stopped) off(); else unlisten = off;
    }).catch(() => { if (!stopped) { automatic = false; clearTimeout(timer); setAutoPaused(true); } });
    request();
    return () => { stopped = true; controller?.abort(); requestRef.current = () => {}; clearTimeout(timer); clearTimeout(slowTimer); unlisten?.(); document.removeEventListener('visibilitychange', activity); };
  }, [read, intervalMs]);
  return { data, error, busy, slow, autoPaused, refresh };
}
