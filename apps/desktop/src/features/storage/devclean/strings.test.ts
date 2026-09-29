import { describe, expect, it } from 'vitest';

import {
  artefactKinds,
  cacheKinds,
  devOutcomes,
  devScanPhases,
  devSections,
  dockerItemKinds,
  worktreeStates,
} from './model';
import { devBundles } from './strings';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

// Romanian needs `_few`; English does not. Parity is on the base key.
const strip = (paths: readonly string[]) =>
  [...new Set(paths.map((path) => path.replace(/_(one|few|many|other)$/, '')))].sort();

describe('developer clean-up translations', () => {
  it('defines the same keys in English and Romanian', () => {
    expect(strip(keyPaths(devBundles.ro))).toEqual(strip(keyPaths(devBundles.en)));
  });

  it('labels every value the backend can send, in both languages', () => {
    const groups: readonly (readonly [string, readonly string[]])[] = [
      ['artefact', artefactKinds],
      ['worktreeState', worktreeStates],
      ['cache', cacheKinds],
      ['docker', dockerItemKinds],
      ['phase', devScanPhases],
      ['outcome', devOutcomes],
      ['section', devSections],
    ];
    for (const locale of ['en', 'ro'] as const) {
      const paths = new Set(strip(keyPaths(devBundles[locale])));
      for (const [group, values] of groups) {
        for (const value of values) {
          expect(paths, `${locale} missing ${group}.${value}`).toContain(`${group}.${value}`);
        }
      }
    }
  });

  it('explains every worktree state that is kept or offered', () => {
    const paths = new Set(strip(keyPaths(devBundles.en)));
    for (const state of worktreeStates) {
      expect(paths, `missing worktree.${state}`).toContain(`worktree.${state}`);
    }
  });

  it('gives Romanian every plural form its counts need', () => {
    const ro = new Set(keyPaths(devBundles.ro));
    for (const base of [
      'scan.found',
      'project.idle',
      'worktree.dirty',
      'worktree.active',
      'dockerCount',
      'summary',
    ]) {
      for (const form of ['one', 'few', 'other']) {
        expect(ro, `ro missing ${base}_${form}`).toContain(`${base}_${form}`);
      }
    }
  });
});
