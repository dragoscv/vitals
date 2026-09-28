import { describe, expect, it } from 'vitest';

import type { Snapshot } from '@/lib/metrics';

import { GAP_THRESHOLD_MS, HISTORY_CAPACITY, HistoryCollector } from './history';
import { makeSystem, type SystemOverrides } from './test-fixtures';

function frame(seq: number, overrides: SystemOverrides = {}, timestampMs = seq * 1000): Snapshot {
  return {
    system: makeSystem(overrides),
    processes: new Map(),
    seq,
    elapsedMs: 1000,
    timestampMs,
  };
}

describe('HistoryCollector', () => {
  it('starts empty', () => {
    const collector = new HistoryCollector();
    expect(collector.current.sampleCount).toBe(0);
    expect(collector.current.core.cpu.size).toBe(0);
  });

  it('records one sample per frame', () => {
    const collector = new HistoryCollector();
    collector.push(frame(1, { cpu: { total: 30 } }));
    collector.push(frame(2, { cpu: { total: 40 } }));

    expect(collector.current.sampleCount).toBe(2);
    expect(collector.current.core.cpu.last).toBe(40);
  });

  it('ignores a frame it has already folded, so a replay does not draw one second twice', () => {
    // The metrics hub hands each new subscriber the current frame, and both
    // Dashboard and Performance feed this collector.
    const collector = new HistoryCollector();
    const same = frame(1, { cpu: { total: 30 } });
    collector.push(same);
    collector.push(same);

    expect(collector.current.sampleCount).toBe(1);
    expect(collector.current.core.cpu.size).toBe(1);
  });

  it('bumps the revision so subscribers know to repaint', () => {
    // The buffers are mutated in place, so identity cannot signal a change.
    // This counter is the only cheap signal a chart has.
    const collector = new HistoryCollector();
    const before = collector.current.revision;
    collector.push(frame(1));
    expect(collector.current.revision).toBeGreaterThan(before);
  });

  it('notifies subscribers and stops after unsubscribe', () => {
    const collector = new HistoryCollector();
    let calls = 0;
    const unsubscribe = collector.subscribe(() => {
      calls += 1;
    });

    collector.push(frame(1));
    expect(calls).toBe(1);

    unsubscribe();
    collector.push(frame(2));
    expect(calls).toBe(1);
  });

  it('sums throughput across every device', () => {
    // The totals must agree with the per-device panels whatever set of devices
    // is present, which is why they are computed here and not per widget.
    const collector = new HistoryCollector();
    collector.push(
      frame(1, {
        disks: [
          { id: 0, read: 100, write: 10 },
          { id: 1, read: 400, write: 40 },
        ],
        networks: [
          { id: 0, rx: 50, tx: 5 },
          { id: 1, rx: 150, tx: 15 },
        ],
      }),
    );

    expect(collector.current.core.diskRead.last).toBe(500);
    expect(collector.current.core.diskWrite.last).toBe(50);
    expect(collector.current.core.netRx.last).toBe(200);
    expect(collector.current.core.netTx.last).toBe(20);
  });

  it('keeps a separate series per device', () => {
    const collector = new HistoryCollector();
    collector.push(
      frame(1, {
        disks: [
          { id: 0, activeTime: 10 },
          { id: 3, activeTime: 90 },
        ],
      }),
    );

    expect(collector.current.diskActive.get(0)?.last).toBe(10);
    expect(collector.current.diskActive.get(3)?.last).toBe(90);
  });

  it('does not shift history onto the wrong device when one disappears', () => {
    // The reason series are keyed by id and not by array index: unplugging a
    // drive would otherwise slide every later device's history one chart left.
    const collector = new HistoryCollector();
    collector.push(
      frame(1, {
        disks: [
          { id: 0, activeTime: 10 },
          { id: 1, activeTime: 80 },
        ],
      }),
    );
    collector.push(frame(2, { disks: [{ id: 1, activeTime: 85 }] }));

    expect(collector.current.diskActive.get(1)?.last).toBe(85);
    // Device 0's history is retained rather than reassigned.
    expect(collector.current.diskActive.get(0)?.last).toBe(10);
  });

  it('does not backfill a device that appears mid-session', () => {
    // Padding with zeroes would draw a flat line implying the device was
    // present and idle, which it was not.
    const collector = new HistoryCollector();
    collector.push(frame(1, { disks: [{ id: 0 }] }));
    collector.push(frame(2, { disks: [{ id: 0 }, { id: 5, activeTime: 40 }] }));

    expect(collector.current.diskActive.get(5)?.size).toBe(1);
    expect(collector.current.diskActive.get(0)?.size).toBe(2);
  });

  it('skips a GPU that reports no memory rather than recording zero', () => {
    const collector = new HistoryCollector();
    collector.push(frame(1, { gpus: [{ id: 0, memoryUsed: null }] }));

    expect(collector.current.gpu.get(0)?.size).toBe(1);
    expect(collector.current.gpuMemory.has(0)).toBe(false);
  });

  it('marks a gap when the sampler stalls', () => {
    const collector = new HistoryCollector();
    collector.push(frame(1, {}, 1000));
    collector.push(frame(2, {}, 1000 + GAP_THRESHOLD_MS + 1));

    // Two samples plus the gap marker between them.
    expect(collector.current.core.cpu.size).toBe(3);
    expect(Number.isNaN(collector.current.core.cpu.at(1) ?? 0)).toBe(true);
  });

  it('tolerates ordinary jitter without breaking the line', () => {
    // A tick arriving slightly late is normal. Marking each one as a gap would
    // shred the chart into disconnected fragments.
    const collector = new HistoryCollector();
    collector.push(frame(1, {}, 1000));
    collector.push(frame(2, {}, 2200));

    expect(collector.current.core.cpu.size).toBe(2);
  });

  it('applies a gap to every series at once', () => {
    // The cause is always the sampler. A chart that keeps drawing while its
    // neighbour has a gap suggests the gap is specific to that device.
    const collector = new HistoryCollector();
    collector.push(frame(1, { disks: [{ id: 0 }], gpus: [{ id: 0 }] }));
    collector.markGap();

    expect(Number.isNaN(collector.current.core.memoryUsed.last ?? 0)).toBe(true);
    expect(Number.isNaN(collector.current.diskActive.get(0)?.last ?? 0)).toBe(true);
    expect(Number.isNaN(collector.current.gpu.get(0)?.last ?? 0)).toBe(true);
  });

  it('is bounded — memory does not grow with uptime', () => {
    // The whole reason for a ring buffer. A monitoring tool that leaks while
    // monitoring is self-defeating.
    const collector = new HistoryCollector();
    for (let seq = 1; seq <= HISTORY_CAPACITY * 3; seq += 1) collector.push(frame(seq));

    expect(collector.current.core.cpu.size).toBe(HISTORY_CAPACITY);
    expect(collector.current.sampleCount).toBe(HISTORY_CAPACITY * 3);
  });

  it('computes the memory percentage rather than storing raw bytes', () => {
    const collector = new HistoryCollector();
    collector.push(frame(1, { memory: { total: 1000, used: 250 } }));
    expect(collector.current.core.memoryPercent.last).toBe(25);
  });

  it('does not divide by zero when total memory is unreported', () => {
    const collector = new HistoryCollector();
    collector.push(frame(1, { memory: { total: 0, used: 0 } }));
    expect(collector.current.core.memoryPercent.last).toBe(0);
  });

  it('clears everything on reset', () => {
    const collector = new HistoryCollector();
    collector.push(frame(1, { disks: [{ id: 0 }] }));
    collector.reset();

    expect(collector.current.sampleCount).toBe(0);
    expect(collector.current.core.cpu.size).toBe(0);
    expect(collector.current.diskActive.size).toBe(0);
  });
});
