import { describe, expect, it } from 'vitest';

import { ProcessFlag } from './constants';
import { buildRows, collectSubtree, flagKeys, matchesKind, matchesQuery, sortValue } from './model';
import { makeMap, makeProcess } from './test-fixtures';

function build(options: Parameters<typeof buildRows>[0]) {
  return buildRows(options);
}

describe('filtering', () => {
  it('matches on name, PID and user', () => {
    const p = makeProcess({ pid: 4242, name: 'chrome.exe', user: 'ALICE' });
    expect(matchesQuery(p, 'chro')).toBe(true);
    expect(matchesQuery(p, '4242')).toBe(true);
    expect(matchesQuery(p, 'alice')).toBe(true);
    expect(matchesQuery(p, 'firefox')).toBe(false);
  });

  it('groups services and containers under "background"', () => {
    expect(matchesKind('service', 'background')).toBe(true);
    expect(matchesKind('containerized', 'background')).toBe(true);
    expect(matchesKind('app', 'background')).toBe(false);
    expect(matchesKind('system', 'system')).toBe(true);
  });

  it('keeps a non-matching parent so a matching child stays reachable', () => {
    // Searching "renderer" must not return nothing merely because the
    // renderers hang off a launcher with a different name.
    const parent = makeProcess({ pid: 1, name: 'launcher.exe' });
    const child = makeProcess({ pid: 2, name: 'renderer.exe', parent: 1 });
    const result = build({
      processes: makeMap([parent, child]),
      query: 'renderer',
      kind: 'all',
      grouped: true,
      expanded: new Set(['1:1000']),
    });
    expect(result.rows.map((r) => r.process.name)).toEqual(['launcher.exe', 'renderer.exe']);
  });

  it('drops a subtree with no match at all', () => {
    const parent = makeProcess({ pid: 1, name: 'launcher.exe' });
    const child = makeProcess({ pid: 2, name: 'renderer.exe', parent: 1 });
    const result = build({
      processes: makeMap([parent, child]),
      query: 'nothing-matches',
      kind: 'all',
      grouped: true,
      expanded: new Set(),
    });
    expect(result.rows).toHaveLength(0);
  });
});

describe('tree building', () => {
  it('hides collapsed children but still counts them', () => {
    const parent = makeProcess({ pid: 1, name: 'app.exe', cpu: 1 });
    const child = makeProcess({ pid: 2, name: 'worker.exe', parent: 1, cpu: 40 });
    const result = build({
      processes: makeMap([parent, child]),
      query: '',
      kind: 'all',
      grouped: true,
      expanded: new Set(),
    });
    expect(result.rows).toHaveLength(1);
    expect(result.rows[0]?.descendantCount).toBe(1);
  });

  it('rolls a hidden child’s CPU into the collapsed parent', () => {
    // A parent reading 1% while concealing a child at 40% would tell the user
    // the app is idle when it is the reason the machine is hot.
    const parent = makeProcess({ pid: 1, cpu: 1 });
    const child = makeProcess({ pid: 2, parent: 1, cpu: 40 });
    const result = build({
      processes: makeMap([parent, child]),
      query: '',
      kind: 'all',
      grouped: true,
      expanded: new Set(),
    });
    expect(result.rows[0]?.rolledCpu).toBe(41);
  });

  it('leaves rolled GPU null when nothing in the subtree reports it', () => {
    const parent = makeProcess({ pid: 1, gpu: null });
    const child = makeProcess({ pid: 2, parent: 1, gpu: null });
    const result = build({
      processes: makeMap([parent, child]),
      query: '',
      kind: 'all',
      grouped: true,
      expanded: new Set(),
    });
    // Never zero: "we did not measure the GPU" and "the GPU is idle" are
    // different claims and only one of them is true here.
    expect(result.rows[0]?.rolledGpu).toBeNull();
  });

  it('survives a parent cycle produced by PID recycling', () => {
    const a = makeProcess({ pid: 1, parent: 2 });
    const b = makeProcess({ pid: 2, parent: 1 });
    const result = build({
      processes: makeMap([a, b]),
      query: '',
      kind: 'all',
      grouped: true,
      expanded: new Set(['1:1000', '2:2000']),
    });
    expect(result.rows.length).toBeGreaterThan(0);
  });

  it('produces a flat list when grouping is off', () => {
    const parent = makeProcess({ pid: 1 });
    const child = makeProcess({ pid: 2, parent: 1 });
    const result = build({
      processes: makeMap([parent, child]),
      query: '',
      kind: 'all',
      grouped: false,
      expanded: new Set(),
    });
    expect(result.rows).toHaveLength(2);
    expect(result.rows.every((row) => row.depth === 0)).toBe(true);
  });

  it('collects a whole subtree for a tree kill', () => {
    const root = makeProcess({ pid: 1 });
    const mid = makeProcess({ pid: 2, parent: 1 });
    const leaf = makeProcess({ pid: 3, parent: 2 });
    const result = build({
      processes: makeMap([root, mid, leaf]),
      query: '',
      kind: 'all',
      grouped: true,
      expanded: new Set(['1:1000', '2:2000']),
    });
    expect(collectSubtree(result.byId, '1:1000')).toHaveLength(3);
  });
});

describe('sort values', () => {
  it('reports NaN, not zero, for a handle count we could not read', () => {
    const result = build({
      processes: makeMap([makeProcess({ pid: 1, handleCount: null })]),
      query: '',
      kind: 'all',
      grouped: false,
      expanded: new Set(),
    });
    const row = result.rows[0];
    expect(row).toBeDefined();
    expect(Number.isNaN(sortValue(row as never, 'handles'))).toBe(true);
  });
});

describe('flags', () => {
  it('decodes the bitfield', () => {
    expect(flagKeys(ProcessFlag.Critical | ProcessFlag.Elevated)).toEqual(['elevated', 'critical']);
    expect(flagKeys(0)).toEqual([]);
  });
});
