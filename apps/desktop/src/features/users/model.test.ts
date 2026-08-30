import { describe, expect, it } from 'vitest';

import {
  displayName,
  filterSessions,
  isInteractive,
  rollupFor,
  sortSessions,
  type LogonSession,
  type SessionRollup,
} from './model';

function session(overrides: Partial<LogonSession> = {}): LogonSession {
  return {
    sessionId: 1,
    userName: 'Alice',
    domain: 'CORP',
    clientName: null,
    state: 'active',
    logonTime: null,
    isServices: false,
    ...overrides,
  };
}

function rollup(overrides: Partial<SessionRollup> = {}): SessionRollup {
  return {
    sessionId: 1,
    processCount: 10,
    cpuPercent: 15.5,
    memoryBytes: 1024 * 1024 * 512,
    ...overrides,
  };
}

describe('displayName', () => {
  it('formats with domain when both are present', () => {
    const s = session({ domain: 'CORP', userName: 'Alice' });
    expect(displayName(s)).toBe('CORP\\Alice');
  });

  it('formats without domain when only user is present', () => {
    const s = session({ domain: null, userName: 'Bob' });
    expect(displayName(s)).toBe('Bob');
  });

  it('returns Services for session 0', () => {
    const s = session({ sessionId: 0, userName: null, domain: null, isServices: true });
    expect(displayName(s)).toBe('Services');
  });

  it('returns listener label for listen state', () => {
    const s = session({ sessionId: 65536, userName: null, state: 'listen' });
    expect(displayName(s)).toBe('Listener (session 65536)');
  });

  it('returns session ID as fallback', () => {
    const s = session({ sessionId: 3, userName: null, domain: null, isServices: false });
    expect(displayName(s)).toBe('Session 3');
  });
});

describe('isInteractive', () => {
  it('classifies interactive states as interactive', () => {
    expect(isInteractive(session({ state: 'active' }))).toBe(true);
    expect(isInteractive(session({ state: 'connected' }))).toBe(true);
    expect(isInteractive(session({ state: 'disconnected' }))).toBe(true);
    expect(isInteractive(session({ state: 'shadow' }))).toBe(true);
  });

  it('classifies non-interactive states as not interactive', () => {
    expect(isInteractive(session({ state: 'idle' }))).toBe(false);
    expect(isInteractive(session({ state: 'listen' }))).toBe(false);
    expect(isInteractive(session({ state: 'unknown' }))).toBe(false);
  });

  it('classifies session 0 as not interactive', () => {
    const s = session({ sessionId: 0, isServices: true, state: 'active' });
    expect(isInteractive(s)).toBe(false);
  });
});

describe('rollupFor', () => {
  it('finds the matching rollup', () => {
    const s = session({ sessionId: 2 });
    const rollups = [rollup({ sessionId: 1 }), rollup({ sessionId: 2 }), rollup({ sessionId: 3 })];
    const result = rollupFor(s, rollups);
    expect(result?.sessionId).toBe(2);
  });

  it('returns null when no rollup exists', () => {
    const s = session({ sessionId: 99 });
    const rollups = [rollup({ sessionId: 1 }), rollup({ sessionId: 2 })];
    expect(rollupFor(s, rollups)).toBeNull();
  });

  it('returns null for empty rollup list', () => {
    const s = session({ sessionId: 1 });
    expect(rollupFor(s, [])).toBeNull();
  });
});

describe('sortSessions', () => {
  it('sorts interactive sessions before non-interactive', () => {
    const sessions = [
      session({ sessionId: 1, state: 'listen' }),
      session({ sessionId: 2, state: 'active' }),
      session({ sessionId: 3, state: 'idle' }),
    ];
    const sorted = sortSessions(sessions);
    expect(sorted[0]?.state).toBe('active');
  });

  it('sorts session 0 after interactive but before listeners', () => {
    const sessions = [
      session({ sessionId: 65536, state: 'listen' }),
      session({ sessionId: 0, isServices: true }),
      session({ sessionId: 2, state: 'active' }),
    ];
    const sorted = sortSessions(sessions);
    expect(sorted.map((s) => s.sessionId)).toEqual([2, 0, 65536]);
  });

  it('sorts by session ID within groups', () => {
    const sessions = [
      session({ sessionId: 3, state: 'active' }),
      session({ sessionId: 1, state: 'active' }),
      session({ sessionId: 2, state: 'active' }),
    ];
    const sorted = sortSessions(sessions);
    expect(sorted.map((s) => s.sessionId)).toEqual([1, 2, 3]);
  });

  it('does not mutate the input', () => {
    const sessions = [
      session({ sessionId: 3 }),
      session({ sessionId: 1 }),
      session({ sessionId: 2 }),
    ];
    const original = [...sessions];
    sortSessions(sessions);
    expect(sessions).toEqual(original);
  });
});

describe('filterSessions', () => {
  it('returns all sessions for empty query', () => {
    const sessions = [session({ userName: 'Alice' }), session({ userName: 'Bob' })];
    expect(filterSessions(sessions, '')).toEqual(sessions);
    expect(filterSessions(sessions, '  ')).toEqual(sessions);
  });

  it('filters by user name', () => {
    const sessions = [session({ userName: 'Alice' }), session({ userName: 'Bob' })];
    const filtered = filterSessions(sessions, 'alice');
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.userName).toBe('Alice');
  });

  it('filters by domain', () => {
    const sessions = [
      session({ domain: 'CORP', userName: 'Alice' }),
      session({ domain: 'HOME', userName: 'Bob' }),
    ];
    const filtered = filterSessions(sessions, 'corp');
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.domain).toBe('CORP');
  });

  it('filters by client name', () => {
    const sessions = [
      session({ userName: 'Alice', clientName: 'LAPTOP-001' }),
      session({ userName: 'Bob', clientName: null }),
    ];
    const filtered = filterSessions(sessions, 'laptop');
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.clientName).toBe('LAPTOP-001');
  });

  it('is case-insensitive', () => {
    const sessions = [session({ userName: 'Alice' })];
    expect(filterSessions(sessions, 'ALICE')).toHaveLength(1);
    expect(filterSessions(sessions, 'alice')).toHaveLength(1);
    expect(filterSessions(sessions, 'AlIcE')).toHaveLength(1);
  });

  it('does not mutate the input', () => {
    const sessions = [session({ userName: 'Alice' }), session({ userName: 'Bob' })];
    const original = [...sessions];
    filterSessions(sessions, 'alice');
    expect(sessions).toEqual(original);
  });
});
