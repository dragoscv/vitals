/**
 * Pairings: which PCs this phone may talk to, and how a new one arrives.
 *
 * A pairing is claimed from the URL fragment (`#t=<token>`). The fragment is
 * never sent to the server, so the token does not appear in any request line
 * or access log; it is read once here and then removed from the address bar
 * and from history, so a shared phone does not keep it in its back stack.
 */

import type { Health } from '@vitals/client';
import type { HostInfo } from '@vitals/protocol';

/** Versioned so a future shape change can migrate instead of crashing on parse. */
export const PAIRINGS_KEY = 'vitals.mobile.pairings.v1';

/** One paired PC. */
export interface Pairing {
  /** The origin, which is also the identity: re-scanning a PC replaces its token. */
  readonly id: string;
  readonly baseUrl: string;
  readonly token: string;
  readonly name: string;
  readonly addedAt: number;
  /**
   * Learned, not declared: the health endpoint does not say what a token may
   * do, so this is set the first time the PC answers a control request with
   * 403 and cleared when the PC is re-paired.
   */
  readonly readOnly: boolean;
}

/** The calls needed to validate a fresh pairing. Injected so tests never touch the network. */
export interface PairingProbe {
  health(): Promise<Health>;
  host(): Promise<HostInfo | null>;
}

/** Reads the stored list; malformed storage yields an empty list rather than a crash. */
export function readPairings(storage: Storage): Pairing[] {
  const raw = storage.getItem(PAIRINGS_KEY);
  if (raw === null) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter(isPairing) : [];
  } catch {
    return [];
  }
}

export function writePairings(storage: Storage, pairings: readonly Pairing[]): void {
  storage.setItem(PAIRINGS_KEY, JSON.stringify(pairings));
}

/** Extracts the token from `#t=…`, or `undefined` when the fragment is not a pairing. */
export function tokenFromFragment(hash: string): string | undefined {
  const match = /^#t=([^&]+)/.exec(hash);
  return match?.[1] === undefined ? undefined : decodeURIComponent(match[1]);
}

/**
 * Removes the fragment without a navigation and without leaving the tokened
 * URL in history — `location.hash = ''` would push a new entry and keep the
 * old one one swipe away.
 */
export function stripFragment(location: Location, history: History): void {
  history.replaceState(history.state, '', location.pathname + location.search);
}

export interface ClaimOptions {
  readonly location: Location;
  readonly history: History;
  readonly storage: Storage;
  readonly makeProbe: (baseUrl: string, token: string) => PairingProbe;
  readonly now: () => number;
}

/**
 * Claims the pairing in the URL, if any.
 *
 * Resolves to the new pairing, `undefined` when the URL carried none, and
 * rejects when the PC did not answer — the caller shows that as a message,
 * and nothing is stored, so a mistyped or expired token cannot leave a dead
 * card behind.
 */
export async function claimPairing(options: ClaimOptions): Promise<Pairing | undefined> {
  const token = tokenFromFragment(options.location.hash);
  if (token === undefined) return undefined;

  // Stripped before validation, not after: even a failed claim must not leave
  // the token in the address bar.
  stripFragment(options.location, options.history);

  const baseUrl = options.location.origin;
  const probe = options.makeProbe(baseUrl, token);
  await probe.health();
  const host = await probe.host();

  const existing = readPairings(options.storage);
  const previous = existing.find((p) => p.id === baseUrl);
  const pairing: Pairing = {
    id: baseUrl,
    baseUrl,
    token,
    name: previous?.name ?? host?.hostname ?? hostFromUrl(baseUrl),
    addedAt: options.now(),
    readOnly: false,
  };
  writePairings(options.storage, [...existing.filter((p) => p.id !== baseUrl), pairing]);
  return pairing;
}

function hostFromUrl(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

function isPairing(value: unknown): value is Pairing {
  if (typeof value !== 'object' || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v['id'] === 'string' &&
    typeof v['baseUrl'] === 'string' &&
    typeof v['token'] === 'string' &&
    typeof v['name'] === 'string' &&
    typeof v['addedAt'] === 'number' &&
    typeof v['readOnly'] === 'boolean'
  );
}
