import { describe, expect, it } from 'vitest';

import type { Connection } from '@vitals/protocol';

import {
  applySearch,
  connectionId,
  groupByApp,
  isExternal,
  isPublicListener,
  matchesFilter,
  matchesQuery,
  type ConnectionRow,
} from './model';

function makeConnection(overrides: Partial<Connection> = {}): Connection {
  return {
    protocol: 'tcp',
    localAddress: '192.168.1.20',
    localPort: 51234,
    remoteAddress: '140.82.121.4',
    remotePort: 443,
    state: 'established',
    ownerPid: 1000,
    ownerName: null,
    remoteHost: null,
    country: null,
    bytesSent: null,
    bytesReceived: null,
    ...overrides,
  } as Connection;
}

function makeRow(overrides: Partial<Connection> = {}): ConnectionRow {
  const connection = makeConnection(overrides);
  return { ...connection, id: connectionId(connection) };
}

const options = { nameFor: () => null, unknownLabel: 'Unknown program' };

describe('connectionId', () => {
  it('identifies a socket by its four-tuple and protocol', () => {
    // Not the array index: a socket closing above a row would otherwise change
    // every subsequent key, remounting the list on every poll and destroying
    // the user's selection and scroll position.
    const a = connectionId(makeConnection({ localPort: 100 }));
    const b = connectionId(makeConnection({ localPort: 101 }));

    expect(a).not.toBe(b);
    expect(connectionId(makeConnection({ localPort: 100 }))).toBe(a);
  });

  it('distinguishes TCP from UDP on the same ports', () => {
    expect(connectionId(makeConnection({ protocol: 'tcp' }))).not.toBe(
      connectionId(makeConnection({ protocol: 'udp' })),
    );
  });

  it('is stable when a null remote is involved', () => {
    const listener = makeConnection({ remoteAddress: null, remotePort: null });
    expect(connectionId(listener)).toBe(connectionId({ ...listener }));
  });
});

describe('isExternal', () => {
  it('treats every private IPv4 range as local', () => {
    // RFC1918 plus loopback, link-local and CGNAT. A machine behind a VPN or
    // a mobile hotspot sees 100.64/10, and calling that "external" would be
    // misleading in exactly the case a user is investigating a VPN.
    for (const address of [
      '127.0.0.1',
      '10.0.0.5',
      '192.168.1.1',
      '172.16.0.1',
      '172.31.255.254',
      '169.254.1.1',
      '100.64.0.1',
      '0.0.0.0',
    ]) {
      expect(isExternal(address), address).toBe(false);
    }
  });

  it('does not mistake 172.32 for a private address', () => {
    // The 172 range is 16-31 only. A naive `startsWith('172.')` gets this
    // wrong and silently hides real internet traffic from the external count.
    expect(isExternal('172.32.0.1')).toBe(true);
    expect(isExternal('172.15.0.1')).toBe(true);
  });

  it('recognises public IPv4', () => {
    expect(isExternal('140.82.121.4')).toBe(true);
    expect(isExternal('8.8.8.8')).toBe(true);
  });

  it('handles IPv6 loopback, link-local and unique-local', () => {
    for (const address of ['::1', '::', 'fe80::1', 'fd00::1', 'fc00::1']) {
      expect(isExternal(address), address).toBe(false);
    }
    expect(isExternal('2606:4700::1111')).toBe(true);
  });

  it('treats a missing address as local rather than guessing', () => {
    // A listener has no peer. Counting it as external would inflate the one
    // number on the screen that is supposed to mean "talking to the internet".
    expect(isExternal(null)).toBe(false);
    expect(isExternal('')).toBe(false);
  });
});

describe('isPublicListener', () => {
  it('flags a wildcard bind', () => {
    expect(isPublicListener(makeConnection({ state: 'listen', localAddress: '0.0.0.0' }))).toBe(
      true,
    );
    expect(isPublicListener(makeConnection({ state: 'listen', localAddress: '::' }))).toBe(true);
  });

  it('does not flag a loopback-only listener', () => {
    // Invisible to the network, so not worth telling anyone about.
    expect(isPublicListener(makeConnection({ state: 'listen', localAddress: '127.0.0.1' }))).toBe(
      false,
    );
  });

  it('does not flag an outbound connection from a wildcard-looking address', () => {
    expect(
      isPublicListener(makeConnection({ state: 'established', localAddress: '0.0.0.0' })),
    ).toBe(false);
  });
});

describe('groupByApp', () => {
  it('merges every PID of one application', () => {
    // A browser is dozens of PIDs. Reading them individually is hopeless,
    // which is the whole reason this screen groups.
    const rows = [
      makeRow({ ownerPid: 1 }),
      makeRow({ ownerPid: 2, localPort: 2 }),
      makeRow({ ownerPid: 3, localPort: 3 }),
    ];
    const names = new Map([
      [1, 'chrome.exe'],
      [2, 'chrome.exe'],
      [3, 'code.exe'],
    ]);

    const groups = groupByApp(rows, { ...options, nameFor: (pid) => names.get(pid) ?? null });

    expect(groups).toHaveLength(2);
    expect(groups[0]?.name).toBe('chrome.exe');
    expect(groups[0]?.pids).toEqual([1, 2]);
  });

  it('collapses unresolvable owners into one bucket', () => {
    // Short-lived processes that exited between the socket table and the
    // process snapshot. A hundred single-row groups named after numbers would
    // drown the list.
    const rows = [
      makeRow({ ownerPid: 90001, localPort: 1 }),
      makeRow({ ownerPid: 90002, localPort: 2 }),
    ];

    const groups = groupByApp(rows, options);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.name).toBe('Unknown program');
  });

  it('counts distinct hosts, not sockets', () => {
    // Twenty sockets to one CDN edge is one host, and reporting twenty would
    // make an ordinary page load look like a botnet.
    const rows = [
      makeRow({ localPort: 1, remoteAddress: '1.1.1.1' }),
      makeRow({ localPort: 2, remoteAddress: '1.1.1.1' }),
      makeRow({ localPort: 3, remoteAddress: '8.8.8.8' }),
    ];

    expect(groupByApp(rows, options)[0]?.remoteHosts).toBe(2);
  });

  it('counts states and external peers separately', () => {
    const rows = [
      makeRow({ localPort: 1, state: 'established', remoteAddress: '8.8.8.8' }),
      makeRow({ localPort: 2, state: 'established', remoteAddress: '10.0.0.1' }),
      makeRow({ localPort: 3, state: 'listen', localAddress: '0.0.0.0', remoteAddress: null }),
    ];

    const group = groupByApp(rows, options)[0];
    expect(group?.established).toBe(2);
    expect(group?.listening).toBe(1);
    expect(group?.external).toBe(1);
    expect(group?.publicListeners).toBe(1);
  });

  it('orders busiest first, breaking ties deterministically', () => {
    // Without the tie-break two equal apps swap places on every two-second
    // poll purely from map iteration order.
    const rows = [makeRow({ ownerPid: 1, localPort: 1 }), makeRow({ ownerPid: 2, localPort: 2 })];
    const names = new Map([
      [1, 'zeta.exe'],
      [2, 'alpha.exe'],
    ]);

    const groups = groupByApp(rows, { ...options, nameFor: (pid) => names.get(pid) ?? null });
    expect(groups.map((group) => group.name)).toEqual(['alpha.exe', 'zeta.exe']);
  });

  it('is empty for no rows', () => {
    expect(groupByApp([], options)).toEqual([]);
  });
});

describe('matchesFilter', () => {
  it('passes everything on "all"', () => {
    expect(matchesFilter(makeConnection({ state: 'closed' }), 'all')).toBe(true);
  });

  it('separates active from listening', () => {
    expect(matchesFilter(makeConnection({ state: 'established' }), 'established')).toBe(true);
    expect(matchesFilter(makeConnection({ state: 'listen' }), 'established')).toBe(false);
    expect(matchesFilter(makeConnection({ state: 'listen' }), 'listening')).toBe(true);
  });

  it('selects external peers', () => {
    expect(matchesFilter(makeConnection({ remoteAddress: '8.8.8.8' }), 'external')).toBe(true);
    expect(matchesFilter(makeConnection({ remoteAddress: '10.0.0.1' }), 'external')).toBe(false);
  });
});

describe('matchesQuery', () => {
  it('matches an empty query', () => {
    expect(matchesQuery(makeConnection(), 'chrome.exe', '   ')).toBe(true);
  });

  it('matches the application name', () => {
    expect(matchesQuery(makeConnection(), 'chrome.exe', 'CHROME')).toBe(true);
  });

  it('matches a port as a whole token, not a substring', () => {
    // Searching "80" must not return 8080, 3080 and 44380 — which is what a
    // naive `includes` does, and it makes the single most common query
    // useless.
    const connection = makeConnection({ localPort: 8080, remotePort: 443 });

    expect(matchesQuery(connection, 'app.exe', '8080')).toBe(true);
    expect(matchesQuery(connection, 'app.exe', '443')).toBe(true);
    expect(matchesQuery(connection, 'app.exe', '80')).toBe(false);
  });

  it('matches a partial address', () => {
    expect(matchesQuery(makeConnection({ remoteAddress: '140.82.121.4' }), 'x', '140.82')).toBe(
      true,
    );
  });

  it('matches an exact PID', () => {
    expect(matchesQuery(makeConnection({ ownerPid: 4242 }), 'x', '4242')).toBe(true);
  });
});

describe('applySearch', () => {
  const groups = groupByApp(
    [
      makeRow({ ownerPid: 1, localPort: 1, state: 'established', remoteAddress: '8.8.8.8' }),
      makeRow({ ownerPid: 1, localPort: 2, state: 'listen', remoteAddress: null }),
      makeRow({ ownerPid: 2, localPort: 3, state: 'established', remoteAddress: '10.0.0.1' }),
    ],
    {
      unknownLabel: 'Unknown',
      nameFor: (pid) => (pid === 1 ? 'server.exe' : 'client.exe'),
    },
  );

  it('narrows rows within a group', () => {
    const result = applySearch(groups, 'listening', '');
    expect(result).toHaveLength(1);
    expect(result[0]?.rows).toHaveLength(1);
  });

  it('drops a group whose rows all filtered out', () => {
    // Showing an application with zero visible connections tells the user
    // nothing and makes a narrow filter look like a broken one.
    const result = applySearch(groups, 'external', '');
    expect(result.map((group) => group.name)).toEqual(['server.exe']);
  });

  it('combines filter and query', () => {
    expect(applySearch(groups, 'established', 'client')).toHaveLength(1);
    expect(applySearch(groups, 'listening', 'client')).toHaveLength(0);
  });

  it('preserves the group summary counts, which describe the unfiltered set', () => {
    // The badge says how many public listeners the app has, not how many are
    // currently visible — a filter must not change what is true about the app.
    const result = applySearch(groups, 'established', '');
    expect(result[0]?.listening).toBe(1);
  });
});
