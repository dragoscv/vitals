/**
 * Pairing a PC with this TV: by six-digit code (the normal way on a TV,
 * where nobody can scan a QR code) or by pasting the 43-character token.
 *
 * Mirrors `apps/android/core/.../PairCheck.kt`, so the Google TV and the
 * Samsung TV agree about what a valid pairing is: a pairing is saved only
 * after `/health` proved a Vitals server is there speaking model version 1
 * and an authenticated call proved the token works.
 */

import { VitalsClient, isVitalsError } from '@vitals/client';

import { isPrivateAddress, normaliseAddress } from './address';

/** What the token may do. `unknown` until a control call or the pairing reply says. */
export type Scope = 'read' | 'control' | 'unknown';

export interface Pairing {
  readonly id: string;
  readonly label: string;
  readonly baseUrl: string;
  readonly token: string;
  readonly scope: Scope;
  readonly createdMs: number;
}

/** Why a PC was not saved. Each maps to one sentence in the locale files (`pairing.*`). */
export type PairRefusal =
  | 'badAddress'
  | 'notPrivate'
  | 'badCode'
  | 'badToken'
  | 'unreachable'
  | 'unauthorised'
  | 'incompatible'
  | 'olderPc'
  | 'notReady'
  | 'failed';

export type PairResult =
  | { readonly ok: true; readonly pairing: Pairing }
  | { readonly ok: false; readonly reason: PairRefusal; readonly status?: number };

/** The frame-shape version this app understands. A PC speaking another is refused, not guessed at. */
export const MODEL_VERSION = 1;

/** A token is 32 random bytes, base64url without padding: always 43 characters. */
const TOKEN = /^[A-Za-z0-9_-]{43}$/;
const CODE = /^\d{6}$/;

/** Digits only, as typed on a remote: the 3+3 grouping and spaces the desktop shows are ignored. */
export function cleanCode(raw: string): string {
  return raw.replace(/\D/g, '');
}

/**
 * Exactly six digits after cleaning. Seven is refused rather than truncated:
 * silently dropping a digit would spend one of the code's five attempts on a
 * guess the user never made.
 */
export function isCode(raw: string): boolean {
  return CODE.test(cleanCode(raw));
}

export function isToken(raw: string): boolean {
  return TOKEN.test(raw.trim());
}

function refused(reason: PairRefusal, status?: number): PairResult {
  return status === undefined ? { ok: false, reason } : { ok: false, reason, status };
}

function newId(): string {
  // `crypto.randomUUID` needs a secure context; a packaged app's `file://`
  // origin normally is one, but a dev build over plain HTTP on the LAN is not.
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    try {
      return crypto.randomUUID();
    } catch {
      // Fall through to the non-cryptographic id; it only needs to be unique locally.
    }
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}

/** What `POST /api/v1/pair` answered, before the token has been checked. */
export type Redeemed =
  | { readonly ok: true; readonly token: string; readonly scope: Scope }
  | { readonly ok: false; readonly reason: PairRefusal; readonly status?: number };

/**
 * Exchanges a code for a token.
 *
 * `@vitals/client` has no method for this yet, so it is a direct `fetch`.
 * Every refusal from the server is the same `403` — wrong, expired, burnt or
 * no code active — by design, so it maps to one message that covers all four.
 */
export async function redeemCode(
  baseUrl: string,
  code: string,
  deviceLabel: string,
  fetchImpl: typeof fetch,
): Promise<Redeemed> {
  let response: Response;
  try {
    response = await fetchImpl(`${baseUrl}/api/v1/pair`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code, label: deviceLabel }),
    });
  } catch {
    return { ok: false, reason: 'unreachable' };
  }
  switch (response.status) {
    case 200:
      break;
    // 400 means the body was malformed; this module never sends one, so if it
    // happens the code itself was what the server disliked.
    case 400:
    case 403:
      return { ok: false, reason: 'badCode', status: response.status };
    // A PC whose Vitals predates pairing codes.
    case 404:
      return { ok: false, reason: 'olderPc', status: 404 };
    default:
      return { ok: false, reason: 'failed', status: response.status };
  }
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    return { ok: false, reason: 'failed', status: 200 };
  }
  if (typeof body !== 'object' || body === null)
    return { ok: false, reason: 'failed', status: 200 };
  const { token, scope } = body as { token?: unknown; scope?: unknown };
  if (typeof token !== 'string' || !isToken(token)) {
    return { ok: false, reason: 'failed', status: 200 };
  }
  return { ok: true, token, scope: scope === 'read' || scope === 'control' ? scope : 'unknown' };
}

/** Checks a token against a PC and builds the pairing to save. */
export async function checkToken(
  baseUrl: string,
  rawToken: string,
  scope: Scope,
  existing: readonly Pairing[],
  fetchImpl: typeof fetch,
  now: () => number = Date.now,
): Promise<PairResult> {
  const token = rawToken.trim();
  if (!isToken(token)) return refused('badToken');
  const client = new VitalsClient({ baseUrl, token, fetch: fetchImpl });
  try {
    const health = await client.health();
    if (health.modelVersion !== MODEL_VERSION) return refused('incompatible');
  } catch (error) {
    return refusal(error);
  }
  let hostname: string | null;
  try {
    // Authenticated, so it proves the token as well as naming the PC.
    hostname = (await client.host())?.hostname ?? null;
  } catch (error) {
    return refusal(error);
  }
  const previous = existing.find((p) => p.baseUrl === baseUrl);
  const label = hostname ?? new URL(baseUrl).hostname;
  return {
    ok: true,
    pairing: {
      // Re-pairing the same PC replaces its entry rather than listing it twice.
      id: previous?.id ?? newId(),
      label,
      baseUrl,
      token,
      scope,
      createdMs: previous?.createdMs ?? now(),
    },
  };
}

function refusal(error: unknown): PairResult {
  if (!isVitalsError(error)) return refused('failed');
  switch (error.kind) {
    case 'network':
      return refused('unreachable');
    case 'unauthorised':
      return refused('unauthorised');
    case 'protocol':
      return refused('incompatible', error.status);
    default:
      return error.status === 503 ? refused('notReady', 503) : refused('failed', error.status);
  }
}

function prepare(rawAddress: string): { ok: true; baseUrl: string } | PairResult {
  const baseUrl = normaliseAddress(rawAddress);
  if (baseUrl === null) return refused('badAddress');
  if (!isPrivateAddress(baseUrl)) return refused('notPrivate');
  return { ok: true, baseUrl };
}

export async function pairWithCode(
  rawAddress: string,
  rawCode: string,
  deviceLabel: string,
  existing: readonly Pairing[],
  fetchImpl: typeof fetch,
): Promise<PairResult> {
  const address = prepare(rawAddress);
  if (!('baseUrl' in address)) return address;
  const code = cleanCode(rawCode);
  if (!CODE.test(code)) return refused('badCode');
  const grant = await redeemCode(address.baseUrl, code, deviceLabel, fetchImpl);
  if (!grant.ok) return refused(grant.reason, grant.status);
  return checkToken(address.baseUrl, grant.token, grant.scope, existing, fetchImpl);
}

export async function pairWithToken(
  rawAddress: string,
  rawToken: string,
  existing: readonly Pairing[],
  fetchImpl: typeof fetch,
): Promise<PairResult> {
  const address = prepare(rawAddress);
  if (!('baseUrl' in address)) return address;
  return checkToken(address.baseUrl, rawToken, 'unknown', existing, fetchImpl);
}

// ── Storage ───────────────────────────────────────────────────────────────

/**
 * Pairings live in `localStorage`, which on a Tizen TV is the app's own
 * sandbox. A web app has no Keystore to encrypt them with, so the token sits
 * there in plain text; it is a bearer credential for one PC on the LAN, and
 * the README says so.
 */
const STORAGE_KEY = 'vitals.pairings.v1';

function isPairing(value: unknown): value is Pairing {
  if (typeof value !== 'object' || value === null) return false;
  const p = value as Record<string, unknown>;
  return (
    typeof p.id === 'string' &&
    typeof p.label === 'string' &&
    typeof p.baseUrl === 'string' &&
    typeof p.token === 'string' &&
    (p.scope === 'read' || p.scope === 'control' || p.scope === 'unknown') &&
    typeof p.createdMs === 'number'
  );
}

export function loadPairings(storage: Pick<Storage, 'getItem'>): Pairing[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(storage.getItem(STORAGE_KEY) ?? '[]');
  } catch {
    return [];
  }
  // A saved entry that points outside the LAN is dropped on load too, so a
  // tampered store cannot aim a token at the internet.
  return Array.isArray(parsed)
    ? parsed.filter(isPairing).filter((p) => isPrivateAddress(p.baseUrl))
    : [];
}

/** The saved PCs, observable for React's `useSyncExternalStore`. */
export class PairingStore {
  private list: readonly Pairing[];
  private readonly listeners = new Set<() => void>();

  constructor(private readonly storage: Pick<Storage, 'getItem' | 'setItem'>) {
    this.list = loadPairings(storage);
  }

  readonly get = (): readonly Pairing[] => this.list;

  readonly subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  upsert(pairing: Pairing): void {
    const others = this.list.filter((p) => p.id !== pairing.id);
    this.set([...others, pairing].sort((a, b) => a.createdMs - b.createdMs));
  }

  remove(id: string): void {
    this.set(this.list.filter((p) => p.id !== id));
  }

  /** Records what a control attempt taught us about the token. */
  setScope(id: string, scope: Scope): void {
    const current = this.list.find((p) => p.id === id);
    if (current === undefined || current.scope === scope) return;
    this.upsert({ ...current, scope });
  }

  private set(next: readonly Pairing[]): void {
    this.list = next;
    try {
      this.storage.setItem(STORAGE_KEY, JSON.stringify(next));
    } catch (error) {
      // Quota or a disabled store: keep working for this session.
      console.warn('could not save pairings', error);
    }
    for (const listener of this.listeners) listener();
  }
}
