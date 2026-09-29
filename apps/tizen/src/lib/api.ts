/**
 * The PC calls `@vitals/client` does not cover yet (`/sensors`, `/history`),
 * and the mapping from a control failure to the sentence the TV shows.
 *
 * Kept tiny on purpose: these are two GETs with a bearer token. When the SDK
 * grows `sensors()` and `history()`, this file is the one to delete.
 */

import { isVitalsError } from '@vitals/client';
import type { MachineSample, SensorLine } from '@vitals/protocol';

import type { Pairing } from './pairing';

export class HttpError extends Error {
  override readonly name = 'HttpError';
  constructor(readonly status: number) {
    super(`HTTP ${status}`);
  }
}

async function getJson<T>(pairing: Pairing, path: string, fetchImpl: typeof fetch): Promise<T> {
  const response = await fetchImpl(`${pairing.baseUrl}${path}`, {
    headers: { Authorization: `Bearer ${pairing.token}` },
  });
  if (!response.ok) throw new HttpError(response.status);
  return (await response.json()) as T;
}

export function fetchSensors(pairing: Pairing, fetchImpl: typeof fetch): Promise<SensorLine[]> {
  return getJson<SensorLine[]>(pairing, '/api/v1/sensors', fetchImpl);
}

export function fetchHistory(
  pairing: Pairing,
  seconds: number,
  fetchImpl: typeof fetch,
): Promise<MachineSample[]> {
  return getJson<MachineSample[]>(pairing, `/api/v1/history?seconds=${seconds}`, fetchImpl);
}

/** What a failed control call means, as a locale key plus its interpolation. */
export interface ControlOutcome {
  readonly key: string;
  readonly values?: Record<string, string | number>;
  /** `true` when the failure proved the token is read-only. */
  readonly readOnly: boolean;
}

export function controlFailure(error: unknown): ControlOutcome {
  if (!isVitalsError(error)) return { key: 'control.unreachable', readOnly: false };
  const detail = error.detail;
  switch (error.kind) {
    case 'network':
    case 'closed':
      return { key: 'control.unreachable', readOnly: false };
    case 'forbidden':
      // 403 carries either "this token cannot act" or "Windows refused";
      // only the first says anything about the pairing.
      if (detail?.kind === 'access-denied') return { key: 'control.accessDenied', readOnly: false };
      return { key: 'control.forbidden', readOnly: true };
    default:
      break;
  }
  switch (detail?.kind) {
    case 'not-found':
      return { key: 'control.notFound', readOnly: false };
    case 'access-denied':
      return { key: 'control.accessDenied', readOnly: false };
    case 'unsupported':
      return { key: 'control.unsupported', values: { reason: detail.message }, readOnly: false };
    case 'internal':
      return { key: 'control.internal', values: { reason: detail.message }, readOnly: false };
    default:
      return { key: 'control.failed', values: { status: error.status ?? 0 }, readOnly: false };
  }
}
