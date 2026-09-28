import { describe, expect, it } from 'vitest';

import {
  countServices,
  countStartup,
  filterServices,
  filterStartup,
  formatCpuSeconds,
  isMachineWide,
  labelFor,
  sortServices,
  sortStartup,
  type ServiceEntry,
  type StartupEntry,
} from './model';

function entry(overrides: Partial<StartupEntry> = {}): StartupEntry {
  return {
    name: 'Thing',
    displayName: null,
    command: 'C:\\Program Files\\Thing\\thing.exe',
    imagePath: null,
    publisher: null,
    source: 'userRun',
    state: 'enabled',
    pid: null,
    impact: null,
    company: null,
    microsoft: false,
    risk: 'safe',
    ...overrides,
  };
}

function service(overrides: Partial<ServiceEntry> = {}): ServiceEntry {
  return {
    name: 'Spooler',
    displayName: 'Print Spooler',
    state: 'running',
    startType: 'automatic',
    pid: 1234,
    binaryPath: null,
    svchostGroup: null,
    company: null,
    microsoft: false,
    risk: 'safe',
    imagePath: null,
    ...overrides,
  };
}

describe('formatCpuSeconds', () => {
  it('renders an unmeasured value as an em dash, never as zero seconds', () => {
    // The distinction the whole feature rests on: "0.0 s" claims the item
    // cost nothing at boot; "—" says it was not measured.
    expect(formatCpuSeconds(null, 'en')).toBe('—');
    expect(formatCpuSeconds(0, 'en')).toBe('0.0 s');
  });

  it('shows milliseconds as seconds with one decimal in the locale', () => {
    expect(formatCpuSeconds(4213, 'en')).toBe('4.2 s');
    expect(formatCpuSeconds(4213, 'ro')).toBe('4,2 s');
  });
});

describe('labelFor', () => {
  it('prefers the friendly name', () => {
    expect(labelFor(entry({ name: 'Spooler', displayName: 'Print Spooler' }))).toBe(
      'Print Spooler',
    );
  });

  it('falls back to the key rather than showing nothing', () => {
    // The SCM returns an empty display name for services whose registry
    // `DisplayName` was never set. A blank row would be indistinguishable
    // from a rendering failure.
    expect(labelFor(entry({ name: 'Spooler', displayName: null }))).toBe('Spooler');
  });
});

describe('isMachineWide', () => {
  it('recognises the machine-scoped sources', () => {
    // Drives the "all users" badge, and more usefully tells the UI that
    // changing the entry will need elevation.
    for (const source of [
      'machineRun',
      'machineRunOnce32',
      'commonStartupFolder',
      'service',
    ] as const) {
      expect(isMachineWide(entry({ source })), source).toBe(true);
    }
  });

  it('does not claim a per-user entry affects everyone', () => {
    for (const source of ['userRun', 'userRunOnce', 'userStartupFolder'] as const) {
      expect(isMachineWide(entry({ source })), source).toBe(false);
    }
  });
});

describe('countStartup', () => {
  it('never folds unknown into enabled', () => {
    // The central honesty rule. A permission failure reading `StartupApproved`
    // rendered as "this will run" is a fabricated fact, and it is exactly the
    // fact the user would act on when deciding what to disable.
    const counts = countStartup([
      entry({ state: 'enabled' }),
      entry({ state: 'unknown' }),
      entry({ state: 'unknown' }),
      entry({ state: 'disabled' }),
    ]);

    expect(counts).toEqual({ total: 4, enabled: 1, disabled: 1, unknown: 2 });
  });

  it('handles an empty inventory', () => {
    expect(countStartup([])).toEqual({ total: 0, enabled: 0, disabled: 0, unknown: 0 });
  });
});

describe('countServices', () => {
  it('counts boot and system drivers as starting at boot', () => {
    // The question is "what starts without being asked", and a boot driver
    // certainly does. Counting only `automatic` would understate it.
    const counts = countServices([
      service({ startType: 'boot' }),
      service({ startType: 'system' }),
      service({ startType: 'automatic' }),
      service({ startType: 'manual' }),
    ]);

    expect(counts.automatic).toBe(3);
  });

  it('reports unreadable start types separately, never as manual', () => {
    // Unelevated this is the normal case. Guessing manual would understate
    // how much runs at boot — the number the whole screen exists to show.
    const counts = countServices([
      service({ startType: 'unknown' }),
      service({ startType: 'unknown' }),
      service({ startType: 'automatic' }),
    ]);

    expect(counts.automatic).toBe(1);
    expect(counts.unknownStartType).toBe(2);
  });

  it('counts running and stopped without inventing the pending states', () => {
    // A starting service is neither running nor stopped, and forcing it into
    // one would make the two figures disagree with the total.
    const counts = countServices([
      service({ state: 'running' }),
      service({ state: 'stopped' }),
      service({ state: 'startPending' }),
    ]);

    expect(counts.running).toBe(1);
    expect(counts.stopped).toBe(1);
    expect(counts.total).toBe(3);
  });
});

describe('filterStartup', () => {
  const entries = [
    entry({ name: 'Steam', state: 'enabled', publisher: 'Valve' }),
    entry({ name: 'OneDrive', state: 'disabled' }),
    entry({ name: 'Mystery', state: 'unknown' }),
  ];

  it('excludes unknown from both enabled and disabled', () => {
    // The consequence of not folding: a filter must not silently include an
    // entry whose state it does not know.
    expect(filterStartup(entries, 'enabled', '')).toHaveLength(1);
    expect(filterStartup(entries, 'disabled', '')).toHaveLength(1);
    expect(filterStartup(entries, 'all', '')).toHaveLength(3);
  });

  it('searches the command line, not just the name', () => {
    // Where an entry launches from is often the only clue to what it is.
    const rows = filterStartup(
      [entry({ name: 'Opaque', command: 'C:\\Weird\\hidden.exe' })],
      'all',
      'hidden',
    );
    expect(rows).toHaveLength(1);
  });

  it('searches the publisher', () => {
    expect(filterStartup(entries, 'all', 'valve')).toHaveLength(1);
  });

  it('is case-insensitive and ignores surrounding space', () => {
    expect(filterStartup(entries, 'all', '  STEAM ')).toHaveLength(1);
  });
});

describe('filterServices', () => {
  const services = [
    service({ name: 'A', state: 'running', startType: 'automatic' }),
    service({ name: 'B', state: 'stopped', startType: 'manual' }),
    service({ name: 'C', state: 'stopped', startType: 'boot' }),
  ];

  it('treats boot and system as starts-at-boot', () => {
    const rows = filterServices(services, 'automatic', '');
    expect(rows.map((row) => row.name)).toEqual(['A', 'C']);
  });

  it('narrows by run state', () => {
    expect(filterServices(services, 'running', '')).toHaveLength(1);
    expect(filterServices(services, 'stopped', '')).toHaveLength(2);
  });

  it('searches the binary path', () => {
    const rows = filterServices(
      [service({ name: 'X', binaryPath: 'C:\\Windows\\System32\\svchost.exe -k netsvcs' })],
      'all',
      'netsvcs',
    );
    expect(rows).toHaveLength(1);
  });
});

describe('sortStartup', () => {
  it('puts unknown with enabled, not at the bottom', () => {
    // An unknown entry MIGHT be running. Burying a maybe at the end of the
    // list is how it never gets looked at.
    const sorted = sortStartup([
      entry({ name: 'Disabled', state: 'disabled' }),
      entry({ name: 'Unknown', state: 'unknown' }),
      entry({ name: 'Enabled', state: 'enabled' }),
    ]);

    expect(sorted.map((row) => row.name)).toEqual(['Enabled', 'Unknown', 'Disabled']);
  });

  it('breaks ties on name so the order is stable between refreshes', () => {
    // Otherwise the list reflects whatever order the registry enumerated,
    // which can differ between reads.
    const sorted = sortStartup([
      entry({ name: 'Zeta', state: 'enabled' }),
      entry({ name: 'Alpha', state: 'enabled' }),
    ]);

    expect(sorted.map((row) => row.name)).toEqual(['Alpha', 'Zeta']);
  });

  it('does not mutate its input', () => {
    const input = [entry({ name: 'B' }), entry({ name: 'A' })];
    sortStartup(input);
    expect(input.map((row) => row.name)).toEqual(['B', 'A']);
  });
});

describe('sortServices', () => {
  it('puts running services first', () => {
    const sorted = sortServices([
      service({ name: 'Stopped', state: 'stopped' }),
      service({ name: 'Running', state: 'running' }),
    ]);

    expect(sorted.map((row) => row.name)).toEqual(['Running', 'Stopped']);
  });

  it('sorts by display name, which is what the user reads', () => {
    const sorted = sortServices([
      service({ name: 'zzz', displayName: 'Alpha', state: 'running' }),
      service({ name: 'aaa', displayName: 'Beta', state: 'running' }),
    ]);

    expect(sorted.map((row) => row.displayName)).toEqual(['Alpha', 'Beta']);
  });
});
