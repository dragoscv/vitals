import { describe, expect, it } from 'vitest';

import { foldTrend, powerLevel, powerScore } from './power';

const idle = { cpu: 0, gpu: null, disk: 0, network: null };

describe('power usage', () => {
  it('ranks like Task Manager on the readings it was calibrated against', () => {
    expect(powerLevel(powerScore(idle))).toBe('veryLow');
    // Defender scanning at 11 % CPU.
    expect(powerLevel(powerScore({ ...idle, cpu: 11 }))).toBe('veryHigh');
    // The search indexer at 2 % CPU.
    expect(powerLevel(powerScore({ ...idle, cpu: 2 }))).toBe('high');
    // Node at 1 % CPU and 20 MB/s of disk.
    expect(powerLevel(powerScore({ ...idle, cpu: 1, disk: 20 * 1024 * 1024 }))).toBe('moderate');
  });

  it('counts GPU work, so a game at 0 % CPU is not "very low"', () => {
    expect(powerLevel(powerScore({ ...idle, gpu: 40 }))).toBe('veryHigh');
  });

  it('treats an unmeasured GPU or network as contributing nothing, not as an error', () => {
    expect(Number.isFinite(powerScore(idle))).toBe(true);
  });
});

describe('power trend', () => {
  it('starts a new process at its current score rather than at zero', () => {
    const next = foldTrend(new Map(), new Map([['a', 4]]), 1000);
    expect(next.get('a')).toBe(4);
  });

  it('moves slowly, so a one-second spike does not read as a drain', () => {
    const next = foldTrend(new Map([['a', 0]]), new Map([['a', 10]]), 1000);
    expect(next.get('a')).toBeGreaterThan(0);
    expect(next.get('a')).toBeLessThan(0.3);
  });

  it('forgets processes that have exited', () => {
    const next = foldTrend(new Map([['gone', 3]]), new Map([['a', 1]]), 1000);
    expect(next.has('gone')).toBe(false);
  });
});
