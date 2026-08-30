/**
 * The connections data model: grouping, filtering, and what counts as risky.
 *
 * # Grouped by process, not a flat list
 *
 * `netstat -ano` produces a thousand rows on an idle machine and answers
 * nothing, because the question is never "what is this socket" — it is "what
 * is this *program* talking to". A browser alone accounts for hundreds of
 * rows across dozens of PIDs, and reading them individually is hopeless.
 *
 * So rows are grouped by executable name, with each process's sockets nested
 * underneath. The Rust side already computes a per-PID rollup; this merges
 * those by name so a browser's ninety renderers read as one application.
 *
 * # "Risky" is described, never judged
 *
 * There is a strong temptation to flag connections as suspicious. This module
 * deliberately does not: it has no threat intelligence, and a false positive
 * on a system process is worse than saying nothing — it teaches the user to
 * ignore the warning, and it may push them to kill something load-bearing.
 *
 * What it does instead is describe facts that are *worth noticing* and let the
 * user decide: this socket is listening on a public interface, this peer is
 * outside your network. Those are observations, not accusations.
 */

import type { Connection } from '@vitals/protocol';

export interface ConnectionRow extends Connection {
  /** Stable across refreshes, so React keys and selection survive a poll. */
  readonly id: string;
}

/** One application, with every socket its processes hold. */
export interface ConnectionGroup {
  /** Executable name, or a placeholder when the owner could not be resolved. */
  readonly name: string;
  readonly pids: readonly number[];
  readonly rows: readonly ConnectionRow[];
  readonly established: number;
  readonly listening: number;
  /** Distinct remote addresses, so twenty sockets to one CDN read as one host. */
  readonly remoteHosts: number;
  /** Rows whose peer is outside private address space. */
  readonly external: number;
  /** Listeners bound to an address reachable from off-machine. */
  readonly publicListeners: number;
}

/**
 * Identity for a socket.
 *
 * The four-tuple plus protocol, which is what actually identifies a
 * connection. Using the array index instead would make every row's key change
 * whenever a single socket above it closed — remounting the whole list on
 * every poll and destroying the user's selection.
 */
export function connectionId(connection: Connection): string {
  return [
    connection.protocol,
    connection.localAddress,
    connection.localPort,
    connection.remoteAddress ?? '',
    connection.remotePort ?? '',
  ].join('|');
}

/**
 * Whether an address is outside private/loopback space.
 *
 * Deliberately conservative: anything not recognised as local is treated as
 * external. Getting this wrong in the safe direction means showing the user a
 * connection that turns out to be local, which costs a glance. Wrong in the
 * other direction means quietly omitting a connection to the internet from a
 * count labelled "external", which is the whole point of the column.
 */
export function isExternal(address: string | null): boolean {
  if (address === null || address === '') return false;

  // IPv6 first: loopback, link-local, and unique-local.
  if (address.includes(':')) {
    const normalised = address.toLowerCase();
    if (normalised === '::1' || normalised === '::') return false;
    if (normalised.startsWith('fe80:')) return false;
    // fc00::/7 — unique local addresses.
    if (/^f[cd]/.test(normalised)) return false;
    return true;
  }

  const octets = address.split('.').map(Number);
  if (octets.length !== 4 || octets.some((part) => !Number.isInteger(part))) return false;

  const [a = 0, b = 0] = octets;
  if (a === 127 || a === 0) return false;
  if (a === 10) return false;
  if (a === 192 && b === 168) return false;
  if (a === 172 && b >= 16 && b <= 31) return false;
  // 169.254/16 link-local, and 100.64/10 carrier-grade NAT — the latter is
  // what a machine behind a VPN or a mobile hotspot sees, and calling it
  // "external" would be misleading.
  if (a === 169 && b === 254) return false;
  if (a === 100 && b >= 64 && b <= 127) return false;
  return true;
}

/**
 * Whether a listener is reachable from off-machine.
 *
 * `0.0.0.0` and `::` are the wildcard binds that accept from any interface —
 * the ones worth knowing about. A service listening only on `127.0.0.1` is
 * invisible to the network and not interesting here.
 */
export function isPublicListener(connection: Connection): boolean {
  if (connection.state !== 'listen') return false;
  const address = connection.localAddress;
  return address === '0.0.0.0' || address === '::' || address === '*';
}

export interface GroupOptions {
  /** Resolves a PID to an executable name, from the process snapshot. */
  readonly nameFor: (pid: number) => string | null;
  /** Shown when the owner cannot be resolved. Supplied translated. */
  readonly unknownLabel: string;
}

/**
 * Groups rows by owning application.
 *
 * Processes that cannot be resolved to a name collapse into one bucket rather
 * than becoming one group per PID. A machine reports a handful of these —
 * short-lived processes that exited between the socket table being read and
 * the process snapshot being taken — and a hundred single-row groups named
 * after numbers would drown the list.
 */
export function groupByApp(
  rows: readonly ConnectionRow[],
  options: GroupOptions,
): readonly ConnectionGroup[] {
  const groups = new Map<
    string,
    {
      rows: ConnectionRow[];
      pids: Set<number>;
      hosts: Set<string>;
      established: number;
      listening: number;
      external: number;
      publicListeners: number;
    }
  >();

  for (const row of rows) {
    const pid = row.ownerPid;
    const name = (pid !== null ? options.nameFor(pid) : null) ?? options.unknownLabel;

    let group = groups.get(name);
    if (group === undefined) {
      group = {
        rows: [],
        pids: new Set(),
        hosts: new Set(),
        established: 0,
        listening: 0,
        external: 0,
        publicListeners: 0,
      };
      groups.set(name, group);
    }

    group.rows.push(row);
    if (pid !== null) group.pids.add(pid);
    if (row.remoteAddress !== null && row.remoteAddress !== '') group.hosts.add(row.remoteAddress);
    if (row.state === 'established') group.established += 1;
    if (row.state === 'listen') group.listening += 1;
    if (isExternal(row.remoteAddress)) group.external += 1;
    if (isPublicListener(row)) group.publicListeners += 1;
  }

  return (
    [...groups.entries()]
      .map(([name, group]) => ({
        name,
        pids: [...group.pids].sort((a, b) => a - b),
        rows: group.rows,
        established: group.established,
        listening: group.listening,
        remoteHosts: group.hosts.size,
        external: group.external,
        publicListeners: group.publicListeners,
      }))
      // Busiest first, ties broken on name so the order is stable across polls.
      // Without the tie-break, two apps with equal counts would swap places on
      // every refresh purely from map iteration order.
      .sort((a, b) =>
        b.rows.length !== a.rows.length
          ? b.rows.length - a.rows.length
          : a.name.localeCompare(b.name),
      )
  );
}

export const connectionFilters = ['all', 'established', 'listening', 'external'] as const;
export type ConnectionFilter = (typeof connectionFilters)[number];

export function matchesFilter(row: Connection, filter: ConnectionFilter): boolean {
  switch (filter) {
    case 'all':
      return true;
    case 'established':
      return row.state === 'established';
    case 'listening':
      return row.state === 'listen';
    case 'external':
      return isExternal(row.remoteAddress);
    default: {
      const exhaustive: never = filter;
      return exhaustive;
    }
  }
}

/**
 * Free-text search across the fields a person would actually type.
 *
 * Port is matched as a whole token rather than a substring: searching "80"
 * should not return every connection on port 8080, 3080 and 44380, which is
 * what a naive `includes` does and which makes the search useless for the
 * single most common query.
 */
export function matchesQuery(row: Connection, name: string, query: string): boolean {
  const needle = query.trim().toLowerCase();
  if (needle === '') return true;

  if (name.toLowerCase().includes(needle)) return true;
  if (row.localAddress.toLowerCase().includes(needle)) return true;
  if (row.remoteAddress?.toLowerCase().includes(needle) === true) return true;
  if (row.ownerPid !== null && String(row.ownerPid) === needle) return true;

  if (/^\d+$/.test(needle)) {
    const port = Number(needle);
    return row.localPort === port || row.remotePort === port;
  }

  return false;
}

/** Applies filter and search, keeping only groups that still have rows. */
export function applySearch(
  groups: readonly ConnectionGroup[],
  filter: ConnectionFilter,
  query: string,
): readonly ConnectionGroup[] {
  const result: ConnectionGroup[] = [];

  for (const group of groups) {
    const rows = group.rows.filter(
      (row) => matchesFilter(row, filter) && matchesQuery(row, group.name, query),
    );
    // An empty group is dropped rather than shown with a zero: a list of
    // applications with no visible connections tells the user nothing and
    // makes a narrow filter look like a broken one.
    if (rows.length > 0) result.push({ ...group, rows });
  }

  return result;
}
