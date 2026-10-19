import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { describeSchedule, type TaskSchedule } from './schedule';
import { STARTUP_NS, registerStartupStrings } from './strings';

beforeAll(async () => {
  await initI18n();
  registerStartupStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function schedule(overrides: Partial<TaskSchedule> = {}): TaskSchedule {
  return {
    atLogon: false,
    atBoot: false,
    repeatEverySecs: null,
    calendar: null,
    other: [],
    ...overrides,
  };
}

function describe_(value: TaskSchedule | null, locale = 'en'): string {
  return describeSchedule(value, i18n.getFixedT(locale, STARTUP_NS), locale);
}

describe('describeSchedule', () => {
  it('says nothing for an entry without a schedule rather than inventing one', () => {
    expect(describe_(null)).toBe('');
  });

  it('says nothing for a schedule whose triggers were all disabled', () => {
    expect(describe_(schedule())).toBe('');
  });

  it('names the logon and boot triggers', () => {
    expect(describe_(schedule({ atLogon: true }))).toBe('At sign-in');
    expect(describe_(schedule({ atBoot: true }))).toBe('At boot');
  });

  it('adds the repetition after the sign-in trigger in lower case', () => {
    expect(describe_(schedule({ atLogon: true, repeatEverySecs: 300 }))).toBe(
      'At sign-in · every 5 min',
    );
  });

  it('capitalises the repetition when it opens the line', () => {
    expect(describe_(schedule({ repeatEverySecs: 1_800 }))).toBe('Every 30 min');
  });

  it('picks the largest whole unit for an interval', () => {
    expect(describe_(schedule({ repeatEverySecs: 3_600 }))).toBe('Every 1 hour');
    expect(describe_(schedule({ repeatEverySecs: 7_200 }))).toBe('Every 2 hours');
    expect(describe_(schedule({ repeatEverySecs: 172_800 }))).toBe('Every 2 days');
    expect(describe_(schedule({ repeatEverySecs: 12_600 }))).toBe('Every 210 min');
    expect(describe_(schedule({ repeatEverySecs: 90 }))).toBe('Every 90 s');
  });

  it('never prints a zero or negative interval', () => {
    expect(describe_(schedule({ atBoot: true, repeatEverySecs: 0 }))).toBe('At boot');
    expect(describe_(schedule({ atBoot: true, repeatEverySecs: -60 }))).toBe('At boot');
    expect(describe_(schedule({ repeatEverySecs: Number.NaN }))).toBe('');
  });

  it('words calendar schedules with and without an interval', () => {
    expect(describe_(schedule({ calendar: { kind: 'daily', interval: 1 } }))).toBe('Daily');
    expect(describe_(schedule({ calendar: { kind: 'daily', interval: null } }))).toBe('Daily');
    expect(describe_(schedule({ calendar: { kind: 'daily', interval: 2 } }))).toBe('Every 2 days');
    expect(describe_(schedule({ calendar: { kind: 'weekly', interval: 1 } }))).toBe('Weekly');
    expect(describe_(schedule({ calendar: { kind: 'weekly', interval: 3 } }))).toBe(
      'Every 3 weeks',
    );
    expect(describe_(schedule({ calendar: { kind: 'monthly', interval: null } }))).toBe('Monthly');
  });

  it('lists the other trigger kinds in a fixed order after the boot trigger', () => {
    expect(
      describe_(
        schedule({
          atBoot: true,
          calendar: { kind: 'daily', interval: 1 },
          other: ['idle', 'event', 'once', 'sessionChange', 'registration'],
        }),
      ),
    ).toBe(
      'At boot · daily · when idle · on an event · once · on session change · when registered',
    );
  });

  it('uses the Romanian plural forms for one, few and many', () => {
    expect(describe_(schedule({ repeatEverySecs: 60 }), 'ro')).toBe('La fiecare 1 minut');
    expect(describe_(schedule({ repeatEverySecs: 300 }), 'ro')).toBe('La fiecare 5 minute');
    expect(describe_(schedule({ repeatEverySecs: 1_200 }), 'ro')).toBe('La fiecare 20 de minute');
    expect(describe_(schedule({ repeatEverySecs: 3 * 3_600 }), 'ro')).toBe('La fiecare 3 ore');
    expect(describe_(schedule({ atLogon: true, repeatEverySecs: 300 }), 'ro')).toBe(
      'La conectare · la fiecare 5 minute',
    );
    expect(
      describe_(schedule({ atBoot: true, calendar: { kind: 'weekly', interval: 1 } }), 'ro'),
    ).toBe('La pornire · săptămânal');
  });
});
