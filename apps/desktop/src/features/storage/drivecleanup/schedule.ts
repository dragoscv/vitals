/**
 * The weekly safe clean-up: what is kept in settings and pushed to Rust.
 *
 * # The backend decides what runs
 *
 * A schedule carries the drive's saved choices, but the backend keeps only
 * the items it considers safe to run with nobody watching
 * (`drive_cleanup::schedule`): never anything elevated, never a compaction,
 * never WSL or Docker stopped, never a danger item. `schedulableKinds`
 * mirrors that list so the wizard can say which choices the schedule
 * includes; contract.test.ts keeps the two equal.
 *
 * # The choices are the profile's
 *
 * A schedule stores when, not what: the kinds are the drive's saved
 * profile at the moment they are pushed, so changing the choices in the
 * wizard changes the weekly clean-up too. A drive with no profile sends
 * every schedulable kind and the backend keeps its safe defaults.
 *
 * # Keyed by volume, not letter
 *
 * The key is the volume's GUID path (`\\?\Volume{…}\`) the backend reports,
 * so a USB disk that comes back under another letter keeps its schedule.
 * `mount` is the letter last seen, for display.
 *
 * Small and free of React and the storage strings on purpose: the shell
 * loads it with a dynamic import to push the schedules at start, before the
 * Storage screen has ever loaded, and it must not drag that screen in. The
 * settings type and parser live in `settings/schema.ts`, which is in the
 * first paint; this module is not.
 */

import { hasTauriHost } from '../../../shell/host';
import type { DriveSchedules } from '../../../settings/schema';

/** `SCHEDULABLE_KINDS` in `drive_cleanup/schedule.rs`. */
export const schedulableKinds = [
  'tempOld',
  'npmCache',
  'pnpmCache',
  'nugetCache',
  'dockerBuildCache',
  'dockerOldImages',
  'wslAptCache',
  'wslJournal',
  'wslTmp',
  'wslDockerPrune',
] as const;

export type { DriveSchedule, DriveSchedules } from '../../../settings/schema';
export { parseSchedules } from '../../../settings/schema';

/** Sunday at 03:00 unless the user picks otherwise. */
export const DEFAULT_WEEKDAY = 6;
export const DEFAULT_MINUTE = 3 * 60;

/** The choices the weekly clean-up will actually run. */
export function scheduledKinds(kinds: readonly string[]): readonly string[] {
  const allowed = new Set<string>(schedulableKinds);
  return kinds.filter((kind) => allowed.has(kind));
}

/** `ScheduleDto` in `drive_schedule.rs`. */
export function toWire(
  schedules: DriveSchedules,
  profiles: Readonly<Record<string, readonly string[]>>,
) {
  return Object.entries(schedules).map(([volume, s]) => ({
    volume,
    mount: s.mount,
    weekday: s.weekday,
    minute: s.minute,
    kinds: [...(profiles[s.mount] ?? schedulableKinds)],
    enabledAt: s.enabledAt,
  }));
}

/** Hands the background thread the schedules. Failures keep the last set. */
export async function pushDriveSchedules(
  schedules: DriveSchedules,
  profiles: Readonly<Record<string, readonly string[]>>,
): Promise<void> {
  if (!hasTauriHost()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_drive_schedules', { schedules: toWire(schedules, profiles) });
  } catch {
    // The previous schedules stay in force; never worth an unhandled rejection.
  }
}

export interface ScheduleStrings {
  readonly title: string;
  /** With `{mount}` and `{size}`, filled in by Rust. */
  readonly freed: string;
  /** With `{mount}`: the gain could not be measured. */
  readonly freedUnknown: string;
}

/**
 * The notification text. Not i18next: Rust fills `{mount}` and `{size}`
 * when it fires with the window hidden, and the shell pushes it before the
 * Storage strings (a lazy chunk) exist.
 */
export const scheduleToast: Readonly<Record<'en' | 'ro', ScheduleStrings>> = {
  en: {
    title: 'Vitals',
    freed: 'The weekly clean-up of {mount} freed {size}.',
    freedUnknown:
      'The weekly clean-up of {mount} finished; the space it freed could not be measured.',
  },
  ro: {
    title: 'Vitals',
    freed: 'Curățenia săptămânală a unității {mount} a eliberat {size}.',
    freedUnknown:
      'Curățenia săptămânală a unității {mount} s-a încheiat; spațiul eliberat nu a putut fi măsurat.',
  },
};

/** The toast text for `locale`, English for any other. */
export function toastFor(locale: string): ScheduleStrings {
  return locale.startsWith('ro') ? scheduleToast.ro : scheduleToast.en;
}

/** The decimal separator of `locale`, so the toast writes 4,20 GB in Romanian. */
export function decimalSeparator(locale: string): string {
  return (
    new Intl.NumberFormat(locale).formatToParts(1.5).find((part) => part.type === 'decimal')
      ?.value ?? '.'
  );
}

/** Hands Rust the toast text: it fires with the window hidden. */
export async function pushScheduleStrings(strings: ScheduleStrings, locale: string): Promise<void> {
  if (!hasTauriHost()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_drive_schedule_strings', {
      strings: { ...strings, decimal: decimalSeparator(locale) },
    });
  } catch {
    // As above.
  }
}

/** `HH:MM` for a minute of the day. */
export function minuteText(minute: number): string {
  const h = Math.floor(minute / 60);
  const m = minute % 60;
  return `${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}`;
}
