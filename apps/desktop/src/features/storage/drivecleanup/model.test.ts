import { describe, expect, it } from 'vitest';

import {
  blockersFor,
  groupCheckState,
  groupSelectable,
  initialSelection,
  isCleanableKind,
  mountKey,
  profileKinds,
  sizeTotal,
  type DriveCleanupPlan,
  type PlanItem,
} from './model';

function item(id: string, overrides: Partial<PlanItem> = {}): PlanItem {
  return {
    id,
    kind: 'tempOld',
    label: null,
    size: 1,
    safety: 'safe',
    defaultSelected: false,
    irreversible: false,
    needsElevation: false,
    stopsWsl: false,
    stopsDocker: false,
    detail: null,
    ...overrides,
  };
}

const plan: DriveCleanupPlan = {
  scanId: 1,
  mount: 'C:\\',
  role: 'system',
  roleReasons: [],
  freeBytes: 0,
  totalBytes: 0,
  groups: [
    {
      key: 'docker',
      items: [
        item('a', { kind: 'dockerOldImages', defaultSelected: true, stopsDocker: true }),
        item('b', { kind: 'dockerNamedVolume', defaultSelected: true, safety: 'danger' }),
        item('c', { kind: 'compactVhd', size: null, stopsWsl: true }),
      ],
    },
  ],
  blockers: [
    { kind: 'wslDistro', name: 'Ubuntu', restartAfter: true },
    { kind: 'container', name: 'pg', restartAfter: true },
  ],
};

const items = plan.groups[0]?.items ?? [];

describe('drive clean-up selection rules', () => {
  it('never preselects a danger item, with or without a profile', () => {
    expect([...initialSelection(plan, undefined)]).toEqual(['a']);
    expect([...initialSelection(plan, ['dockerNamedVolume', 'compactVhd'])]).toEqual(['c']);
  });

  it('select-all reaches everything but danger, and the header reflects it', () => {
    const group = plan.groups[0];
    if (group === undefined) throw new Error('fixture');
    expect(groupSelectable(group)).toEqual(['a', 'c']);
    expect(groupCheckState(group, new Set(['a']))).toBe('indeterminate');
    expect(groupCheckState(group, new Set(['a', 'c', 'b']))).toBe(true);
  });

  it('counts unmeasured items apart rather than as zero', () => {
    expect(sizeTotal(items)).toEqual({ count: 3, bytes: 2, unmeasured: 1 });
  });

  it('stopping WSL stops everything; a Docker step stops only Docker', () => {
    expect(blockersFor(plan, [])).toEqual([]);
    expect(blockersFor(plan, [items[0] as PlanItem]).map((b) => b.kind)).toEqual(['container']);
    expect(blockersFor(plan, [items[2] as PlanItem])).toHaveLength(2);
  });

  it('remembers kinds, never a danger kind', () => {
    expect(profileKinds(items)).toEqual(['dockerOldImages', 'compactVhd']);
  });

  it('never preselects or select-alls an item that cannot be done here', () => {
    const blocked: DriveCleanupPlan = {
      ...plan,
      groups: [
        {
          key: 'windows',
          items: [item('w', { kind: 'componentStore', defaultSelected: true, available: false })],
        },
      ],
    };
    expect([...initialSelection(blocked, undefined)]).toEqual([]);
    expect(groupSelectable(blocked.groups[0] as (typeof blocked.groups)[number])).toEqual([]);
  });

  it('treats C:, c:\\ and C:\\ as one drive, and skips network and optical', () => {
    expect(mountKey('c:\\')).toBe(mountKey('C:'));
    expect(isCleanableKind('network')).toBe(false);
    expect(isCleanableKind('optical')).toBe(false);
    expect(isCleanableKind('nvme')).toBe(true);
  });
});
