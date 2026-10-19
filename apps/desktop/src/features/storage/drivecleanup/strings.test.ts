import { describe, expect, it } from 'vitest';

import {
  analyseStages,
  blockerKinds,
  driveCleanStages,
  driveItemStatuses,
  driveRoles,
  planGroupKeys,
  planItemKinds,
  roleReasonKeys,
  unavailableReasons,
} from './model';
import { driveBundles } from './strings';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

const strip = (paths: readonly string[]) =>
  [...new Set(paths.map((path) => path.replace(/_(one|few|many|other)$/, '')))].sort();

describe('drive clean-up translations', () => {
  it('defines the same keys in English and Romanian', () => {
    expect(strip(keyPaths(driveBundles.ro))).toEqual(strip(keyPaths(driveBundles.en)));
  });

  it('names every value the backend can send, in both languages', () => {
    const expected = [
      ...planItemKinds.flatMap((kind) => [`item.${kind}.name`, `item.${kind}.what`]),
      ...planGroupKeys.map((key) => `group.${key}`),
      ...analyseStages.map((stage) => `analyse.stage.${stage}`),
      ...driveRoles.map((role) => `role.${role}`),
      ...roleReasonKeys.map((key) => `role.reason.${key}`),
      ...unavailableReasons.map((reason) => `unavailableReason.${reason}`),
      ...blockerKinds.flatMap((kind) => [`blockers.kind.${kind}`, `blockers.advice.${kind}`]),
      'blockers.unknownHyperV',
      'blockers.unknownHyperVAdvice',
      ...driveItemStatuses.map((status) => `report.status.${status}`),
      ...driveCleanStages.filter((s) => s !== 'compacting').map((s) => `run.stage.${s}`),
      'run.stage.compacting',
    ];
    for (const locale of ['en', 'ro'] as const) {
      const paths = new Set(strip(keyPaths(driveBundles[locale])));
      for (const key of expected) expect(paths, `${locale} missing ${key}`).toContain(key);
    }
  });

  it('gives Romanian every plural form its counts need', () => {
    const ro = new Set(keyPaths(driveBundles.ro));
    for (const base of ['selection', 'usual.body']) {
      for (const form of ['one', 'few', 'other']) {
        expect(ro, `ro missing ${base}_${form}`).toContain(`${base}_${form}`);
      }
    }
  });

  it('writes Romanian with comma-below ș and ț, never the cedilla forms', () => {
    const text = JSON.stringify(driveBundles.ro);
    expect(text).not.toMatch(/[şţŞŢ]/);
    expect(text).toMatch(/[șț]/);
  });
});
