import { describe, expect, it } from 'vitest';

import { initI18n } from '@vitals/i18n';
import type { AlertKind, Severity } from '@vitals/protocol';

import { MOBILE_ALERTS_NS, bundles, registerMobileStrings } from './strings';

/**
 * The server's `AlertKind` and throttle slugs, as values. Kept literal on
 * purpose: a list derived from the type would grow silently with Rust, and
 * the point of this test is to fail when a kind appears whose strings nobody
 * wrote. `satisfies` still makes a typo a compile error.
 */
const KINDS = [
  'cpuSustained',
  'cpuThrottled',
  'memoryPressure',
  'memoryCommit',
  'diskSaturated',
  'diskLatency',
  'diskSpace',
  'diskHealth',
  'gpuThrottled',
  'thermalCpu',
  'networkErrors',
  'batteryLow',
  'batteryHealth',
] as const satisfies readonly AlertKind[];

const THROTTLE_SLUGS = [
  'thermal',
  'powerLimit',
  'currentLimit',
  'voltageDrop',
  'powerPolicy',
  'unknown',
] as const;

const SEVERITIES = ['info', 'warning', 'critical'] as const satisfies readonly Severity[];

function leafKeys(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];
  return Object.entries(value).flatMap(([k, v]) =>
    leafKeys(v, prefix === '' ? k : `${prefix}.${k}`),
  );
}

describe('mobile alert strings', () => {
  it('has the same keys in en and ro, like check-drift.ps1 enforces for the shared locales', () => {
    expect(leafKeys(bundles.ro).sort()).toEqual(leafKeys(bundles.en).sort());
  });

  it('has a title and every cause the server can emit, for every kind, in both locales', async () => {
    await initI18n('en');
    registerMobileStrings();
    const { i18n } = await import('@vitals/i18n');
    for (const locale of ['en', 'ro'] as const) {
      const t = i18n.getFixedT(locale, MOBILE_ALERTS_NS);
      for (const kind of KINDS) {
        expect(i18n.exists(`alert.${kind}.title`, { ns: MOBILE_ALERTS_NS, lng: locale })).toBe(
          true,
        );
        const causes =
          kind === 'cpuThrottled' || kind === 'gpuThrottled'
            ? THROTTLE_SLUGS
            : (['cause'] as const);
        for (const cause of causes) {
          expect(i18n.exists(`alert.${kind}.${cause}`, { ns: MOBILE_ALERTS_NS, lng: locale })).toBe(
            true,
          );
        }
      }
      for (const severity of SEVERITIES) {
        expect(t(`alert.severity.${severity}`)).not.toMatch(/^alert\./);
      }
      expect(t('alert.none')).not.toMatch(/^alert\./);
    }
  });

  it('does not clobber the shared translation namespace', async () => {
    await initI18n('en');
    registerMobileStrings();
    const { i18n } = await import('@vitals/i18n');
    expect(i18n.t('mobile.tab.machines')).toBe('Machines');
  });

  it('interpolates {{percent}} verbatim from the server values, in both locales', async () => {
    // The server rounds; the phone must print what it was given, not re-round.
    await initI18n('en');
    registerMobileStrings();
    const { i18n } = await import('@vitals/i18n');
    const values = { disk: 'C:', percent: 4.5 };
    const en = i18n.getFixedT('en', MOBILE_ALERTS_NS)('alert.diskSpace.cause', values);
    const ro = i18n.getFixedT('ro', MOBILE_ALERTS_NS)('alert.diskSpace.cause', values);
    expect(en).toContain('4.5%');
    expect(ro).toContain('4.5%');
    expect(en).not.toMatch(/\{\{/);
    expect(ro).not.toMatch(/\{\{/);
  });
});
