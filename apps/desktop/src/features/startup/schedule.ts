/**
 * How often a scheduled task runs, in words.
 *
 * The backend sends the triggers as data (see `TaskScheduleDto` in
 * `inventory.rs`) rather than prose, so the wording follows the user's
 * language and the table and the export say exactly the same thing.
 *
 * Nothing here invents a schedule: a task whose triggers could not be read,
 * or are all disabled, produces an empty string and the row shows nothing —
 * not "Unknown", which would look like a schedule called Unknown.
 */

import type { TFunction } from 'i18next';

export type ScheduleOtherKey = 'idle' | 'event' | 'once' | 'sessionChange' | 'registration';

export interface TaskCalendar {
  readonly kind: 'daily' | 'weekly' | 'monthly';
  /** Days for `daily`, weeks for `weekly`; `null` when unread or monthly. */
  readonly interval: number | null;
}

export interface TaskSchedule {
  readonly atLogon: boolean;
  readonly atBoot: boolean;
  /** `null` when nothing repeats or the interval could not be read; never 0. */
  readonly repeatEverySecs: number | null;
  readonly calendar: TaskCalendar | null;
  readonly other: readonly ScheduleOtherKey[];
}

const MINUTE = 60;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/**
 * Seconds as the largest whole unit, so 3600 reads "every 1 hour" rather than
 * "every 60 min". A value that is not a whole number of any larger unit stays
 * in the smaller one: 90 s is "every 90 s", never a rounded "every 2 min".
 */
function describeInterval(seconds: number, t: TFunction): string {
  if (seconds % DAY === 0) return t('schedule.everyDays', { count: seconds / DAY });
  if (seconds % HOUR === 0) return t('schedule.everyHours', { count: seconds / HOUR });
  if (seconds % MINUTE === 0) return t('schedule.everyMinutes', { count: seconds / MINUTE });
  return t('schedule.everySeconds', { count: seconds });
}

function describeCalendar(calendar: TaskCalendar, t: TFunction): string {
  const interval = calendar.interval;
  switch (calendar.kind) {
    case 'daily':
      return interval !== null && interval > 1
        ? t('schedule.everyNDays', { count: interval })
        : t('schedule.daily');
    case 'weekly':
      return interval !== null && interval > 1
        ? t('schedule.everyNWeeks', { count: interval })
        : t('schedule.weekly');
    case 'monthly':
      return t('schedule.monthly');
  }
}

/**
 * The schedule as one line, e.g. "At sign-in · every 5 min".
 *
 * Order is fixed — what starts it, how often it repeats, the calendar, then
 * the rest — so two rows with the same triggers read identically however the
 * task author ordered them.
 *
 * Every fragment is translated in lower case and only the line's first
 * letter is raised: any fragment can open the line (a task whose logon
 * trigger is disabled starts with its repetition), and a capital in the
 * middle of "At boot · Daily" reads as two sentences.
 */
export function describeSchedule(
  schedule: TaskSchedule | null,
  t: TFunction,
  locale?: string,
): string {
  if (schedule === null) return '';

  const parts: string[] = [];
  if (schedule.atLogon) parts.push(t('schedule.atLogon'));
  if (schedule.atBoot) parts.push(t('schedule.atBoot'));
  // A non-positive or non-finite interval is not something the backend
  // sends; refusing it here keeps "every 0 min" off the screen regardless.
  const every = schedule.repeatEverySecs;
  if (every !== null && Number.isFinite(every) && every > 0) {
    parts.push(describeInterval(every, t));
  }
  if (schedule.calendar !== null) parts.push(describeCalendar(schedule.calendar, t));
  for (const other of schedule.other) parts.push(t(`schedule.${other}`));

  const line = parts.join(t('schedule.joiner'));
  return line.charAt(0).toLocaleUpperCase(locale) + line.slice(1);
}
