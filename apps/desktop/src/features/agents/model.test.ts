import { describe, expect, it } from 'vitest';

import { NOW, makeSession, makeSnapshot, makeTicket } from './fixtures.test-utils';
import {
  activeShare,
  byClient,
  eventTone,
  formatDuration,
  headline,
  repoName,
  resourceSlots,
  workFor,
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
});
