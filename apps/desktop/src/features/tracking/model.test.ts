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
  canClaim,
  canComplete,
  formatAge,
  generatedAtMs,
  groupItems,
  hasDrift,
  indexStatusCounts,
  indexTotals,
  isKnownRootIssue,
  isKnownStatus,
  isKnownTaskStatus,
  knownRootIssues,
  openCount,
  openNextSteps,
  parseIndex,
  parseResumePack,
  repoItemCount,
  repoStatusCounts,
  sessionClaims,
  sessionOwners,
  shortRef,
  sortedStatuses,
  statusTone,
  taskOpen,
  taskStatuses,
  taskTone,
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
        rootIssue: null,
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
        rootIssue: null,
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

/** A one-repo raw index carrying exactly the `rootIssue` given. */
function rawRootIssue(rootIssue: unknown): unknown {
  return {
    version: 1,
    generatedAt: '',
    repos: [{ repo: 'x', root: '/x', rootIssue, trackers: [], drift: [] }],
  };
}

describe('the root issue a repository may carry', () => {
  it('reads an absent root issue as healthy, so an index written before the field still parses', () => {
    const raw = {
      version: 1,
      generatedAt: '',
      repos: [{ repo: 'x', root: '/x', trackers: [], drift: [] }],
    };
    expect(parseIndex(raw)?.repos[0]?.rootIssue).toBeNull();
  });

  it('reads an explicit null as healthy too', () => {
    expect(parseIndex(rawRootIssue(null))?.repos[0]?.rootIssue).toBeNull();
  });

  it('keeps a recorded root issue code so the card can warn about it', () => {
    expect(parseIndex(rawRootIssue('mapping-missing-at-root'))?.repos[0]?.rootIssue).toBe(
      'mapping-missing-at-root',
    );
  });

  it('keeps a code it does not recognise rather than reading it as healthy', () => {
    // Failing to warn about a real problem is worse than warning in generic
    // words: an unknown code must survive the parse.
    expect(parseIndex(rawRootIssue('something-new'))?.repos[0]?.rootIssue).toBe('something-new');
  });

  it('recognises exactly the codes it has a specific message for', () => {
    for (const code of knownRootIssues) expect(isKnownRootIssue(code)).toBe(true);
    expect(isKnownRootIssue('something-new')).toBe(false);
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

/**
 * A resume pack as the host returns it: the raw file shapes, not this type's.
 * `chat-1` owns one task, its subagent `term-7` owns another, one task is
 * nobody's, and one is already finished.
 */
function rawPack(): Record<string, unknown> {
  return {
    sessionId: 'chat-1',
    plan: {
      goal: 'Ship the resume pack',
      doneMeans: ['typecheck clean', 'tests green'],
      next: [
        { title: 'Write the panel', status: 'todo' },
        { title: 'Already done', status: 'done' },
      ],
      roster: [{ agentName: 'term-7', purpose: 'screen test', status: 'done' }],
    },
    tasks: [
      { id: 'S26-37', title: 'Panel', status: 'claimed', owner: 'chat-1' },
      { id: 'S26-38', title: 'Rust', status: 'in-progress', owner: 'term-7' },
      { id: 'S26-39', title: 'Free', status: 'todo' },
      { id: 'S26-40', title: 'Finished', status: 'done', owner: 'chat-1' },
    ],
  };
}

describe('reading a resume pack', () => {
  it('parses the session, its plan and every task in the store', () => {
    const parsed = parseResumePack(JSON.parse(JSON.stringify(rawPack())) as unknown);
    expect(parsed?.sessionId).toBe('chat-1');
    expect(parsed?.plan?.goal).toBe('Ship the resume pack');
    expect(parsed?.plan?.doneMeans).toEqual(['typecheck clean', 'tests green']);
    expect(parsed?.tasks.map((t) => t.id)).toEqual(['S26-37', 'S26-38', 'S26-39', 'S26-40']);
  });

  it('refuses a body that is not a pack rather than inventing one', () => {
    expect(parseResumePack(null)).toBeNull();
    expect(parseResumePack('nope')).toBeNull();
    expect(parseResumePack([])).toBeNull();
  });

  it('reads a machine that has written nothing as an empty pack, not an error', () => {
    expect(parseResumePack({})).toEqual({ sessionId: null, plan: null, tasks: [] });
  });

  it('drops a task with no id, because a task without an id cannot be claimed', () => {
    const parsed = parseResumePack({ tasks: [{ title: 'no id' }, { id: 'K1' }] });
    expect(parsed?.tasks.map((t) => t.id)).toEqual(['K1']);
    // A task with no status is open, the way `track.mjs` reads its own default.
    expect(at(parsed?.tasks ?? []).status).toBe('todo');
  });

  it('drops a plan with no goal, because that is the one field a resume must not lose', () => {
    expect(parseResumePack({ plan: { next: [{ title: 'orphan' }] } })?.plan).toBeNull();
  });

  it('keeps an absent owner off the task rather than setting it to undefined', () => {
    // `exactOptionalPropertyTypes` is on: this fails to compile if `parsePlanTask`
    // ever writes `owner: undefined`, and the runtime shape matters too.
    const task = parseResumePack({ tasks: [{ id: 'K1', status: 'todo' }] })?.tasks[0];
    expect(task && 'owner' in task).toBe(false);
    expect(task && 'evidence' in task).toBe(false);
  });
});

describe('the open steps of a plan', () => {
  it('hides a step the plan already marked done', () => {
    const plan = parseResumePack(rawPack())?.plan ?? null;
    expect(openNextSteps(plan).map((s) => s.title)).toEqual(['Write the panel']);
  });

  it('returns nothing when there is no plan at all', () => {
    expect(openNextSteps(null)).toEqual([]);
  });
});

describe('what a session may claim and complete', () => {
  const tasks = parseResumePack(rawPack())?.tasks ?? [];
  const free = at(tasks.filter((t) => t.id === 'S26-39'));
  const mine = at(tasks.filter((t) => t.id === 'S26-37'));
  const theirs = at(tasks.filter((t) => t.id === 'S26-38'));
  const finished = at(tasks.filter((t) => t.id === 'S26-40'));

  it('lets a session claim an unowned open task', () => {
    expect(canClaim(free, 'chat-1')).toBe(true);
  });

  it('refuses a task another session already holds', () => {
    expect(canClaim(theirs, 'chat-1')).toBe(false);
    // The holder may re-claim its own, which is what makes a retry safe.
    expect(canClaim(mine, 'chat-1')).toBe(true);
  });

  it('refuses anything when there is no session to claim as', () => {
    expect(canClaim(free, null)).toBe(false);
    expect(canComplete(mine, null)).toBe(false);
  });

  it('lets the owner complete what it holds, but not what it never claimed', () => {
    expect(canComplete(mine, 'chat-1')).toBe(true);
    expect(canComplete(free, 'chat-1')).toBe(false);
    expect(canComplete(finished, 'chat-1')).toBe(false);
  });

  it('treats done and dropped as no longer open, and nothing else as closed', () => {
    expect(taskOpen(finished)).toBe(false);
    expect(taskOpen(mine)).toBe(true);
    expect(taskOpen({ id: 'x', title: '', status: 'dropped' })).toBe(false);
    // A status nobody recognises is still work a human has to act on.
    expect(taskOpen({ id: 'x', title: '', status: 'weird' })).toBe(true);
  });
});

describe('the claims that belong to one session', () => {
  it('includes a subagent\u2019s claim, because a resume must wait on or re-dispatch it', () => {
    const parsed = parseResumePack(rawPack());
    const ids = sessionClaims(parsed?.tasks ?? [], parsed?.plan ?? null, 'chat-1').map((t) => t.id);
    expect(ids).toEqual(['S26-37', 'S26-38', 'S26-40']);
  });

  it('still counts the roster alone when the session has no name for itself', () => {
    const owners = sessionOwners(parseResumePack(rawPack())?.plan ?? null, null);
    expect([...owners]).toEqual(['term-7']);
  });

  it('returns nothing when there is neither a session id nor a roster', () => {
    expect(sessionClaims([{ id: 'K1', title: '', status: 'todo' }], null, null)).toEqual([]);
  });
});

describe('the tone of a task status', () => {
  it('recognises exactly the task vocabulary, and tones anything else neutral', () => {
    for (const status of taskStatuses) expect(isKnownTaskStatus(status)).toBe(true);
    expect(isKnownTaskStatus('unknown')).toBe(false);
    expect(taskTone('done')).toBe('ok');
    expect(taskTone('in-progress')).toBe('accent');
    expect(taskTone('unknown')).toBe('neutral');
  });
});
