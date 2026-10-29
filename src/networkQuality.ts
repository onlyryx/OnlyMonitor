import type { PingSample } from './bindings';
export type { PingOutcome, PingSample } from './bindings';

export type NetworkQualitySummary = {
  attempts: number;
  missedReplies: number;
  noReplyPercent: number | null;
  latestRttMs: number | null;
  averageRttMs: number | null;
  maximumRttMs: number | null;
  jitterMs: number | null;
  jitterPairs: number;
};

const WINDOW_SIZE = 60;
const usableRtt = (sample: PingSample) => sample.outcome === 'reply' && sample.rttMs != null && Number.isFinite(sample.rttMs) && sample.rttMs >= 0;

export function summarizeNetworkQuality(samples: readonly PingSample[]): NetworkQualitySummary {
  const window = samples.slice(-WINDOW_SIZE);
  const rtts = window.filter(usableRtt).map(sample => sample.rttMs!);
  const differences: number[] = [];
  for (let index = 1; index < window.length; index++) {
    const previous = window[index - 1];
    const current = window[index];
    if (usableRtt(previous) && usableRtt(current)) differences.push(Math.abs(current.rttMs! - previous.rttMs!));
  }
  const missedReplies = window.filter(sample => sample.outcome !== 'reply').length;
  const last = window[window.length - 1];
  return {
    attempts: window.length,
    missedReplies,
    noReplyPercent: window.length ? missedReplies / window.length * 100 : null,
    latestRttMs: last && usableRtt(last) ? last.rttMs : null,
    averageRttMs: rtts.length ? rtts.reduce((sum, rtt) => sum + rtt, 0) / rtts.length : null,
    maximumRttMs: rtts.length ? Math.max(...rtts) : null,
    jitterMs: differences.length ? differences.reduce((sum, difference) => sum + difference, 0) / differences.length : null,
    jitterPairs: differences.length,
  };
}
