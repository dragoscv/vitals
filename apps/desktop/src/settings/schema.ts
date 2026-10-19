import { defaultLocale, isSupportedLocale, type Locale } from '@vitals/i18n';

import { defaultRoute, isRouteId, type RouteId } from '../shell/navigation';
import { defaultTheme, isAccent, isDensity, isThemeMode } from '../theme/types';
import type { ThemeSettings } from '../theme/types';

export const samplingRates = ['fast', 'normal', 'slow'] as const;
export type SamplingRate = (typeof samplingRates)[number];

/** Milliseconds between samples. The UI never speaks in numbers; this does. */
export const samplingIntervalMs: Readonly<Record<SamplingRate, number>> = {
  fast: 500,
  normal: 1000,
  slow: 2000,
};

export const retentionDayOptions = [1, 7, 30, 90] as const;

/**
 * One drive's weekly clean-up (`features/storage/drivecleanup/schedule.ts`).
 * Declared here so the settings module, which loads with the first paint,
 * does not pull the clean-up code in with it.
 */
export interface DriveSchedule {
  /** The drive letter last seen, `C:`. */
  readonly mount: string;
  /** 0 = Monday … 6 = Sunday. */
  readonly weekday: number;
  /** Minute of the local day, 0–1439. */
  readonly minute: number;
  /** When it was turned on, in unix seconds: an earlier slot is not due. */
  readonly enabledAt: number;
}

export type DriveSchedules = Readonly<Record<string, DriveSchedule>>;

export interface AppSettings {
  readonly theme: ThemeSettings;
  readonly locale: Locale;

  readonly startWithWindows: boolean;
  readonly startMinimised: boolean;
  /** The × button hides the window instead of quitting. */
  readonly closeToTray: boolean;
  /** CPU load as the taskbar button's progress fill while the window is open. */
  readonly taskbarLoad: boolean;
  readonly confirmEndTask: boolean;
  /** Whether the always-on-top overlay is showing. Re-opened on launch. */
  readonly hudVisible: boolean;

  readonly samplingRate: SamplingRate;
  readonly throttleWhenHidden: boolean;

  readonly notificationsEnabled: boolean;
  readonly notifyHighCpu: boolean;
  readonly notifyHighMemory: boolean;
  readonly notifyThermal: boolean;

  readonly historyEnabled: boolean;
  readonly retentionDays: number;

  /** Check GitHub for a new version after launch, install it on quit. */
  readonly autoUpdate: boolean;

  /** Shell layout. Persisted so the window reopens the way it was left. */
  readonly sidebarCollapsed: boolean;
  readonly lastRoute: RouteId;

  /**
   * Devices hidden or re-shown on the Performance page, keyed by a name that
   * survives a reboot. Only overrides of the defaults are stored.
   */
  readonly resourceVisibility: Readonly<Record<string, 'hidden' | 'shown'>>;
  /** The Performance page's "show hidden devices" switch. */
  readonly showHiddenResources: boolean;
  /**
   * The dashboard's first-run card was dismissed. Persisted here because it
   * is a per-user preference like any other, and a card that comes back on
   * every launch is nagging, not onboarding.
   */
  readonly firstRunDismissed: boolean;
  /**
   * "Remember my choices for this drive" in the drive clean-up wizard: the
   * item KINDS last chosen, keyed by mount (`C:`). Kinds rather than ids,
   * because ids belong to one analysis and mean nothing the next time.
   */
  readonly driveCleanupProfiles: Readonly<Record<string, readonly string[]>>;
  /**
   * The weekly safe clean-up, per drive, keyed by the volume's GUID path so a
   * changed drive letter keeps it. Absent = off, the default.
   */
  readonly driveCleanupSchedules: DriveSchedules;
}

/**
 * Defaults.
 *
 * Every switch that sends data anywhere, records to disk, or touches hardware
 * is off. A system monitor that opts you in to telemetry is not one you should
 * trust with process-level visibility into your machine.
 */
export const defaultSettings: AppSettings = {
  theme: defaultTheme,
  locale: defaultLocale,

  startWithWindows: false,
  startMinimised: false,
  // On, unlike almost everything else here, because the alternative is worse:
  // a monitor closed by reflex stops monitoring with no warning. It records
  // nothing and sends nothing, so the "everything off" promise is untouched.
  closeToTray: true,
  taskbarLoad: true,
  confirmEndTask: true,
  hudVisible: false,

  samplingRate: 'normal',
  throttleWhenHidden: true,

  notificationsEnabled: false,
  notifyHighCpu: false,
  notifyHighMemory: false,
  notifyThermal: false,

  historyEnabled: false,
  retentionDays: 7,

  // On, the one switch here that makes a network request by default. The
  // request goes to GitHub, carries no data about the machine beyond what any
  // download does (address, app version), and an installed monitor that never
  // receives its security fixes is the worse failure. The privacy policy says
  // so, and the switch is in Settings → About.
  autoUpdate: true,

  sidebarCollapsed: false,
  lastRoute: defaultRoute,

  resourceVisibility: {},
  showHiddenResources: false,
  firstRunDismissed: false,
  driveCleanupProfiles: {},
  // Off for every drive: deleting on a timer is something the user asks for.
  driveCleanupSchedules: {},
};

function bool(value: unknown, fallback: boolean): boolean {
  return typeof value === 'boolean' ? value : fallback;
}

function pick<T extends string>(
  value: unknown,
  guard: (candidate: string) => candidate is T,
  fallback: T,
): T {
  return typeof value === 'string' && guard(value) ? value : fallback;
}

function isSamplingRate(value: string): value is SamplingRate {
  return (samplingRates as readonly string[]).includes(value);
}

/**
 * Keeps only well-formed entries: a hand-edited value other than the two
 * choices costs that one device its override, not the whole map.
 */
function parseVisibility(value: unknown): Readonly<Record<string, 'hidden' | 'shown'>> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return {};
  const out: Record<string, 'hidden' | 'shown'> = {};
  for (const [key, choice] of Object.entries(value as Record<string, unknown>)) {
    if (choice === 'hidden' || choice === 'shown') out[key] = choice;
  }
  return out;
}

/**
 * Keeps only well-formed profiles: a hand-edited entry that is not a list of
 * strings costs that one drive its profile, not every drive's.
 */
function isInt(value: unknown, min: number, max: number): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= min && value <= max;
}

/**
 * Keeps only well-formed schedules: a hand-edited entry costs that one
 * drive its schedule, not every drive's.
 */
export function parseSchedules(value: unknown): DriveSchedules {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return {};
  const out: Record<string, DriveSchedule> = {};
  for (const [volume, raw] of Object.entries(value as Record<string, unknown>)) {
    if (typeof raw !== 'object' || raw === null) continue;
    const entry = raw as Record<string, unknown>;
    if (
      volume !== '' &&
      typeof entry['mount'] === 'string' &&
      isInt(entry['weekday'], 0, 6) &&
      isInt(entry['minute'], 0, 1439) &&
      isInt(entry['enabledAt'], 0, Number.MAX_SAFE_INTEGER)
    ) {
      out[volume] = {
        mount: entry['mount'],
        weekday: entry['weekday'],
        minute: entry['minute'],
        enabledAt: entry['enabledAt'],
      };
    }
  }
  return out;
}

function parseProfiles(value: unknown): Readonly<Record<string, readonly string[]>> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return {};
  const out: Record<string, readonly string[]> = {};
  for (const [mount, kinds] of Object.entries(value as Record<string, unknown>)) {
    if (Array.isArray(kinds) && kinds.every((kind) => typeof kind === 'string')) {
      out[mount] = kinds;
    }
  }
  return out;
}

/**
 * Rebuilds settings from whatever is on disk.
 *
 * Deliberately total: it accepts `unknown` and always returns a complete,
 * valid object. The store file is plain JSON in the user's roaming profile,
 * survives upgrades that rename or remove options, and is trivially editable
 * by hand — so a single unexpected value must degrade to the default for that
 * one field rather than take the whole window down before it can paint.
 */
export function parseSettings(raw: unknown): AppSettings {
  if (typeof raw !== 'object' || raw === null) return defaultSettings;
  const record = raw as Record<string, unknown>;

  const rawTheme =
    typeof record['theme'] === 'object' && record['theme'] !== null
      ? (record['theme'] as Record<string, unknown>)
      : {};

  const reduceMotionRaw = rawTheme['reduceMotion'];

  const theme: ThemeSettings = {
    mode: pick(rawTheme['mode'], isThemeMode, defaultTheme.mode),
    accent: pick(rawTheme['accent'], isAccent, defaultTheme.accent),
    density: pick(rawTheme['density'], isDensity, defaultTheme.density),
    reduceMotion: typeof reduceMotionRaw === 'boolean' ? reduceMotionRaw : null,
  };

  const retention = record['retentionDays'];

  return {
    theme,
    locale: pick(record['locale'], isSupportedLocale, defaultSettings.locale),

    startWithWindows: bool(record['startWithWindows'], defaultSettings.startWithWindows),
    startMinimised: bool(record['startMinimised'], defaultSettings.startMinimised),
    closeToTray: bool(record['closeToTray'], defaultSettings.closeToTray),
    taskbarLoad: bool(record['taskbarLoad'], defaultSettings.taskbarLoad),
    confirmEndTask: bool(record['confirmEndTask'], defaultSettings.confirmEndTask),
    hudVisible: bool(record['hudVisible'], defaultSettings.hudVisible),

    samplingRate: pick(record['samplingRate'], isSamplingRate, defaultSettings.samplingRate),
    throttleWhenHidden: bool(record['throttleWhenHidden'], defaultSettings.throttleWhenHidden),

    notificationsEnabled: bool(
      record['notificationsEnabled'],
      defaultSettings.notificationsEnabled,
    ),
    notifyHighCpu: bool(record['notifyHighCpu'], defaultSettings.notifyHighCpu),
    notifyHighMemory: bool(record['notifyHighMemory'], defaultSettings.notifyHighMemory),
    notifyThermal: bool(record['notifyThermal'], defaultSettings.notifyThermal),

    historyEnabled: bool(record['historyEnabled'], defaultSettings.historyEnabled),
    autoUpdate: bool(record['autoUpdate'], defaultSettings.autoUpdate),
    retentionDays: (retentionDayOptions as readonly number[]).includes(retention as number)
      ? (retention as number)
      : defaultSettings.retentionDays,

    // `crashReports`, `usageData`, `reputationLookups` and `advancedEnabled`
    // were removed: no telemetry, lookup or advanced feature existed behind
    // them, so the switches promised behaviour that never happened. Old store
    // files carrying those keys are simply ignored here.

    sidebarCollapsed: bool(record['sidebarCollapsed'], defaultSettings.sidebarCollapsed),
    lastRoute: isRouteId(record['lastRoute']) ? record['lastRoute'] : defaultSettings.lastRoute,

    resourceVisibility: parseVisibility(record['resourceVisibility']),
    showHiddenResources: bool(record['showHiddenResources'], defaultSettings.showHiddenResources),
    firstRunDismissed: bool(record['firstRunDismissed'], defaultSettings.firstRunDismissed),
    driveCleanupProfiles: parseProfiles(record['driveCleanupProfiles']),
    driveCleanupSchedules: parseSchedules(record['driveCleanupSchedules']),
  };
}
