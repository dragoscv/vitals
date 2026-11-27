import type { MemoryMetrics } from '@vitals/protocol';
import { describe, expect, it } from 'vitest';

import { DASH, bytes } from './format';
import { memoryUsage } from './memory';

function memory(used: number, total: number): MemoryMetrics {
  return { used, total } as MemoryMetrics;
}

describe('memoryUsage', () => {
  it('returns the reading when the PC measured memory', () => {
    expect(memoryUsage(memory(4, 16))).toEqual({ used: 4, total: 16 });
  });

  it('returns null when memory was not reported, so the line prints a dash and not a zero', () => {
    const usage = memoryUsage(memory(0, 0));
    expect(usage).toBeNull();
    // The guard turns the absent reading into a dash ...
    expect(bytes(usage?.used ?? null, 'en')).toBe(DASH);
    expect(bytes(usage?.total ?? null, 'en')).toBe(DASH);
    // ... while a genuine zero byte count still reads as zero, not a dash.
    expect(bytes(0, 'en')).not.toBe(DASH);
  });
});
