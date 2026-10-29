import { useEffect, useRef, useState, type FormEvent } from 'react';
import { listenVisibility, pingOnce, resolvePingTarget } from './api';
import { summarizeNetworkQuality, type PingSample } from './networkQuality';

const WINDOW_SIZE = 60;
const PING_TIMEOUT_MS = 1000;
const INTERRUPTION_MS = 10_000;
const NEXT_ATTEMPT_DELAY_MS = 1000;

type Phase = 'idle' | 'resolving' | 'running' | 'paused' | 'stopped' | 'interrupted' | 'error';
type CancelReason = 'manual' | 'hidden' | 'unmount';
type WaitingFor = 'dns' | 'ping' | null;
type PendingDelay = { id: ReturnType<typeof setTimeout>; finish: () => void };

const errorMessage = (reason: unknown) => reason instanceof Error ? reason.message : String(reason);
const elapsedProblem = (startedAt: number, endedAt: number): 'clock' | 'long' | null => {
  const elapsed = endedAt - startedAt;
  return elapsed < 0 ? 'clock' : elapsed > INTERRUPTION_MS ? 'long' : null;
};
const statusLabel: Record<PingSample['outcome'], string> = {
  reply: '已收到 Echo Reply',
  timeout: '超时，未收到 Echo Reply',
  unreachable: '不可达，未收到 Echo Reply',
  other: '其他结果，未收到 Echo Reply',
};

export function NetworkQualityPanel() {
  const [target, setTarget] = useState('');
  const [resolvedIp, setResolvedIp] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>('idle');
  const [samples, setSamples] = useState<PingSample[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [visibilityReady, setVisibilityReady] = useState(false);
  const [waitingFor, setWaitingFor] = useState<WaitingFor>(null);
  const generation = useRef(0);
  const active = useRef(false);
  const visible = useRef(false);
  const visibilityReadyRef = useRef(false);
  const visibilityFailed = useRef(false);
  const pendingDelay = useRef<PendingDelay | null>(null);
  const operationController = useRef<AbortController | null>(null);
  const cancelRef = useRef<(reason: CancelReason) => void>(() => {});

  function cancel(reason: CancelReason) {
    if (!active.current) return;
    active.current = false;
    generation.current++;
    operationController.current?.abort();
    operationController.current = null;
    setWaitingFor(null);
    if (pendingDelay.current) {
      clearTimeout(pendingDelay.current.id);
      pendingDelay.current.finish();
      pendingDelay.current = null;
    }
    if (reason === 'manual') { setError(null); setPhase('stopped'); }
    if (reason === 'hidden') { setError(null); setPhase('paused'); }
  }
  cancelRef.current = cancel;

  useEffect(() => {
    let disposed = false;
    let nativeVisible: boolean | null = null;
    let unlisten: (() => void) | undefined;
    function syncVisibility() {
      const isVisible = document.visibilityState !== 'hidden' && nativeVisible !== false;
      visible.current = visibilityReadyRef.current && isVisible;
      if (!isVisible) cancelRef.current('hidden');
    }
    function onDocumentVisibility() { syncVisibility(); }
    document.addEventListener('visibilitychange', onDocumentVisibility);
    void listenVisibility(next => {
      nativeVisible = next;
      syncVisibility();
    }).then(off => {
      if (disposed) { off(); return; }
      unlisten = off;
      visibilityReadyRef.current = true;
      setVisibilityReady(true);
      syncVisibility();
    }).catch(reason => {
      if (disposed) return;
      visibilityFailed.current = true;
      visibilityReadyRef.current = false;
      visible.current = false;
      cancelRef.current('hidden');
      setError(`无法监听窗口状态，已停用探测：${errorMessage(reason)}`);
      setPhase('error');
    });
    return () => {
      disposed = true;
      visibilityReadyRef.current = false;
      visible.current = false;
      cancelRef.current('unmount');
      unlisten?.();
      document.removeEventListener('visibilitychange', onDocumentVisibility);
    };
  }, []);

  async function start(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const input = target.trim();
    if (!input || active.current || !visibilityReadyRef.current || !visible.current) return;
    const id = ++generation.current;
    const controller = new AbortController();
    operationController.current = controller;
    const isCurrent = () => generation.current === id && active.current && visible.current;
    const stopForElapsedProblem = (problem: 'clock' | 'long', resolving = false) => {
      active.current = false;
      setWaitingFor(null);
      setPhase(problem === 'long' && resolving ? 'error' : 'interrupted');
      setError(problem === 'clock'
        ? `系统时钟变化，${resolving ? '目标解析' : '采样'}已停止，请重新开始。`
        : resolving ? '目标解析耗时过长，请重新开始。' : '采样中断，请重新开始。');
    };
    active.current = true;
    setSamples([]);
    setResolvedIp(null);
    setError(null);
    setWaitingFor(null);
    setPhase('resolving');
    const resolutionStartedAt = Date.now();
    try {
      const ip = await resolvePingTarget(input, { signal: controller.signal, onWaiting: () => setWaitingFor('dns') });
      if (!isCurrent()) return;
      setWaitingFor(null);
      const resolutionProblem = elapsedProblem(resolutionStartedAt, Date.now());
      if (resolutionProblem) {
        stopForElapsedProblem(resolutionProblem, true);
        return;
      }
      setResolvedIp(ip);
      setPhase('running');
      while (isCurrent()) {
        if (!isCurrent()) return;
        const requestStartedAt = Date.now();
        let sample: PingSample;
        try {
          sample = await pingOnce(ip, { signal: controller.signal, onWaiting: () => setWaitingFor('ping') });
        } catch (reason) {
          if (!isCurrent()) return;
          setWaitingFor(null);
          const requestProblem = elapsedProblem(requestStartedAt, Date.now());
          if (requestProblem) {
            stopForElapsedProblem(requestProblem);
            return;
          }
          active.current = false;
          setError(errorMessage(reason));
          setPhase('error');
          return;
        }
        if (!isCurrent()) return;
        setWaitingFor(null);
        const completedAt = Date.now();
        const requestProblem = elapsedProblem(requestStartedAt, completedAt);
        if (requestProblem) {
          stopForElapsedProblem(requestProblem);
          return;
        }
        setSamples(current => [...current.slice(-(WINDOW_SIZE - 1)), sample]);
        await new Promise<void>(finish => {
          const id = setTimeout(() => {
            if (pendingDelay.current?.id === id) pendingDelay.current = null;
            finish();
          }, NEXT_ATTEMPT_DELAY_MS);
          pendingDelay.current = { id, finish };
        });
        if (!isCurrent()) return;
        const delayProblem = elapsedProblem(completedAt, Date.now());
        if (delayProblem) {
          stopForElapsedProblem(delayProblem);
          return;
        }
      }
    } catch (reason) {
      if (!isCurrent()) return;
      setWaitingFor(null);
      const resolutionProblem = elapsedProblem(resolutionStartedAt, Date.now());
      if (resolutionProblem) {
        stopForElapsedProblem(resolutionProblem, true);
        return;
      }
      active.current = false;
      setError(errorMessage(reason));
      setPhase('error');
    }
  }

  const activeNow = phase === 'resolving' || phase === 'running';
  const summary = summarizeNetworkQuality(samples);
  const phaseText: Record<Phase, string> = {
    idle: '尚未开始',
    resolving: '正在解析目标；尚未发起 Ping',
    running: '正在探测',
    paused: '窗口不可见，探测已暂停',
    stopped: '探测已停止',
    interrupted: '采样中断，请重新开始',
    error: '探测无法继续',
  };
  const currentStatus = waitingFor === 'dns' ? '正在等待上一次目标解析结束' : waitingFor === 'ping' ? '正在等待上一次 Ping 请求结束' : phaseText[phase];
  const latest = summary.latestRttMs == null ? '—' : `${summary.latestRttMs} ms`;
  const average = summary.averageRttMs == null ? '—' : `${summary.averageRttMs.toFixed(1)} ms`;
  const jitter = summary.jitterMs == null ? '—' : `${summary.jitterMs.toFixed(1)} ms`;
  const lastSample = samples[samples.length - 1];

  return <div className="network-quality-panel">
    <form className="network-probe-controls" onSubmit={event => void start(event)}>
      <div className="network-probe-target">
        <label htmlFor="network-quality-target">IPv4 地址或域名</label>
        <input id="network-quality-target" type="text" value={target} disabled={activeNow}
          aria-describedby="network-quality-disclosure"
          placeholder="例如 1.1.1.1 或 example.com" autoComplete="off" spellCheck={false}
          onChange={event => {
            setTarget(event.target.value);
            setResolvedIp(null);
            setSamples([]);
            if (!visibilityFailed.current) { setError(null); setPhase('idle'); }
          }} />
      </div>
      <div className="network-probe-actions">
        <button className="primary-button" type="submit" disabled={!target.trim() || !visibilityReady || activeNow}>
          {phase === 'stopped' || phase === 'paused' || phase === 'interrupted' || phase === 'error' ? '重新开始' : '开始探测'}
        </button>
        {activeNow && <button className="secondary-button" type="button" onClick={() => cancelRef.current('manual')}>停止探测</button>}
      </div>
    </form>
    <p id="network-quality-disclosure" className="muted network-quality-note">不会自动探测。点击“开始探测”后才会使用系统 DNS 解析域名并向目标发送 ICMP 请求；目标可以是外部主机，输入和结果仅保留在当前应用内存中，不上传监控数据。</p>
    <p className="network-probe-state" role="status" aria-live="polite">
      {currentStatus}{resolvedIp ? ` · ${resolvedIp}` : ''}
    </p>
    {lastSample && <p className="network-quality-last-result">最近结果：{statusLabel[lastSample.outcome]}{lastSample.rttMs == null ? '' : ` · ${lastSample.rttMs} ms`}{lastSample.detail ? ` · ${lastSample.detail}` : ''} · 采样时间 {new Date(lastSample.sampledAtMs).toLocaleTimeString('zh-CN', { hour12: false })}</p>}
    {!visibilityReady && !error && <p className="muted network-quality-note">正在确认窗口状态…</p>}
    {error && <p className="form-error network-probe-error" role="alert">{error}</p>}
    <div className="network-quality-metrics" aria-label="连接质量统计">
      <div className="network-quality-metric"><span>最新延迟</span><strong className="quality-latest">{latest}</strong></div>
      <div className="network-quality-metric"><span>平均延迟</span><strong>{average}</strong></div>
      <div className="network-quality-metric"><span>ICMP 未回复率</span><strong>{summary.noReplyPercent == null ? '—' : `${summary.noReplyPercent.toFixed(1)}%`}</strong><small><span className="quality-no-reply-count">{summary.missedReplies}/{summary.attempts}</span> 已完成的 ICMP 请求</small></div>
      <div className="network-quality-metric"><span>RTT 波动</span><strong>{jitter}</strong><small>{summary.jitterPairs} 个连续成功样本对</small></div>
    </div>
    <p className="network-quality-history-label">最近 {samples.length} 次探测结果（最多保留 {WINDOW_SIZE} 次）</p>
    <div className="network-quality-history" role="list" aria-label={`最近 ${samples.length} 次探测结果`}>
      {samples.map((sample, index) => <span key={`${sample.sampledAtMs}-${index}`} role="listitem"
        className={`network-quality-sample ${sample.outcome}`} title={`${new Date(sample.sampledAtMs).toLocaleTimeString('zh-CN', { hour12: false })} · ${statusLabel[sample.outcome]}${sample.rttMs == null ? '' : ` · ${sample.rttMs} ms`}`}
        aria-label={`${statusLabel[sample.outcome]}${sample.rttMs == null ? '' : `，${sample.rttMs} 毫秒`}${sample.detail ? `，${sample.detail}` : ''}`} />)}
    </div>
    <div className="network-quality-legend" aria-label="探测结果图例">
      <span><i className="reply" />已收到 Echo Reply</span>
      <span><i className="timeout" />超时</span>
      <span><i className="unreachable" />不可达</span>
      <span><i className="other" />其他结果</span>
    </div>
    <p className="muted network-quality-note">Ping 使用 Windows 路由选择，不受上方流量统计接口筛选影响。每次请求完成后等待 1 秒；系统 Ping 超时设为 {PING_TIMEOUT_MS / 1000} 秒。停止或隐藏后，已发出的解析或单次请求可能仍在完成，但迟到结果不会继续探测。RTT 波动按连续成功样本的往返时间差绝对值平均计算。ICMP 可能被过滤；未收到 Echo Reply 不代表互联网或应用不可用。</p>
  </div>;
}
