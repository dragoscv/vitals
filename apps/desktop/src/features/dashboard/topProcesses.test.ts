import { describe, expect, it } from 'vitest';

import { groupByName, topBy, topByCpu, topByMemory, TOP_COUNT } from './topProcesses';
import { makeProcess, makeProcessMap } from './test-fixtures';

describe('groupByName', () => {
  it('sums every process sharing an executable name', () => {
    // The reason this widget aggregates at all: a raw top-five on a machine
    // running a browser is five rows called chrome.exe, which answers nothing.
    const processes = makeProcessMap([
      makeProcess({ name: 'chrome.exe', cpu: 8 }),
      makeProcess({ name: 'chrome.exe', cpu: 12 }),
      makeProcess({ name: 'code.exe', cpu: 5 }),
    ]);

    const groups = groupByName(processes, (process) => process.cpu);
    expect(groups.get('chrome.exe')).toMatchObject({ value: 20, count: 2 });
    expect(groups.get('code.exe')).toMatchObject({ value: 5, count: 1 });
  });

  it('remembers the largest contributor, not the last one seen', () => {
    // Selecting a group is meaningless to the Processes screen, which acts on
    // one race-free key. The dominant PID is what a user clicking through means.
    const processes = makeProcessMap([
      makeProcess({ key: { pid: 1, startTime: 1 }, name: 'chrome.exe', cpu: 30 }),
      makeProcess({ key: { pid: 2, startTime: 1 }, name: 'chrome.exe', cpu: 2 }),
    ]);

    expect(groupByName(processes, (process) => process.cpu).get('chrome.exe')?.bestKey).toBe('1:1');
  });

  it('skips processes reporting zero', () => {
    // Otherwise the map is proportional to the process count rather than to
    // the number of active applications, on every tick.
    const processes = makeProcessMap([
      makeProcess({ name: 'idle.exe', cpu: 0 }),
      makeProcess({ name: 'busy.exe', cpu: 4 }),
    ]);

    const groups = groupByName(processes, (process) => process.cpu);
    expect(groups.has('idle.exe')).toBe(false);
    expect(groups.size).toBe(1);
  });
});

describe('topBy', () => {
  function groups(entries: readonly (readonly [string, number])[]) {
    return new Map(
      entries.map(([name, value]) => [name, { value, count: 1, bestKey: `${name}:1` }]),
    );
  }

  it('returns the largest entries in descending order', () => {
    const result = topBy(
      groups([
        ['a', 1],
        ['b', 9],
        ['c', 5],
      ]),
      2,
    );
    expect(result.map((entry) => entry.name)).toEqual(['b', 'c']);
  });

  it('defaults to five', () => {
    const many = groups(Array.from({ length: 20 }, (_, index) => [`p${index}`, index] as const));
    expect(topBy(many)).toHaveLength(TOP_COUNT);
  });

  it('returns everything when there are fewer than the limit', () => {
    expect(topBy(groups([['a', 1]]))).toHaveLength(1);
  });

  it('handles an empty input', () => {
    expect(topBy(new Map())).toEqual([]);
  });

  it('breaks ties deterministically', () => {
    // Without this, two processes at the same value swap places every tick
    // from map iteration order alone — the same flicker the Processes screen's
    // ordering policy exists to prevent, arriving through a different door.
    const first = topBy(
      groups([
        ['zeta', 5],
        ['alpha', 5],
        ['mid', 5],
      ]),
      3,
    );
    const second = topBy(
      groups([
        ['mid', 5],
        ['zeta', 5],
        ['alpha', 5],
      ]),
      3,
    );

    expect(first.map((entry) => entry.name)).toEqual(['alpha', 'mid', 'zeta']);
    expect(second.map((entry) => entry.name)).toEqual(first.map((entry) => entry.name));
  });

  it('agrees with a full sort on a large input', () => {
    // The partial-selection implementation is the whole point of the module;
    // this pins it against the obvious-but-slower version it replaced.
    const values = Array.from(
      { length: 500 },
      (_, index) => [`p${index}`, (index * 7919) % 1000] as const,
    );

    const expected = [...values]
      .sort((a, b) => (b[1] !== a[1] ? b[1] - a[1] : a[0].localeCompare(b[0])))
      .slice(0, TOP_COUNT)
      .map(([name]) => name);

    expect(topBy(groups(values)).map((entry) => entry.name)).toEqual(expected);
  });
});

describe('topByCpu / topByMemory', () => {
  it('ranks by summed CPU across a process group', () => {
    const processes = makeProcessMap([
      makeProcess({ name: 'chrome.exe', cpu: 6 }),
      makeProcess({ name: 'chrome.exe', cpu: 6 }),
      makeProcess({ name: 'game.exe', cpu: 11 }),
    ]);

    const top = topByCpu(processes);
    expect(top[0]).toMatchObject({ name: 'chrome.exe', value: 12, count: 2 });
    expect(top[1]?.name).toBe('game.exe');
  });

  it('uses private bytes, not working set', () => {
    // Working set double-counts shared pages across every process that maps
    // them; private bytes is the honest "how much would I get back" number.
    const processes = makeProcessMap([
      makeProcess({ name: 'a.exe', memoryPrivate: 100, memoryWorkingSet: 9000 }),
      makeProcess({ name: 'b.exe', memoryPrivate: 500, memoryWorkingSet: 600 }),
    ]);

    expect(topByMemory(processes)[0]?.name).toBe('b.exe');
  });

  it('returns nothing when the machine is idle', () => {
    const processes = makeProcessMap([makeProcess({ cpu: 0, memoryPrivate: 0 })]);
    expect(topByCpu(processes)).toEqual([]);
    expect(topByMemory(processes)).toEqual([]);
  });
});
