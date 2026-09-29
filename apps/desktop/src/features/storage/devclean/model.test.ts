import { describe, expect, it } from 'vitest';

import { GB, artefact, devScan, project } from './fixtures.test-utils';
import {
  isStale,
  preselectedIds,
  projectCheckState,
  sectionIds,
  selectableLabelKey,
  selectables,
  selectionTotal,
  sortProjects,
  stopsWsl,
  withoutIds,
} from './model';

const label = (kind: string, path: string) => `${kind}|${path}`;

describe('developer clean-up model', () => {
  it('pre-ticks only the artefacts the backend marked, which are those of stale projects', () => {
    expect([...preselectedIds(devScan())].sort()).toEqual(['a-stale-nm', 'a-stale-target']);
  });

  it('counts an unmeasured selection without adding it to the total as zero', () => {
    const items = selectables(devScan(), label);
    const total = selectionTotal(items, new Set(['a-stale-nm', 'a-stale-target', 'nope']));
    expect(total).toEqual({ bytes: 4 * GB, unmeasured: 1, count: 2 });
  });

  it('sorts projects largest first and breaks ties by path', () => {
    const sorted = sortProjects([
      project({ name: 'b', artefacts: [artefact({ id: 'b1', size: GB })] }),
      project({ name: 'big', artefacts: [artefact({ id: 'x', size: 5 * GB })] }),
      project({ name: 'a', artefacts: [artefact({ id: 'a1', size: GB })] }),
    ]);
    expect(sorted.map((p) => p.name)).toEqual(['big', 'a', 'b']);
  });

  it('offers worktrees, caches and docker items only when they carry an id', () => {
    const ids = selectables(devScan(), label).map((s) => s.id);
    expect(ids).toEqual([
      'a-stale-nm',
      'a-stale-target',
      'a-active-next',
      'w-removable',
      'c-pnpm',
      'd-build',
      'v-docker',
    ]);
  });

  it('marks folder deletions as permanent and a disk compaction as not', () => {
    const byId = new Map(selectables(devScan(), label).map((s) => [s.id, s]));
    expect(byId.get('a-stale-nm')?.permanent).toBe(true);
    expect(byId.get('w-removable')?.permanent).toBe(true);
    expect(byId.get('c-pnpm')?.permanent).toBe(false);
    expect(byId.get('v-docker')?.permanent).toBe(false);
  });

  it('says WSL stops only when a virtual disk is in the selection', () => {
    const scan = devScan();
    expect(stopsWsl(scan, new Set(['a-stale-nm', 'c-pnpm']))).toBe(false);
    expect(stopsWsl(scan, new Set(['v-docker']))).toBe(true);
  });

  it('does not call a project of unknown age stale', () => {
    expect(isStale(project({ name: 'x', idleDays: null }))).toBe(false);
    expect(isStale(project({ name: 'x', idleDays: 29 }))).toBe(false);
    expect(isStale(project({ name: 'x', idleDays: 30 }))).toBe(true);
  });

  it('maps every label kind to a key inside the dev bundle', () => {
    expect(selectableLabelKey('nodeModules')).toBe('dev.artefact.nodeModules');
    expect(selectableLabelKey('worktree:prunable')).toBe('dev.worktreeState.prunable');
    expect(selectableLabelKey('cache:pnpm')).toBe('dev.cache.pnpm');
    expect(selectableLabelKey('docker:buildCache')).toBe('dev.docker.buildCache');
    expect(selectableLabelKey('vdisk:wsl')).toBe('dev.vdisk.wsl');
  });

  it('reports a project as mixed when only some of its folders are ticked', () => {
    const memorai = devScan().projects[0];
    if (memorai === undefined) throw new Error('fixture has no projects');
    expect(projectCheckState(memorai, new Set())).toBe(false);
    expect(projectCheckState(memorai, new Set(['a-stale-nm']))).toBe('indeterminate');
    expect(projectCheckState(memorai, new Set(['a-stale-nm', 'a-stale-target']))).toBe(true);
  });

  it('lists per-section ids without the rows that cannot be selected', () => {
    const ids = sectionIds(devScan());
    expect(ids.worktrees).toEqual(['w-removable']);
    expect(ids.caches).toEqual(['c-pnpm']);
    expect(ids.docker).toEqual(['d-build']);
  });

  it('drops removed items and the projects they emptied, keeping the rest', () => {
    const next = withoutIds(devScan(), new Set(['a-active-next', 'c-pnpm', 'v-docker']));
    expect(next.projects.map((p) => p.name)).toEqual(['memorai']);
    expect(next.caches.map((c) => c.kind)).toEqual(['go']);
    expect(next.vdisks).toEqual([]);
    expect(next.worktrees).toHaveLength(3);
  });
});
