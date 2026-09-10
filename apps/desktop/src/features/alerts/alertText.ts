/**
 * Renders a Rust-produced alert in the user's language.
 *
 * The engine emits keys, never prose: the desktop, the phone and a Windows
 * toast all show the same alert, and the wire must not bake one language in.
 * The keys are `alert.<kind>.title` and `alert.<kind>.<cause>`, where `<cause>`
 * is either the literal `cause` or a throttle slug — so the locale files and
 * the Rust `AlertKind` enum are a contract that no compiler checks. That is
 * what `alertText.test.ts` exists for.
 *
 * `values` arrive already rounded from Rust, so nothing is re-derived here and
 * two consumers cannot disagree about the number.
 */

import type { TFunction } from 'i18next';

import type { Alert, AlertKind } from '@vitals/protocol';

/**
 * Every kind, as a value rather than a type.
 *
 * A literal list rather than something derived: the point of the drift test is
 * to fail when Rust grows a kind whose strings nobody wrote, and a list
 * derived from the generated type would grow silently with it. The
 * `satisfies` clause still makes a typo a compile error.
 */
export const ALERT_KINDS = [
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

/**
 * The cause suffixes a throttle alert can carry.
 *
 * Mirrors `throttle_slug` in `crates/vitals-core/src/alerts.rs`. Only
 * `cpuThrottled` and `gpuThrottled` use these; every other kind uses `cause`.
 */
export const THROTTLE_SLUGS = [
  'thermal',
  'powerLimit',
  'currentLimit',
  'voltageDrop',
  'powerPolicy',
  'unknown',
] as const;

/** Interpolation values, in the shape i18next wants them. */
function interpolation(alert: Alert): Record<string, number | string> {
  return alert.values;
}

/** The headline: what is wrong. */
export function alertTitle(alert: Alert, t: TFunction): string {
  return t(alert.title, interpolation(alert));
}

/** The sentence underneath: why, not a restatement of the number above it. */
export function alertCause(alert: Alert, t: TFunction): string {
  return t(alert.cause, interpolation(alert));
}
