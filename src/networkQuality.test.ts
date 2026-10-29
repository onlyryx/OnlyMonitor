import { expect, it } from 'vitest';
import { summarizeNetworkQuality, type PingSample } from './networkQuality';

const reply = (rttMs: number | null, sampledAtMs: number): PingSample => ({ sampledAtMs, outcome: 'reply', rttMs, detail: null });
const result = (outcome: PingSample['outcome'], sampledAtMs: number): PingSample => ({ sampledAtMs, outcome, rttMs: null, detail: null });

it('counts every completed non-reply while jitter only compares adjacent replies', () => {
  const summary = summarizeNetworkQuality([
    reply(10, 1), reply(20, 2), result('timeout', 3), reply(30, 4), reply(35, 5),
    result('unreachable', 6), result('other', 7), reply(40, 8),
  ]);

  expect(summary).toEqual({
    attempts: 8,
    missedReplies: 3,
    noReplyPercent: 37.5,
    latestRttMs: 40,
    averageRttMs: 27,
    maximumRttMs: 40,
    jitterMs: 7.5,
    jitterPairs: 2,
  });
});

it('keeps empty and no-RTT statistics null and limits the window to the last 60 samples', () => {
  expect(summarizeNetworkQuality([])).toEqual({
    attempts: 0,
    missedReplies: 0,
    noReplyPercent: null,
    latestRttMs: null,
    averageRttMs: null,
    maximumRttMs: null,
    jitterMs: null,
    jitterPairs: 0,
  });
  expect(summarizeNetworkQuality([result('timeout', 1)])).toMatchObject({
    attempts: 1,
    missedReplies: 1,
    noReplyPercent: 100,
    latestRttMs: null,
    averageRttMs: null,
    maximumRttMs: null,
    jitterMs: null,
    jitterPairs: 0,
  });
  expect(summarizeNetworkQuality(Array.from({ length: 61 }, (_, i) => result('timeout', i))).attempts).toBe(60);
});
