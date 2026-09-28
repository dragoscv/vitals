/**
 * The session-long frame listener.
 *
 * Found live 2026-09-28: every source ran its own `listen()` and reconciled
 * from an empty map, so a screen opened between keyframes folded ~90 changed
 * rows onto nothing and showed 116 processes out of 780.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Frame, Process } from '@vitals/protocol';

import type * as Metrics from './metrics';

type Handler = (event: { payload: unknown }) => void;
const handlers = vi.hoisted(() => new Map<string, Handler>());
const listen = vi.hoisted(() =>
  vi.fn((name: string, handler: Handler) => {
    handlers.set(name, handler);
    return Promise.resolve(() => undefined);
  }),
);
const invoke = vi.hoisted(() => vi.fn(() => Promise.resolve()));

vi.mock('@tauri-apps/api/event', () => ({ listen }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

function proc(pid: number): Process {
  return { key: { pid, startTime: 1 }, name: `p${pid}` } as unknown as Process;
}

function frame(seq: number, payload: Frame['payload']): { payload: Frame } {
  const value: Frame = { seq, elapsedMs: 1000, timestampMs: seq * 1000, payload };
  return { payload: value };
}

const system = {} as Frame['payload']['system'];
const emit = (f: { payload: Frame }): void => handlers.get('vitals://frame')?.(f);

let metrics: typeof Metrics;

beforeEach(async () => {
  vi.resetModules();
  handlers.clear();
  listen.mockClear();
  invoke.mockClear();
  metrics = await import('./metrics');
});

describe('the metrics hub', () => {
  it('gives a subscriber that arrives late the whole process map at once', async () => {
    await metrics.subscribeToMetrics({ onSnapshot: () => undefined });
    emit(frame(1, { kind: 'keyframe', system, processes: [proc(1), proc(2), proc(3)] }));
    emit(frame(2, { kind: 'delta', system, changed: [proc(4)], exited: [2] }));

    const seen: number[] = [];
    await metrics.subscribeToMetrics({ onSnapshot: (s) => seen.push(s.processes.size) });

    // 1, 3, 4 — not just the one row the latest delta carried.
    expect(seen).toEqual([3]);
  });

  it('installs one listener for the session, however many screens subscribe and leave', async () => {
    const stop = await metrics.subscribeToMetrics({ onSnapshot: () => undefined });
    stop();
    await metrics.subscribeToMetrics({ onSnapshot: () => undefined });

    expect(listen.mock.calls.filter(([name]) => name === 'vitals://frame')).toHaveLength(1);
  });

  it('asks for a keyframe when it starts, and folds no delta before one arrives', async () => {
    const sizes: number[] = [];
    await metrics.subscribeToMetrics({ onSnapshot: (s) => sizes.push(s.processes.size) });

    expect(invoke).toHaveBeenCalledWith('request_keyframe');

    emit(frame(7, { kind: 'delta', system, changed: [proc(9)], exited: [] }));
    expect(sizes).toEqual([]);
    expect(metrics.latestMetrics()).toBeNull();

    emit(frame(8, { kind: 'keyframe', system, processes: [proc(1), proc(2)] }));
    expect(sizes).toEqual([2]);
  });
});
