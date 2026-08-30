import { describe, expect, it } from 'vitest';

import {
  computeTotals,
  filterHistory,
  formatCpuTime,
  sortHistory,
  type AppHistoryRecord,
} from './model';

const stub = (overrides: Partial<AppHistoryRecord> = {}): AppHistoryRecord => ({
  executable: 'C:\\test.exe',
  name: 'test.exe',
  cpuSeconds: 0,
  diskReadBytes: 0,
  diskWriteBytes: 0,
  peakPrivateBytes: 0,
  firstSeen: Date.parse('2026-01-01T00:00:00Z'),
  lastSeen: Date.parse('2026-01-01T00:00:00Z'),
  sessions: 1,
  ...overrides,
});

describe('sortHistory', () => {
  it('sorts by CPU descending', () => {
    const records = [
      stub({ name: 'a', cpuSeconds: 10 }),
      stub({ name: 'b', cpuSeconds: 50 }),
      stub({ name: 'c', cpuSeconds: 30 }),
    ];

    const sorted = sortHistory(records, 'cpu', 'en');
    expect(sorted.map((r) => r.name)).toEqual(['b', 'c', 'a']);
  });

  it('sorts by total disk I/O descending', () => {
    const records = [
      stub({ name: 'a', diskReadBytes: 100, diskWriteBytes: 200 }), // 300
      stub({ name: 'b', diskReadBytes: 500, diskWriteBytes: 100 }), // 600
      stub({ name: 'c', diskReadBytes: 50, diskWriteBytes: 50 }), // 100
    ];

    const sorted = sortHistory(records, 'disk', 'en');
    expect(sorted.map((r) => r.name)).toEqual(['b', 'a', 'c']);
  });

  it('sorts by peak memory descending', () => {
    const records = [
      stub({ name: 'a', peakPrivateBytes: 1024 }),
      stub({ name: 'b', peakPrivateBytes: 4096 }),
      stub({ name: 'c', peakPrivateBytes: 2048 }),
    ];

    const sorted = sortHistory(records, 'memory', 'en');
    expect(sorted.map((r) => r.name)).toEqual(['b', 'c', 'a']);
  });

  it('sorts by last seen descending', () => {
    const records = [
      stub({ name: 'a', lastSeen: Date.parse('2026-01-01T10:00:00Z') }),
      stub({ name: 'b', lastSeen: Date.parse('2026-01-03T10:00:00Z') }),
      stub({ name: 'c', lastSeen: Date.parse('2026-01-02T10:00:00Z') }),
    ];

    const sorted = sortHistory(records, 'lastSeen', 'en');
    expect(sorted.map((r) => r.name)).toEqual(['b', 'c', 'a']);
  });

  it('falls back to name when values match', () => {
    const records = [
      stub({ name: 'zebra', cpuSeconds: 100 }),
      stub({ name: 'apple', cpuSeconds: 100 }),
    ];

    const sorted = sortHistory(records, 'cpu', 'en');
    expect(sorted.map((r) => r.name)).toEqual(['apple', 'zebra']);
  });
});

describe('filterHistory', () => {
  it('returns all records for empty query', () => {
    const records = [stub({ name: 'a' }), stub({ name: 'b' })];
    expect(filterHistory(records, '')).toEqual(records);
    expect(filterHistory(records, '   ')).toEqual(records);
  });

  it('matches by name, case-insensitive', () => {
    const records = [
      stub({ name: 'chrome.exe' }),
      stub({ name: 'firefox.exe' }),
      stub({ name: 'edge.exe' }),
    ];

    expect(filterHistory(records, 'chrome').map((r) => r.name)).toEqual(['chrome.exe']);
    expect(filterHistory(records, 'FIRE').map((r) => r.name)).toEqual(['firefox.exe']);
  });

  it('matches by executable path', () => {
    const records = [
      stub({ name: 'app.exe', executable: 'C:\\ProgramFiles\\App\\app.exe' }),
      stub({ name: 'tool.exe', executable: 'C:\\Windows\\System32\\tool.exe' }),
    ];

    expect(filterHistory(records, 'ProgramFiles').map((r) => r.name)).toEqual(['app.exe']);
    expect(filterHistory(records, 'system32').map((r) => r.name)).toEqual(['tool.exe']);
  });

  it('returns empty for no matches', () => {
    const records = [stub({ name: 'app.exe' })];
    expect(filterHistory(records, 'nonexistent')).toEqual([]);
  });
});

describe('computeTotals', () => {
  it('sums across all records', () => {
    const records = [
      stub({ cpuSeconds: 10, diskReadBytes: 100, diskWriteBytes: 200 }),
      stub({ cpuSeconds: 20, diskReadBytes: 50, diskWriteBytes: 150 }),
    ];

    const totals = computeTotals(records);
    expect(totals.totalCpuSeconds).toBe(30);
    expect(totals.totalDiskReadBytes).toBe(150);
    expect(totals.totalDiskWriteBytes).toBe(350);
  });

  it('returns zero for empty list', () => {
    const totals = computeTotals([]);
    expect(totals.totalCpuSeconds).toBe(0);
    expect(totals.totalDiskReadBytes).toBe(0);
    expect(totals.totalDiskWriteBytes).toBe(0);
  });
});

describe('formatCpuTime', () => {
  it('formats sub-second values with decimal', () => {
    expect(formatCpuTime(0.5, 'en')).toBe('0.5 s');
    expect(formatCpuTime(0.123, 'en')).toBe('0.1 s');
  });

  it('formats seconds only', () => {
    expect(formatCpuTime(5, 'en')).toBe('5 s');
    expect(formatCpuTime(59, 'en')).toBe('59 s');
  });

  it('formats minutes and seconds', () => {
    expect(formatCpuTime(60, 'en')).toBe('1 min');
    expect(formatCpuTime(65, 'en')).toBe('1 min 5 s');
    expect(formatCpuTime(185, 'en')).toBe('3 min 5 s');
  });

  it('formats hours and minutes', () => {
    expect(formatCpuTime(3600, 'en')).toBe('1h');
    expect(formatCpuTime(3660, 'en')).toBe('1h 1 min');
    expect(formatCpuTime(7800, 'en')).toBe('2h 10 min');
  });

  it('formats large hour values', () => {
    expect(formatCpuTime(172800, 'en')).toBe('48h');
    expect(formatCpuTime(174600, 'en')).toBe('48h 30 min');
  });
});
