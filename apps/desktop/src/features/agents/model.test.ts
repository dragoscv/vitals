import { describe, expect, it } from 'vitest';

import {
  NOW,
  makeEstimate,
  makeSession,
  makeSnapshot,
  makeTicket,
  makeUsageSnapshot,
  makeWork,
} from './fixtures.test-utils';
import {
  activeShare,
  actionPreview,
  buildAction,
  byClient,
  cloneFromSlot,
  eventTone,
  formatCount,
  formatDuration,
  headline,
  initialFields,
  repoName,
  resourceSlots,
  suggestions,
  usageWork,
  workEta,
  workFor,
  workDetail,
} from './model';

describe('agents model', () => {
  it('headline counts holders, waiters and dead tickets separately', () => {
    const h = headline(makeSnapshot());
    expect(h.building).toBe(2);
    expect(h.queued).toBe(1);
    expect(h.stale).toBe(1);
    expect(h.working).toBe(1);
  });

  it('a session is joined to its agentq tickets and lease through the caller registry pid', () => {
    const s = makeSnapshot();
    const work = workFor(s.sessions[0]!, s);
    expect(work.tickets.map((t) => t.id).sort()).toEqual(['m1', 't1', 't2']);
    expect(work.leases).toHaveLength(1);
    expect(work.caller?.app).toBe('Code - Insiders');
  });

  it('an unrelated session in another repo gets no agentq work', () => {
    const s = makeSnapshot();
    const other = makeSession({ id: 'x', pid: 1, repo: 'brivio', workspace: 'E:\\gh\\brivio' });
    expect(workFor(other, s).tickets).toHaveLength(0);
  });

  it('a worktree slot path names the repo, not the slot', () => {
    expect(repoName('E:\\gh\\.wt\\codai\\task-3')).toBe('codai');
    expect(repoName('dragoscv/vitals')).toBe('vitals');
    expect(repoName(null)).toBeNull();
  });

  it('active share is active time over the session life, capped at 1, unknown without a start', () => {
    expect(activeShare(makeSession(), NOW)).toBeCloseTo(0.25);
    expect(activeShare(makeSession({ startedMs: null }), NOW)).toBeNull();
    expect(activeShare(makeSession({ activeMs: 99_999_999 }), NOW)).toBe(1);
  });

  it('a session that reports no counters shows a dash, not zero, and no active share', () => {
    const cloud = makeSession({ activeMs: null, turns: null, toolCalls: null });
    expect(activeShare(cloud, NOW)).toBeNull();
    expect(formatCount(cloud.turns)).toBe('—');
    expect(formatCount(0)).toBe('0');
  });

  it('live clients sort first', () => {
    const groups = byClient([
      makeSession({ id: 'a', client: 'codex', state: 'ended' }),
      makeSession({ id: 'b', client: 'copilotCli', state: 'working' }),
    ]);
    expect(groups[0]?.client).toBe('copilotCli');
  });

  it('slots never show fewer boxes than holders even when capacity is unknown', () => {
    const r = {
      name: 'x',
      kind: 'other' as const,
      capacity: null,
      tickets: [makeTicket(), makeTicket({ id: 'b' })],
    };
    expect(resourceSlots(r).capacity).toBe(2);
  });

  it('durations and tones', () => {
    expect(formatDuration(3_840_000)).toBe('1h 04m');
    expect(formatDuration(null)).toBe('—');
    expect(eventTone('commit-failed')).toBe('danger');
    expect(eventTone('idle-yield')).toBe('warn');
    expect(eventTone('done')).toBe('ok');
  });

  it('reads the bar and the ETA off the one duration the estimate gives', () => {
    // One figure, two readings: the bar is elapsed/expected and the ETA is
    // expected-elapsed, so a refresh that lands late cannot make the bar move
    // backwards or the ETA disagree with it.
    const work = makeWork({ estimate: makeEstimate({ expectedMs: 300_000, samples: 12 }) });
    const eta = workEta(work, NOW);
    expect(eta).not.toBeNull();
    expect(eta?.elapsedMs).toBe(120_000);
    expect(eta?.expectedMs).toBe(300_000);
    expect(eta?.remainingMs).toBe(180_000);
    expect(eta?.over).toBe(false);
    expect(eta?.overMs).toBe(0);
    expect(eta?.basis).toBe('measured');
    expect(eta?.samples).toBe(12);
  });

  it('reports the overrun once the estimate is passed, with nothing remaining', () => {
    const work = makeWork({
      startedMs: NOW - 400_000,
      estimate: makeEstimate({ expectedMs: 300_000, basis: 'declared', samples: 0 }),
    });
    const eta = workEta(work, NOW);
    expect(eta?.over).toBe(true);
    expect(eta?.overMs).toBe(100_000);
    expect(eta?.remainingMs).toBe(0);
    // Whose figure it is travels with it: an overrun of a launcher's promise
    // and one of the usual for this resource are different news.
    expect(eta?.basis).toBe('declared');
  });

  it('has no progress to show without an estimate, a start, or a real duration', () => {
    // No estimate is the common case: a resource nobody has finished has no
    // history, and no bar is drawn for it rather than a bar sitting at zero.
    expect(workEta(makeWork(), NOW)).toBeNull();
    expect(workEta(makeWork({ startedMs: null, estimate: makeEstimate() }), NOW)).toBeNull();
    expect(workEta(makeWork({ estimate: makeEstimate({ expectedMs: 0 }) }), NOW)).toBeNull();
    expect(
      workEta(makeWork({ estimate: makeEstimate({ expectedMs: Number.NaN }) }), NOW),
    ).toBeNull();
    // A clock that moved backwards mid-sample is not negative elapsed time.
    expect(
      workEta(makeWork({ startedMs: NOW + 5_000, estimate: makeEstimate() }), NOW)?.elapsedMs,
    ).toBe(0);
  });
});

describe('agents action builder', () => {
  it('refuses to build an action while a required field is still empty', () => {
    expect(buildAction('freeze', { resource: '', reason: 'incident' })).toBeNull();
    expect(buildAction('freeze', { resource: 'build:codai', reason: '' })).toBeNull();
    expect(buildAction('freeze', { resource: 'build:codai', reason: 'incident' })).toEqual({
      kind: 'freeze',
      resource: 'build:codai',
      reason: 'incident',
    });
  });

  it('sends an optional flag as null rather than omitting it', () => {
    // The wire type says the field is nullable, and a missing key is not the
    // same fact to a serializer that has a default.
    expect(buildAction('propose', { reason: '' })).toEqual({ kind: 'propose', reason: null });
    expect(buildAction('propose', { reason: 'stuck' })).toEqual({
      kind: 'propose',
      reason: 'stuck',
    });
  });

  it('a sweep opens in preview so the first click cannot reap anything', () => {
    const open = initialFields('sweep');
    expect(open.dryRun).toBe('true');
    expect(buildAction('sweep', open)).toEqual({ kind: 'sweep', dryRun: true });
    expect(buildAction('sweep', { dryRun: 'false' })).toEqual({ kind: 'sweep', dryRun: false });
  });

  it('a command with shell metacharacters stays one value in the action', () => {
    const a = buildAction('run', {
      resource: 'build:codai',
      purpose: 'typecheck',
      command: 'pnpm tsc; rm -rf /',
    });
    expect(a).toEqual({
      kind: 'run',
      resource: 'build:codai',
      purpose: 'typecheck',
      command: 'pnpm tsc; rm -rf /',
      lane: null,
      place: null,
      repo: null,
    });
  });

  it('the preview reads back the command the action becomes', () => {
    expect(
      actionPreview({
        kind: 'land',
        repo: 'E:\\gh\\vitals',
        branch: 'feat/x',
        onto: 'main',
        purpose: 'ship it',
        gate: null,
      }),
    ).toContain('agentq land -Branch feat/x -Onto main -Purpose "ship it"');
    expect(
      actionPreview({
        kind: 'commit',
        repo: 'E:\\gh\\vitals',
        purpose: 'fix',
        message: 'fix(x): y',
        paths: ['a.ts', 'b.ts'],
      }),
    ).toContain('-Paths a.ts,b.ts');
  });

  it('names the main clone from a worktree slot, and nothing from anything else', () => {
    expect(cloneFromSlot('E:\\gh\\.wt\\codai\\task-3', 'codai')).toBe('E:\\gh\\codai');
    expect(cloneFromSlot('E:\\gh\\vitals', 'vitals')).toBeNull();
    expect(cloneFromSlot(null, 'vitals')).toBeNull();
  });

  it('offers the safe remedy for a snapshot, naming the count the string reads', () => {
    const s = makeSnapshot(); // carries one dead ticket, so one holder is stuck
    const list = suggestions(s, NOW);
    const propose = list.find((x) => x.id === 'propose');
    expect(propose?.action.kind).toBe('propose');
    // The key is `holders`, never `count`: a `count` key would make i18next
    // look for plural forms the bundles do not define.
    expect(propose?.params).toEqual({ holders: 1 });
  });

  it('suggests reclaiming a slot only when one is actually stale', () => {
    const fresh = suggestions(makeSnapshot(), NOW).map((x) => x.id);
    expect(fresh).not.toContain('sweep');
    const stale = makeSnapshot({
      leases: [{ ...makeSnapshot().leases[0]!, stale: true }],
    });
    expect(suggestions(stale, NOW).map((x) => x.id)).toContain('sweep');
  });

  it('passes agentq’s work rows through in the order the backend measured them', () => {
    const rows = [makeWork({ pid: 2 }), makeWork({ pid: 1 })];
    expect(usageWork(makeUsageSnapshot({ work: rows }))).toEqual(rows);
    expect(usageWork(null)).toEqual([]);
  });

  it('shows a purpose in preference to the command, and the command when there is none', () => {
    expect(workDetail(makeWork({ purpose: 'why', command: 'pnpm build' }))).toBe('why');
    expect(workDetail(makeWork({ purpose: null, command: 'pnpm build' }))).toBe('pnpm build');
    expect(workDetail(makeWork({ purpose: null, command: null }))).toBeNull();
  });
});
