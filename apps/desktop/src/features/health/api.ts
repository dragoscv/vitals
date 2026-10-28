/**
 * The bridge to the Health commands (apps/desktop/src-tauri/src/health.rs).
 *
 * The one place each command name is spelled, behind an injectable
 * interface so the screen and the settings panel test against a fake. The
 * contract with vmui is docs/api/vmui-health.md; the shapes below mirror the
 * Rust structs, where every reading is optional — `null` is "not measured"
 * and is rendered as an em dash, never as 0.
 */

import { hasTauriHost } from '../../shell/host';

export interface HealthStatus {
  readonly configured: boolean;
  readonly base: string | null;
  /** First eight characters of the device token; the rest never leaves Rust. */
  readonly tokenPrefix: string | null;
}

export interface PairStart {
  readonly id: string;
  /** The four digits the owner approves in vmui. */
  readonly code: string;
}

export type PairStatus = 'pending' | 'approved' | 'rejected' | 'expired';

export type HealthErrorKind = 'signedOut' | 'notOwner' | 'unreachable' | 'notConfigured';

export interface HrPoint {
  readonly t: number | null;
  readonly v: number | null;
}

export interface BodyEstimate {
  readonly bmi: number | null;
  readonly fatPct: number | null;
  readonly fatKg: number | null;
  readonly waterKg: number | null;
  readonly waterPct: number | null;
  readonly leanKg: number | null;
  readonly muscleKg: number | null;
  readonly bmrKcal: number | null;
  readonly boneKg: number | null;
  readonly ageYears: number | null;
  readonly bmiBand: string | null;
}

export interface HealthReadings {
  readonly device: string | null;
  readonly steps: number | null;
  readonly stepsAt: number | null;
  readonly distanceKm: number | null;
  readonly floors: number | null;
  readonly activeKcal: number | null;
  readonly totalKcal: number | null;
  readonly heartRate: number | null;
  readonly heartRateAt: number | null;
  readonly restingHr: number | null;
  readonly hrv: number | null;
  readonly spo2: number | null;
  readonly respiratoryRate: number | null;
  readonly sleepMin: number | null;
  readonly sleepAt: number | null;
  readonly weightKg: number | null;
  readonly weightAt: number | null;
  readonly bodyFat: number | null;
  readonly body: BodyEstimate | null;
  readonly bmr: number | null;
  readonly vo2max: number | null;
  readonly bloodPressure: { readonly sys: number | null; readonly dia: number | null } | null;
  readonly hrHistory: readonly HrPoint[] | null;
}

export interface Macros {
  readonly calories: number | null;
  readonly protein: number | null;
  readonly carbs: number | null;
  readonly fats: number | null;
}

export interface Nutrition {
  readonly day: string | null;
  readonly targets: (Macros & { readonly fiber?: number | null }) | null;
  readonly today: (Macros & { readonly meals?: number | null }) | null;
  readonly remaining: Macros | null;
  readonly balance: number | null;
  readonly activeKcal: number | null;
  readonly water: {
    readonly ml: number | null;
    readonly targetMl: number | null;
    readonly remainingMl: number | null;
  } | null;
}

export interface MeHealth {
  /** Epoch ms when vmui built the answer. */
  readonly at: number | null;
  readonly health: HealthReadings | null;
  readonly nutrition: Nutrition | null;
}

export interface HealthApi {
  status(): Promise<HealthStatus>;
  pairStart(base: string, name: string): Promise<PairStart>;
  pairPoll(id: string): Promise<PairStatus>;
  signIn(base: string, email: string, password: string): Promise<PairStatus>;
  disconnect(): Promise<void>;
  fetch(): Promise<MeHealth>;
}

/** A failed read, carrying the kind the screen chooses its words from. */
export class HealthError extends Error {
  constructor(readonly kind: HealthErrorKind) {
    super(kind);
    this.name = 'HealthError';
  }
}

/** A `health_fetch` rejection, narrowed to its kind; anything else is "unreachable". */
export function errorKind(error: unknown): HealthErrorKind {
  if (typeof error === 'object' && error !== null && 'kind' in error) {
    const { kind } = error;
    if (
      kind === 'signedOut' ||
      kind === 'notOwner' ||
      kind === 'unreachable' ||
      kind === 'notConfigured'
    ) {
      return kind;
    }
  }
  return 'unreachable';
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const core = await import('@tauri-apps/api/core');
  return core.invoke<T>(command, args);
}

export const tauriHealthApi: HealthApi = {
  status: () => invoke<HealthStatus>('health_status'),
  pairStart: (base, name) => invoke<PairStart>('health_pair_start', { base, name }),
  pairPoll: async (id) => (await invoke<{ status: PairStatus }>('health_pair_poll', { id })).status,
  signIn: async (base, email, password) =>
    (await invoke<{ status: PairStatus }>('health_sign_in', { base, email, password })).status,
  disconnect: () => invoke<void>('health_disconnect'),
  fetch: () => invoke<MeHealth>('health_fetch'),
};

/** The browser preview: never connected, nothing to read. */
export const noHostHealthApi: HealthApi = {
  status: () => Promise.resolve({ configured: false, base: null, tokenPrefix: null }),
  pairStart: () => Promise.reject(new Error('no host')),
  pairPoll: () => Promise.resolve('expired'),
  signIn: () => Promise.reject(new Error('no host')),
  disconnect: () => Promise.resolve(),
  fetch: () => Promise.reject(new HealthError('notConfigured')),
};

export function defaultHealthApi(): HealthApi {
  return hasTauriHost() ? tauriHealthApi : noHostHealthApi;
}
