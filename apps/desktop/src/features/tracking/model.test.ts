/**
 * The Tracking page's pure logic: reading the index tolerantly, and the sums,
 * groupings and warnings the screen draws from it.
 *
 * Every test names the guarantee rather than the function, because the point
 * of this module is behaviour: a malformed index must degrade to "nothing
 * trustworthy to draw" rather than to a plausible-looking wrong number.
 */

import { describe, expect, it } from 'vitest';

import {
  formatAge,
  generatedAtMs,
  groupItems,
  hasDrift,
  indexStatusCounts,
  indexTotals,
  isKnownStatus,
  openCount,
  parseIndex,
  repoItemCount,
  repoStatusCounts,
  shortRef,
  sortedStatuses,
  statusTone,
  trackerStatuses,
  type TrackerIndex,
} from './model';

/** One repo in the parsed shape, with sensible defaults a test can override. */
function at<T>(items: readonly T[]): T {
  const value = items[0];
  if (value === undefined) throw new Error('fixture has no first element');
  return value;
}

function index(overrides: Partial<TrackerIndex> = {}): TrackerIndex {
  return {
    version: 1,
    generatedAt: '2026-11-28T10:00:00.000Z',
    repos: [
      {
        repo: 'vitals',
        root: 'E:\\gh\\vitals',
        git: { branch: 'main', head: 'abc1234def567890' },
        trackers: [
          {
            id: 'vitals',
            file: 'docs/tracker.csv',
            // Consistent with the rows below: the counts come from the same
            // scan the rows do, so a fixture where they disagree would test
            // the wrong thing.
            countsByStatus: { done: 330, 'in-progress': 2, todo: 8, blocked: 1 },
            items: [
              { id: 'S1', title: 'First', status: 'done', group: 'Alpha' },
              { id: 'S2', title: 'Second', status: 'todo', group: 'Alpha' },
              { id: 'S3', title: 'Loose', status: 'blocked' },
            ],
          },
        ],
        drift: [],
      },
      {
        repo: 'brivio',
        root: 'E:\\gh\\brivio',
        git: null,
        trackers: [
          {
            id: 'brivio',
            file: 'docs/TRACKER.md',
            countsByStatus: { blocked: 3, deferred: 1 },
            items: [{ id: 'B1', title: 'Blocker', status: 'blocked' }],
          },
        ],
        drift: [
          { kind: 'status', tracker: 'docs/TRACKER.md', ref: 'B1', detail: 'unknown status' },
        ],
      },
    ],
    ...overrides,
  };
}

describe('reading the index', () => {
  it('parses a well-formed index into repositories, trackers and rows', () => {
    // The raw shape is the file's, not this type's: build it as JSON would be.
    const raw = JSON.parse(JSON.stringify(index())) as unknown;
    const parsed = parseIndex(raw);
    expect(parsed).not.toBeNull();
    expect(parsed?.repos.map((r) => r.repo)).toEqual(['vitals', 'brivio']);
    expect(parsed?.repos[0]?.trackers[0]?.items).toHaveLength(3);
    expect(parsed?.repos[1]?.git).toBeNull();
  });

  it('refuses a body that is not an index rather than inventing one', () => {
    expect(parseIndex(null)).toBeNull();
    expect(parseIndex({ version: 1 })).toBeNull();
    expect(parseIndex({ repos: 'nope' })).toBeNull();
    expect(parseIndex([])).toBeNull();
  });

  it('drops a row that has no id, because a row without an id cannot be cited', () => {
    const raw = {
      version: 1,
      generatedAt: '',
      repos: [
        {
          repo: 'x',
          root: '/x',
          trackers: [
            {
              id: 'x',
              file: 'f',
              countsByStatus: {},
              items: [{ title: 'no id' }, { id: 'K1', title: 'has id', status: 'todo' }],
            },
          ],
        },
      ],
    };
    const parsed = parseIndex(raw);
    expect(parsed?.repos[0]?.trackers[0]?.items.map((i) => i.id)).toEqual(['K1']);
  });

  it('keeps an optional field off the row rather than setting it to undefined', () => {
    // `exactOptionalPropertyTypes` is on: this fails to compile if `parseItem`
    // ever writes `group: undefined`, and the runtime shape matters too.
    const raw = {
      version: 1,
      generatedAt: '',
      repos: [
        {
          repo: 'x',
          root: '/x',
          trackers: [{ id: 'x', file: 'f', countsByStatus: {}, items: [{ id: 'K1' }] }],
        },
      ],
    };
    const item = parseIndex(raw)?.repos[0]?.trackers[0]?.items[0];
    expect(item && 'group' in item).toBe(false);
    expect(item && 'evidence' in item).toBe(false);
  });

  it('defaults a row with no status to todo instead of dropping it', () => {
    const raw = {
      version: 1,
      generatedAt: '',
      repos: [
        {
          repo: 'x',
          root: '/x',
          trackers: [{ id: 'x', file: 'f', countsByStatus: {}, items: [{ id: 'K1' }] }],
        },
      ],
    };
    expect(parseIndex(raw)?.repos[0]?.trackers[0]?.items[0]?.status).toBe('todo');
  });

  it('drops a zero from the counts so a status that fell to nothing leaves no chip', () => {
    const raw = {
      version: 1,
      generatedAt: '',
      repos: [
        {
          repo: 'x',
          root: '/x',
          trackers: [{ id: 'x', file: 'f', countsByStatus: { done: 0, todo: 4 }, items: [] }],
        },
      ],
    };
    expect(parseIndex(raw)?.repos[0]?.trackers[0]?.countsByStatus).toEqual({ todo: 4 });
  });
});

describe('the totals the strip and the warnings draw', () => {
  it('sums each status across a repo\u2019s trackers', () => {
    const counts = repoStatusCounts(at(index().repos));
    expect(counts).toEqual({ done: 330, 'in-progress': 2, todo: 8, blocked: 1 });
  });

  it('sums each status across every repo', () => {
    const counts = indexStatusCounts(index());
    expect(counts['done']).toBe(330);
    expect(counts['blocked']).toBe(4);
    expect(counts['deferred']).toBe(1);
  });

  it('counts open rows as everything that is not done, dropped or deferred', () => {
    expect(openCount({ done: 10, dropped: 2, deferred: 1 })).toBe(0);
    expect(openCount({ todo: 3, blocked: 1 })).toBe(4);
    // A status nobody recognises is a row a human still has to act on.
    expect(openCount({ weird: 2 })).toBe(2);
  });

  it('headlines repositories, trackers, rows and drift', () => {
    const totals = indexTotals(index());
    expect(totals.repos).toBe(2);
    expect(totals.trackers).toBe(2);
    expect(totals.items).toBe(4);
    expect(totals.drift).toBe(1);
    expect(totals.reposWithDrift).toBe(1);
    expect(totals.byStatus['blocked']).toBe(4);
  });

  it('flags only the repositories that actually drift', () => {
    const [vitals, brivio] = index().repos;
    expect(vitals !== undefined && hasDrift(vitals)).toBe(false);
    expect(brivio !== undefined && hasDrift(brivio)).toBe(true);
  });

  it('counts rows per repo from the rows, not from the counts', () => {
    expect(repoItemCount(at(index().repos))).toBe(3);
  });
});

describe('reading a tally', () => {
  it('orders statuses by row count, alphabetically on a tie', () => {
    expect(sortedStatuses({ todo: 1, done: 5, blocked: 1 })).toEqual(['done', 'blocked', 'todo']);
  });

  it('maps each known status onto a tone and anything else onto neutral', () => {
    expect(statusTone('blocked')).toBe('danger');
    expect(statusTone('done')).toBe('ok');
    expect(statusTone('something-new')).toBe('neutral');
  });

  it('recognises exactly the canonical status vocabulary', () => {
    for (const status of trackerStatuses) expect(isKnownStatus(status)).toBe(true);
    expect(isKnownStatus('unknown')).toBe(false);
  });
});

describe('grouping rows', () => {
  it('preserves the order groups were first seen in, and buckets ungrouped under null', () => {
    const groups = groupItems(at(at(index().repos).trackers).items);
    expect(groups.map((g) => g.group)).toEqual(['Alpha', null]);
    expect(groups[0]?.items.map((i) => i.id)).toEqual(['S1', 'S2']);
    expect(groups[1]?.items.map((i) => i.id)).toEqual(['S3']);
  });

  it('returns one ungrouped bucket for rows that never set a group', () => {
    const groups = groupItems([{ id: 'A', title: 'a', status: 'todo' }]);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.group).toBeNull();
  });

  it('returns nothing for a tracker with no rows', () => {
    expect(groupItems([])).toEqual([]);
  });
});

describe('the age of a scan', () => {
  it('parses an ISO timestamp and refuses an empty one', () => {
    expect(generatedAtMs({ version: 1, generatedAt: '1970-01-01T00:00:01.000Z', repos: [] })).toBe(
      1000,
    );
    expect(generatedAtMs({ version: 1, generatedAt: '', repos: [] })).toBeNull();
    expect(generatedAtMs({ version: 1, generatedAt: 'not a date', repos: [] })).toBeNull();
  });

  it('formats a duration in hours, minutes and seconds', () => {
    expect(formatAge(45_000)).toBe('45s');
    expect(formatAge(90_000)).toBe('1m 30s');
    expect(formatAge(3_900_000)).toBe('1h 05m');
    expect(formatAge(null)).toBe('\u2014');
    expect(formatAge(-1)).toBe('\u2014');
  });
});

describe('naming the branch', () => {
  it('drops the ref prefix the scanner records and leaves anything else alone', () => {
    // What the real index holds: the scanner records the symbolic ref.
    expect(shortRef('refs/heads/main')).toBe('main');
    expect(shortRef('refs/heads/feature/tracking-page')).toBe('feature/tracking-page');
    // A detached head is a sha, and a bare name is already a name.
    expect(shortRef('4fd4b894c886')).toBe('4fd4b894c886');
    expect(shortRef('main')).toBe('main');
    // A prefix with nothing behind it would render an empty branch, so it is
    // shown as recorded instead.
    expect(shortRef('refs/heads/')).toBe('refs/heads/');
  });
});
