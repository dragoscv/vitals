/**
 * The guided move's vocabulary, checked against the Rust that sends it, and
 * every value named in both languages. The two sides compile separately: a
 * refusal added in `drive_move.rs` would otherwise render as an empty reason.
 */
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { describe, expect, it } from 'vitest';

import {
  diskOwners,
  isLowOnSpace,
  moveRefusals,
  moveStages,
  moveStatuses,
  moveUnavailables,
} from './model';
import { moveBundles } from './strings';

const here = dirname(fileURLToPath(import.meta.url));
const rust = (() => {
  const text = readFileSync(
    resolve(here, '../../../../../../../crates/vitals-win/src/storage/drive_move.rs'),
    'utf8',
  );
  const cut = text.indexOf('#[cfg(test)]');
  return cut === -1 ? text : text.slice(0, cut);
})();

/** The `=> "x"` arms of the `impl <name>` block's `key`. */
function keys(name: string): string[] {
  const at = rust.indexOf(`impl ${name} {`);
  if (at === -1) throw new Error(`impl ${name} not found; the Rust moved, update this test`);
  const end = rust.indexOf('\n}\n', at);
  return [...rust.slice(at, end).matchAll(/=> "(\w+)"/g)].map((m) => m[1] ?? '').sort();
}

const sorted = (list: readonly string[]) => [...list].sort();

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('guided move contract with the Rust backend', () => {
  it('knows exactly the refusals, unavailable reasons, stages, statuses and owners', () => {
    expect(sorted(moveRefusals)).toEqual(keys('TargetRefusal'));
    expect(sorted(moveUnavailables)).toEqual(keys('MoveUnavailable'));
    expect(sorted(moveStages)).toEqual(keys('MoveStage'));
    expect(sorted(moveStatuses)).toEqual(keys('MoveStatus'));
    expect(sorted(diskOwners)).toEqual(keys('DiskOwner'));
  });

  it('defines the same keys in English and Romanian, and names every value', () => {
    const en = keyPaths(moveBundles.en).sort();
    expect(keyPaths(moveBundles.ro).sort()).toEqual(en);
    const expected = [
      ...moveRefusals.map((r) => `target.refusal.${r}`),
      ...moveUnavailables.map((u) => `disk.unavailable.${u}`),
      ...moveStages.map((s) => `run.stage.${s}`),
      ...moveStatuses.map((s) => `report.status.${s}`),
    ];
    for (const key of expected) expect(en, key).toContain(key);
  });

  it('writes Romanian with comma-below ș and ț, never the cedilla forms', () => {
    expect(JSON.stringify(moveBundles.ro)).not.toMatch(/[şţŞŢ]/);
  });
});

describe('when a drive counts as low on space', () => {
  const GB = 1024 ** 3;
  it('is under 15 % free or under 25 GB free, and an unread size is never low', () => {
    expect(isLowOnSpace({ freeBytes: 140 * GB, totalBytes: 1000 * GB })).toBe(true);
    expect(isLowOnSpace({ freeBytes: 200 * GB, totalBytes: 1000 * GB })).toBe(false);
    expect(isLowOnSpace({ freeBytes: 20 * GB, totalBytes: 100 * GB })).toBe(true);
    expect(isLowOnSpace({ freeBytes: 300 * GB, totalBytes: 4000 * GB })).toBe(true);
    expect(isLowOnSpace({ freeBytes: 0, totalBytes: 0 })).toBe(false);
  });
});
