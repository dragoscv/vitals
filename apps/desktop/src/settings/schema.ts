import { defaultLocale, isSupportedLocale, type Locale } from '@vitals/i18n';

import { defaultRoute, isRouteId, type RouteId } from '../shell/navigation';
import { defaultTheme, isAccent, isDensity, isSurface, isThemeMode } from '../theme/types';
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

export interface AppSettings {
  readonly theme: ThemeSettings;
  readonly locale: Locale;

  readonly startWithWindows: boolean;
  readonly startMinimised: boolean;
  readonly confirmEndTask: boolean;

  readonly samplingRate: SamplingRate;
  readonly throttleWhenHidden: boolean;

  readonly notificationsEnabled: boolean;
  readonly notifyHighCpu: boolean;
  readonly notifyHighMemory: boolean;
  readonly notifyThermal: boolean;

  readonly historyEnabled: boolean;
  readonly retentionDays: number;

  /** Shell layout. Persisted so the window reopens the way it was left. */
  readonly sidebarCollapsed: boolean;
  readonly lastRoute: RouteId;
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
  confirmEndTask: true,

  samplingRate: 'normal',
  throttleWhenHidden: true,

  notificationsEnabled: false,
  notifyHighCpu: false,
  notifyHighMemory: false,
  notifyThermal: false,

  historyEnabled: false,
  retentionDays: 7,

  sidebarCollapsed: false,
  lastRoute: defaultRoute,
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
    surface: pick(rawTheme['surface'], isSurface, defaultTheme.surface),
    density: pick(rawTheme['density'], isDensity, defaultTheme.density),
    reduceMotion: typeof reduceMotionRaw === 'boolean' ? reduceMotionRaw : null,
  };

  const retention = record['retentionDays'];

  return {
    theme,
    locale: pick(record['locale'], isSupportedLocale, defaultSettings.locale),

    startWithWindows: bool(record['startWithWindows'], defaultSettings.startWithWindows),
    startMinimised: bool(record['startMinimised'], defaultSettings.startMinimised),
    confirmEndTask: bool(record['confirmEndTask'], defaultSettings.confirmEndTask),

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
    retentionDays: (retentionDayOptions as readonly number[]).includes(retention as number)
      ? (retention as number)
      : defaultSettings.retentionDays,

    // `crashReports`, `usageData`, `reputationLookups` and `advancedEnabled`
    // were removed: no telemetry, lookup or advanced feature existed behind
    // them, so the switches promised behaviour that never happened. Old store
    // files carrying those keys are simply ignored here.

    sidebarCollapsed: bool(record['sidebarCollapsed'], defaultSettings.sidebarCollapsed),
    lastRoute: isRouteId(record['lastRoute']) ? record['lastRoute'] : defaultSettings.lastRoute,
  };
}
