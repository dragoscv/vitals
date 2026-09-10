/**
 * The drift guard between the Rust engine and the locale files.
 *
 * Nothing checks this at compile time: `crates/vitals-core/src/alerts.rs`
 * builds `alert.<kind>.title` and `alert.<kind>.<cause>` as strings, and
 * i18next answers a missing key with the key itself. So a kind added in Rust
 * without translations reaches the user as the literal text
 * `alert.diskHealth.title` — and reaches a Windows toast as nothing at all,
 * because Rust skips a kind it has no pushed title for rather than falling
 * back to English.
 *
 * Both locales, because the missing one is always the locale nobody on the
 * team reads.
 */

import { beforeAll, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { Alert, AlertKind } from '@vitals/protocol';

import { DASHBOARD_NS, registerDashboardStrings } from '../dashboard/strings';
import { alertCause, alertTitle, ALERT_KINDS, THROTTLE_SLUGS } from './alertText';

beforeAll(async () => {
  await initI18n();
  registerDashboardStrings();
});

/** Every cause suffix a kind can carry, mirroring the Rust `evaluate`. */
function causesFor(kind: AlertKind): readonly string[] {
  return kind === 'cpuThrottled' || kind === 'gpuThrottled' ? THROTTLE_SLUGS : ['cause'];
}

function alertFor(kind: AlertKind, cause: string): Alert {
  return {
    kind,
    severity: 'warning',
    subject: '',
    title: `alert.${kind}.title`,
    cause: `alert.${kind}.${cause}`,
    // Every placeholder any string uses. i18next leaves an unmatched
    // `{{token}}` in place, so supplying the union catches an interpolation
    // that was renamed in Rust without the locale following.
    values: {
      percent: 91,
      seconds: 15,
      faults: 4000,
      ms: 40,
      celsius: 99,
      errors: 12,
      disk: 'C:',
      gpu: 'RTX 4090',
      adapter: 'Ethernet',
    },
    route: null,
    sinceSample: 1,
  };
}

describe.each(['en', 'ro'])('alert strings in %s', (locale) => {
  beforeAll(async () => {
    await i18n.changeLanguage(locale);
  });

  it('has thirteen kinds, matching the Rust enum', () => {
    // A literal list on purpose: derived from the generated type it would
    // grow silently with Rust and prove nothing.
    expect(ALERT_KINDS).toHaveLength(13);
  });

  it.each(ALERT_KINDS)('translates the title and every cause of %s', (kind) => {
    const t = i18n.getFixedT(locale, DASHBOARD_NS);

    for (const cause of causesFor(kind)) {
      const alert = alertFor(kind, cause);

      const title = alertTitle(alert, t);
      expect(title, `alert.${kind}.title is untranslated`).not.toMatch(/^alert\./);
      expect(title).not.toContain('{{');

      const why = alertCause(alert, t);
      expect(why, `alert.${kind}.${cause} is untranslated`).not.toMatch(/^alert\./);
      expect(why).not.toContain('{{');
    }
  });
});

describe('interpolation', () => {
  it('uses the numbers Rust already rounded rather than re-deriving them', async () => {
    // Two consumers that round independently disagree, and the one that is
    // wrong is whichever the user is not looking at.
    await i18n.changeLanguage('en');
    const t = i18n.getFixedT('en', DASHBOARD_NS);

    const alert = alertFor('thermalCpu', 'cause');
    expect(alertCause(alert, t)).toContain('99');
  });
});
