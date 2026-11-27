import { defaultLocale, isSupportedLocale, type Locale } from '@vitals/i18n';
import type {
  Channel as AlertChannel,
  ChannelKind,
  Comparator,
  Rule as AlertRule,
  RuleMetric,
  Severity as RuleSeverity,
} from '@vitals/protocol';

import { defaultRoute, isRouteId, type RouteId } from '../shell/navigation';
import { defaultTheme, isAccent, isDensity, isThemeMode } from '../theme/types';
import type { ThemeSettings } from '../theme/types';

export const samplingRates = ['fast', 'normal', 'slow'] as const;
export type SamplingRate = (typeof samplingRates)[number];

/**
 * What the tray icon shows.
 *
 * `off` draws the plain mark and costs nothing at all; every other value makes
 * the icon carry a reading. A fill rather than digits, because 32 px cannot
 * hold two legible digits at 100% scaling — the tooltip carries the numbers.
 */
export const trayMetrics = [
  'off',
  'cpu',
  'cpuTemperature',
  'memory',
  'network',
  'battery',
] as const;
export type TrayMetric = (typeof trayMetrics)[number];

function isTrayMetric(value: unknown): value is TrayMetric {
  return typeof value === 'string' && (trayMetrics as readonly string[]).includes(value);
}

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

/**
 * Where readings are published beyond this machine.
 *
 * The non-secret half of a broker: the switch, the address and the optional
 * username. The password is deliberately absent — it lives in the OS secret
 * store (`apps/desktop/src-tauri/src/mqtt.rs`), because this object is plain
 * JSON in the user's profile and a broker password there would be readable by
 * anything that can read the profile and would end up in a backup.
 */
export interface MqttSettings {
  /** The user's switch. Off by default; nothing is published while false. */
  readonly enabled: boolean;
  /** `mqtt://host[:port]` or `mqtts://host[:port]`. Empty until filled in. */
  readonly url: string;
  /** Empty means the broker is contacted without a username. */
  readonly username: string;
}

export interface AppSettings {
  readonly theme: ThemeSettings;
  readonly locale: Locale;

  readonly startWithWindows: boolean;
  readonly startMinimised: boolean;
  /** The × button hides the window instead of quitting. */
  readonly closeToTray: boolean;
  /**
   * Which reading the tray icon carries. Off by default: an icon that draws a
   * number nobody asked for is a cost, however small, for everyone.
   */
  readonly trayMetric: TrayMetric;
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
  /**
   * Rules the user wrote, evaluated by the Rust engine at the sampling rate
   * and shown in Settings > Alerts. The type is the wire type on purpose: the
   * engine consumes exactly what the screen stores, so there is nothing to
   * keep in step.
   */
  readonly alertRules: readonly AlertRule[];
  /**
   * Where alerts are sent beyond this machine. The wire type, like the rules:
   * Rust consumes exactly what the screen stores.
   *
   * Nothing is sent anywhere until a URL is written down, and a channel with
   * no URL is inert rather than an error — the screen saves as the user types.
   */
  readonly notificationChannels: readonly AlertChannel[];

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

  /** Publishing to an MQTT broker. Off until the user turns it on. */
  readonly mqtt: MqttSettings;
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
  trayMetric: 'off',
  taskbarLoad: true,
  confirmEndTask: true,
  hudVisible: false,

  samplingRate: 'normal',
  throttleWhenHidden: true,

  notificationsEnabled: false,
  notifyHighCpu: false,
  notifyHighMemory: false,
  notifyThermal: false,
  // No rules by default: a monitor that invents conditions for you is not
  // one you should trust with process-level visibility.
  alertRules: [],
  notificationChannels: [],

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
  // Off, with no address: publishing readings to a broker is something the
  // user asks for, and an empty address is the honest "not set up yet".
  mqtt: { enabled: false, url: '', username: '' },
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

/**
 * Keeps only well-formed MQTT settings.
 *
 * Total like the rest of this file: the settings are plain JSON a user can
 * edit, and one odd value must cost that one field, not the whole section. A
 * non-string address or username falls back to the default rather than
 * reaching the backend as something it would have to refuse.
 */
function parseMqtt(value: unknown): MqttSettings {
  if (typeof value !== 'object' || value === null) return defaultSettings.mqtt;
  const entry = value as Record<string, unknown>;
  return {
    enabled: entry['enabled'] === true,
    url: typeof entry['url'] === 'string' ? entry['url'] : defaultSettings.mqtt.url,
    username:
      typeof entry['username'] === 'string' ? entry['username'] : defaultSettings.mqtt.username,
  };
}

/** The values the engine accepts, spelled out so a hand-edited rule does not
 * reach Rust as something it would silently ignore. */
const ruleMetrics: readonly RuleMetric[] = [
  'cpuPercent',
  'cpuTemperature',
  'memoryPercent',
  'diskActivePercent',
  'batteryPercent',
  'networkErrorsPerSec',
];
const comparators: readonly Comparator[] = ['above', 'below'];
const ruleSeverities: readonly RuleSeverity[] = ['info', 'warning', 'critical'];

/** The longest a window may be, matching `MAX_RULE_WINDOW_SECS` in Rust. */
const MAX_RULE_WINDOW_SECS = 86_400;

/**
 * Keeps only well-formed rules.
 *
 * Deliberately total, like the rest of this file: the settings live in plain
 * JSON a user can edit, and one broken rule must cost that rule, not every
 * other one.
 */
function parseRules(value: unknown): readonly AlertRule[] {
  if (!Array.isArray(value)) return [];
  const out: AlertRule[] = [];
  for (const raw of value) {
    if (typeof raw !== 'object' || raw === null) continue;
    const entry = raw as Record<string, unknown>;
    const metric = ruleMetrics.find((candidate) => candidate === entry['metric']);
    const comparator = comparators.find((candidate) => candidate === entry['comparator']);
    const severity = ruleSeverities.find((candidate) => candidate === entry['severity']);
    if (
      typeof entry['id'] !== 'string' ||
      typeof entry['name'] !== 'string' ||
      metric === undefined ||
      comparator === undefined ||
      severity === undefined ||
      typeof entry['threshold'] !== 'number' ||
      !Number.isFinite(entry['threshold'])
    ) {
      continue;
    }
    out.push({
      id: entry['id'],
      name: entry['name'],
      metric,
      comparator,
      threshold: entry['threshold'],
      sustainSecs: isInt(entry['sustainSecs'], 0, MAX_RULE_WINDOW_SECS) ? entry['sustainSecs'] : 0,
      cooldownSecs: isInt(entry['cooldownSecs'], 0, MAX_RULE_WINDOW_SECS)
        ? entry['cooldownSecs']
        : 0,
      severity,
      channels: Array.isArray(entry['channels'])
        ? (entry['channels'] as unknown[]).filter((id): id is string => typeof id === 'string')
        : [],
    });
  }
  return out;
}

const channelKinds: readonly ChannelKind[] = ['webhook', 'ntfy', 'discord', 'homeAssistant'];

/**
 * Keeps only well-formed channels.
 *
 * Total, like the rule parser and for the same reason: the settings are plain
 * JSON a user can edit, and a broken channel must cost that channel rather
 * than the whole panel. A URL is deliberately *not* validated here — a
 * half-typed one is normal while the user is typing, and the engine treats it
 * as inert.
 */
function parseChannels(value: unknown): readonly AlertChannel[] {
  if (!Array.isArray(value)) return [];
  const out: AlertChannel[] = [];
  for (const raw of value) {
    if (typeof raw !== 'object' || raw === null) continue;
    const entry = raw as Record<string, unknown>;
    const kind = channelKinds.find((candidate) => candidate === entry['kind']);
    const minSeverity = ruleSeverities.find((candidate) => candidate === entry['minSeverity']);
    if (
      typeof entry['id'] !== 'string' ||
      typeof entry['name'] !== 'string' ||
      kind === undefined ||
      minSeverity === undefined
    ) {
      continue;
    }
    out.push({
      id: entry['id'],
      name: entry['name'],
      kind,
      url: typeof entry['url'] === 'string' ? entry['url'] : '',
      enabled: entry['enabled'] === true,
      secret: typeof entry['secret'] === 'string' ? entry['secret'] : null,
      minSeverity,
    });
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
    trayMetric: pick(record['trayMetric'], isTrayMetric, defaultSettings.trayMetric),
    alertRules: parseRules(record['alertRules']),
    notificationChannels: parseChannels(record['notificationChannels']),
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
    mqtt: parseMqtt(record['mqtt']),
  };
}
