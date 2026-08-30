/**
 * Startup and services: the data shape, and what the numbers honestly mean.
 *
 * # Two screens, one fetch
 *
 * Startup entries and services come from the same Rust call because they
 * overlap: a service set to start automatically IS a startup item, and the
 * inventory already resolves that rather than making each screen re-derive it.
 * Splitting the fetch would mean reading the SCM twice.
 *
 * # "Unknown" is never rounded to a convenient answer
 *
 * Both enums carry an explicit unknown state, and both matter:
 *
 * - A startup entry whose `StartupApproved` record could not be read is
 *   `unknown`, not `enabled`. Reporting it as enabled is a fabricated fact,
 *   and it is precisely the fact the user would act on.
 * - A service whose start type could not be queried — the normal case when
 *   unelevated — is `unknown`, not `manual`. Guessing manual understates how
 *   much runs at boot, which is the number the whole screen exists to show.
 *
 * So counts here deliberately UNDERCOUNT rather than guess, and the UI is
 * expected to say how many it could not read.
 */

export type StartupSourceKey =
  | 'machineRun'
  | 'machineRun32'
  | 'machineRunOnce'
  | 'machineRunOnce32'
  | 'userRun'
  | 'userRunOnce'
  | 'commonStartupFolder'
  | 'userStartupFolder'
  | 'scheduledTask'
  | 'service';

export type StartupStateKey = 'enabled' | 'disabled' | 'unknown';

export type ServiceStateKey =
  | 'stopped'
  | 'startPending'
  | 'stopPending'
  | 'running'
  | 'continuePending'
  | 'pausePending'
  | 'paused'
  | 'unknown';

export type StartTypeKey = 'boot' | 'system' | 'automatic' | 'manual' | 'disabled' | 'unknown';

export interface StartupEntry {
  readonly name: string;
  readonly displayName: string | null;
  readonly command: string | null;
  readonly imagePath: string | null;
  readonly publisher: string | null;
  readonly source: StartupSourceKey;
  readonly state: StartupStateKey;
  readonly pid: number | null;
}

export interface ServiceEntry {
  readonly name: string;
  readonly displayName: string | null;
  readonly state: ServiceStateKey;
  readonly startType: StartTypeKey;
  readonly pid: number | null;
  readonly binaryPath: string | null;
  readonly svchostGroup: string | null;
}

export interface StartupSnapshot {
  readonly entries: readonly StartupEntry[];
  readonly services: readonly ServiceEntry[];
  readonly unreadableTasks: number;
}

/**
 * Sources that apply to every user of the machine.
 *
 * Drives the "affects all users" badge, and more usefully tells the UI that
 * changing the entry will need elevation — so the button can say so before
 * the user clicks and fails.
 */
const MACHINE_WIDE: ReadonlySet<StartupSourceKey> = new Set([
  'machineRun',
  'machineRun32',
  'machineRunOnce',
  'machineRunOnce32',
  'commonStartupFolder',
  'service',
]);

export function isMachineWide(entry: StartupEntry): boolean {
  return MACHINE_WIDE.has(entry.source);
}

/** The label to show: the friendly name when there is one, else the key. */
export function labelFor(entry: StartupEntry | ServiceEntry): string {
  return entry.displayName ?? entry.name;
}

export interface StartupCounts {
  readonly total: number;
  /** Entries that will definitely run at the next logon. */
  readonly enabled: number;
  readonly disabled: number;
  /**
   * Entries whose state could not be determined.
   *
   * Reported separately and never folded into either side. A screen that says
   * "18 enabled" when three were unreadable is claiming a precision it does
   * not have about the exact thing the user is trying to reduce.
   */
  readonly unknown: number;
}

export function countStartup(entries: readonly StartupEntry[]): StartupCounts {
  let enabled = 0;
  let disabled = 0;
  let unknown = 0;

  for (const entry of entries) {
    if (entry.state === 'enabled') enabled += 1;
    else if (entry.state === 'disabled') disabled += 1;
    else unknown += 1;
  }

  return { total: entries.length, enabled, disabled, unknown };
}

export interface ServiceCounts {
  readonly total: number;
  readonly running: number;
  readonly stopped: number;
  /** Configured to start at boot. Undercounts when start types are unreadable. */
  readonly automatic: number;
  /** Services whose start type could not be read, so `automatic` is a floor. */
  readonly unknownStartType: number;
}

export function countServices(services: readonly ServiceEntry[]): ServiceCounts {
  let running = 0;
  let stopped = 0;
  let automatic = 0;
  let unknownStartType = 0;

  for (const service of services) {
    if (service.state === 'running') running += 1;
    else if (service.state === 'stopped') stopped += 1;

    // Boot and System drivers start earlier than Automatic and are counted
    // with it: the question is "what starts without being asked", and a boot
    // driver certainly does.
    if (
      service.startType === 'automatic' ||
      service.startType === 'boot' ||
      service.startType === 'system'
    ) {
      automatic += 1;
    } else if (service.startType === 'unknown') {
      unknownStartType += 1;
    }
  }

  return { total: services.length, running, stopped, automatic, unknownStartType };
}

export const startupFilters = ['all', 'enabled', 'disabled'] as const;
export type StartupFilter = (typeof startupFilters)[number];

export const serviceFilters = ['all', 'running', 'stopped', 'automatic'] as const;
export type ServiceFilter = (typeof serviceFilters)[number];

export function filterStartup(
  entries: readonly StartupEntry[],
  filter: StartupFilter,
  query: string,
): readonly StartupEntry[] {
  const needle = query.trim().toLowerCase();

  return entries.filter((entry) => {
    if (filter === 'enabled' && entry.state !== 'enabled') return false;
    if (filter === 'disabled' && entry.state !== 'disabled') return false;
    if (needle === '') return true;

    return (
      labelFor(entry).toLowerCase().includes(needle) ||
      entry.name.toLowerCase().includes(needle) ||
      entry.command?.toLowerCase().includes(needle) === true ||
      entry.publisher?.toLowerCase().includes(needle) === true
    );
  });
}

export function filterServices(
  services: readonly ServiceEntry[],
  filter: ServiceFilter,
  query: string,
): readonly ServiceEntry[] {
  const needle = query.trim().toLowerCase();

  return services.filter((service) => {
    if (filter === 'running' && service.state !== 'running') return false;
    if (filter === 'stopped' && service.state !== 'stopped') return false;
    if (
      filter === 'automatic' &&
      service.startType !== 'automatic' &&
      service.startType !== 'boot' &&
      service.startType !== 'system'
    ) {
      return false;
    }
    if (needle === '') return true;

    return (
      labelFor(service).toLowerCase().includes(needle) ||
      service.name.toLowerCase().includes(needle) ||
      service.binaryPath?.toLowerCase().includes(needle) === true
    );
  });
}

/**
 * Sorts startup entries so the ones worth attention come first.
 *
 * Enabled before disabled, because the list exists to answer "what is slowing
 * my logon" and a disabled entry answers nothing. Unknown sits with enabled
 * rather than at the bottom — it *might* be running, and hiding a maybe at
 * the end of the list is how it never gets looked at.
 */
export function sortStartup(entries: readonly StartupEntry[]): readonly StartupEntry[] {
  const rank = (state: StartupStateKey): number =>
    state === 'enabled' ? 0 : state === 'unknown' ? 1 : 2;

  return [...entries].sort((a, b) => {
    const byState = rank(a.state) - rank(b.state);
    // Name breaks the tie so the order is stable between refreshes rather
    // than reflecting whatever order the registry enumerated.
    return byState !== 0 ? byState : labelFor(a).localeCompare(labelFor(b));
  });
}

/** Running services first, then alphabetical. */
export function sortServices(services: readonly ServiceEntry[]): readonly ServiceEntry[] {
  return [...services].sort((a, b) => {
    const byState = Number(b.state === 'running') - Number(a.state === 'running');
    return byState !== 0 ? byState : labelFor(a).localeCompare(labelFor(b));
  });
}
