/**
 * The vocabulary the wizard knows, checked against the Rust that sends it.
 *
 * Every kind, stage, group, blocker, role, reason and status the backend can
 * emit is a string literal in `crates/vitals-win/src/storage`. The webview
 * keeps its own lists (and a string per value, checked by strings.test.ts),
 * and the two compile separately: a kind added in Rust would otherwise render
 * as "Other item" with nobody told. This reads the Rust source and fails on
 * any difference in either direction.
 */
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

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
import { historyOnlyKinds } from './history';
import { schedulableKinds } from './schedule';

const here = dirname(fileURLToPath(import.meta.url));
const storage = resolve(here, '../../../../../../crates/vitals-win/src/storage');

/** The file without its `#[cfg(test)]` module: fixtures are not the contract. */
function source(file: string): string {
  const text = readFileSync(resolve(storage, file), 'utf8');
  const cut = text.indexOf('#[cfg(test)]');
  return cut === -1 ? text : text.slice(0, cut);
}

/** The `{ … }` body that follows the first occurrence of `marker`. */
function block(text: string, marker: string): string {
  const at = text.indexOf(marker);
  if (at === -1) throw new Error(`${marker} not found; the Rust moved, update this test`);
  const open = text.indexOf('{', at);
  let depth = 0;
  for (let i = open; i < text.length; i += 1) {
    if (text[i] === '{') depth += 1;
    else if (text[i] === '}') {
      depth -= 1;
      if (depth === 0) return text.slice(open, i + 1);
    }
  }
  throw new Error(`unbalanced braces after ${marker}`);
}

function all(text: string, pattern: RegExp): string[] {
  return [...text.matchAll(pattern)].map((m) => m[1] ?? '');
}

const sorted = (values: Iterable<string>) => [...new Set(values)].sort();

const mod = source('drive_cleanup/mod.rs');
const run = source('drive_cleanup/run.rs');
const role = source('drive_role.rs');
const schedule = source('drive_cleanup/schedule.rs');
const history = source('drive_cleanup/history.rs');

describe('drive clean-up contract with the Rust backend', () => {
  it('knows exactly the item kinds the analysis can emit', () => {
    const emitted = [
      ...all(mod, /PlannedItem::new\(\s*Group::\w+,\s*"(\w+)"/g),
      ...all(mod, /\(Handler::\w+,\s*"(\w+)"\)/g),
      ...all(mod, /FolderKind::\w+,\s*"(\w+)"\)/g),
      ...all(block(mod, 'const fn kind(self)'), /"(\w+)"/g),
      ...all(block(mod, 'let kind = match f.kind'), /"(\w+)"/g),
    ];
    expect(sorted(planItemKinds)).toEqual(sorted(emitted));
  });

  it('knows exactly the groups, analysis stages and blocker kinds', () => {
    expect(sorted(planGroupKeys)).toEqual(sorted(all(block(mod, 'impl Group'), /=> "(\w+)"/g)));
    expect([...analyseStages]).toEqual(all(block(mod, 'pub const STAGES'), /"(\w+)"/g));
    expect(sorted(blockerKinds)).toEqual(
      sorted(all(block(mod, 'impl BlockerKind'), /=> "(\w+)"/g)),
    );
  });

  it('knows exactly the run stages and result statuses', () => {
    const stages = [
      ...all(run, /stage: "(\w+)"/g),
      ...all(block(run, 'stage: match line.stage.as_str()'), /=> "(\w+)"/g),
    ];
    expect(sorted(driveCleanStages)).toEqual(sorted(stages));
    expect(sorted(driveItemStatuses)).toEqual(
      sorted(all(block(run, 'pub const fn key(self)'), /=> "(\w+)"/g)),
    );
  });

  it('knows exactly the drive roles and the reasons given for them', () => {
    expect(sorted(driveRoles)).toEqual(sorted(all(block(role, 'impl DriveRole'), /=> "(\w+)"/g)));
    expect(sorted(roleReasonKeys)).toEqual(sorted(all(role, /"role\.reason\.(\w+)"/g)));
  });

  it('knows exactly the reasons an item is shown but not offered', () => {
    expect(sorted(unavailableReasons)).toEqual(
      sorted(all(block(mod, 'impl UnavailableReason'), /=> "(\w+)"/g)),
    );
  });

  it('knows exactly the kinds the weekly clean-up may run, all of them real kinds', () => {
    const at = schedule.indexOf('pub const SCHEDULABLE_KINDS');
    const list = schedule.slice(schedule.indexOf('= [', at), schedule.indexOf('];', at));
    const rust = all(list, /"(\w+)"/g);
    expect(rust.length).toBeGreaterThan(0);
    expect(sorted(schedulableKinds)).toEqual(sorted(rust));
    for (const kind of schedulableKinds) expect(planItemKinds as readonly string[]).toContain(kind);
  });

  it('knows exactly the extra kinds the drive history records', () => {
    const at = history.indexOf('pub const DISK_KINDS');
    const list = history.slice(history.indexOf('= [', at), history.indexOf('];', at));
    expect(sorted(historyOnlyKinds)).toEqual(sorted(all(list, /"(\w+)"/g)));
  });
});
