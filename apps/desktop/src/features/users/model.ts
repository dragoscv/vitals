/**
 * Users: the data shape and pure transforms.
 *
 * # Session 0 and listeners are not hidden
 *
 * Session 0 is the Services session and is not an interactive user. Listening
 * sessions are idle connection slots. Both are included in the list and
 * classified by `isInteractive` rather than filtered out — so their presence
 * is legible.
 *
 * # "Unknown" is never rounded to a convenient answer
 *
 * The state enum carries an explicit unknown, which surfaces when Windows
 * returns a value this build cannot name. Reporting it as Active or
 * Disconnected would be a fabricated fact.
 */

export type SessionStateKey =
  'active' | 'connected' | 'disconnected' | 'idle' | 'listen' | 'shadow' | 'unknown';

export interface LogonSession {
  readonly sessionId: number;
  readonly userName: string | null;
  readonly domain: string | null;
  readonly clientName: string | null;
  readonly state: SessionStateKey;
  /// Unix timestamp in seconds, or null if unavailable.
  readonly logonTime: number | null;
  readonly isServices: boolean;
}

export interface SessionRollup {
  readonly sessionId: number;
  readonly processCount: number;
  readonly cpuPercent: number;
  readonly memoryBytes: number;
}

export interface UsersSnapshot {
  readonly sessions: readonly LogonSession[];
  readonly rollups: readonly SessionRollup[];
}

/**
 * The display name: DOMAIN\User, or User if there is no domain, or a fallback
 * label for session 0 or a listener.
 */
export function displayName(session: LogonSession): string {
  if (session.domain && session.userName) {
    return `${session.domain}\\${session.userName}`;
  }
  if (session.userName) {
    return session.userName;
  }
  if (session.isServices) {
    return 'Services';
  }
  if (session.state === 'listen') {
    return `Listener (session ${session.sessionId})`;
  }
  return `Session ${session.sessionId}`;
}

/**
 * Whether this session is an interactive user.
 *
 * Session 0 and listeners are not — they are classified rather than hidden, so
 * the UI can explain their presence rather than leaving them out and looking
 * incomplete.
 */
export function isInteractive(session: LogonSession): boolean {
  if (session.isServices) return false;
  return ['active', 'connected', 'disconnected', 'shadow'].includes(session.state);
}

/**
 * Finds the rollup for a session, if it exists.
 */
export function rollupFor(
  session: LogonSession,
  rollups: readonly SessionRollup[],
): SessionRollup | null {
  return rollups.find((r) => r.sessionId === session.sessionId) ?? null;
}

/**
 * Sorts sessions: interactive first, then session 0, then listeners, then by
 * session ID within each group.
 */
export function sortSessions(sessions: readonly LogonSession[]): LogonSession[] {
  return [...sessions].sort((a, b) => {
    const aInteractive = isInteractive(a);
    const bInteractive = isInteractive(b);

    if (aInteractive && !bInteractive) return -1;
    if (!aInteractive && bInteractive) return 1;

    if (a.isServices && !b.isServices) return -1;
    if (!a.isServices && b.isServices) return 1;

    return a.sessionId - b.sessionId;
  });
}

/**
 * Filters sessions by a search query.
 *
 * Matches against the display name, domain, user name, and client name.
 */
export function filterSessions(sessions: readonly LogonSession[], query: string): LogonSession[] {
  if (query.trim() === '') return [...sessions];

  const lower = query.toLowerCase();
  return sessions.filter((session) => {
    const name = displayName(session).toLowerCase();
    const domain = session.domain?.toLowerCase() ?? '';
    const user = session.userName?.toLowerCase() ?? '';
    const client = session.clientName?.toLowerCase() ?? '';

    return (
      name.includes(lower) ||
      domain.includes(lower) ||
      user.includes(lower) ||
      client.includes(lower)
    );
  });
}
