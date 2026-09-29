/**
 * PC addresses: normalising what a person typed, and refusing anything that
 * is not on a private network.
 *
 * The LAN API is plain HTTP with a bearer token (ADR-0003). A token pointed
 * at a public address would be sent in the clear across the internet, so the
 * restriction is enforced here, before anything is saved or sent. Mirrors
 * `apps/android/core/.../LanAddress.kt` and `PairCheck.normalise` so the TV
 * apps cannot disagree about what a valid PC address is.
 */

export const DEFAULT_PORT = 7331;

/**
 * `192.168.1.20` → `http://192.168.1.20:7331`. `null` when it cannot be an
 * address at all. A trailing slash or path is dropped: the API lives at the
 * root of the PC's server.
 */
export function normaliseAddress(raw: string): string | null {
  const text = raw.trim().replace(/\/+$/, '');
  if (text === '') return null;
  // `http:` with or without slashes counts as a scheme, so a bare `http://`
  // (trimmed to `http:`) is refused rather than read as a host named "http".
  const withScheme = /^https?:/i.test(text) ? text : `http://${text}`;
  let url: URL;
  try {
    url = new URL(withScheme);
  } catch {
    return null;
  }
  if (url.hostname === '' || url.username !== '' || url.password !== '') return null;
  const port = url.port === '' ? DEFAULT_PORT : Number(url.port);
  return `${url.protocol}//${url.hostname}:${port}`;
}

function isPrivateIpv4(host: string): boolean {
  const parts = host.split('.');
  if (parts.length !== 4) return false;
  const p: number[] = [];
  for (const part of parts) {
    if (!/^\d{1,3}$/.test(part)) return false;
    const n = Number(part);
    if (n > 255) return false;
    p.push(n);
  }
  const [a = -1, b = -1] = p;
  return (
    a === 10 ||
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 168) ||
    (a === 169 && b === 254) ||
    a === 127 ||
    // CGNAT: Tailscale and some ISP-provided home routers.
    (a === 100 && b >= 64 && b <= 127)
  );
}

function isPrivateIpv6(host: string): boolean {
  const h = host.toLowerCase();
  if (!h.includes(':')) return false;
  return h === '::1' || /^fe[89ab]/.test(h) || h.startsWith('fc') || h.startsWith('fd');
}

/** Whether a normalised base URL points at the local network. */
export function isPrivateAddress(baseUrl: string): boolean {
  let host: string;
  try {
    host = new URL(baseUrl).hostname;
  } catch {
    return false;
  }
  host = host.replace(/^\[|\]$/g, '');
  const lower = host.toLowerCase();
  if (lower === 'localhost' || lower.endsWith('.local')) return true;
  return isPrivateIpv4(host) || isPrivateIpv6(host);
}
