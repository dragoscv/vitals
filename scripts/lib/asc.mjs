// A self-contained App Store Connect client for the scripts that have to run
// in CI.
//
// Why this exists rather than importing the shared client: the shared copy
// lives at ~/.copilot/bin/apple-asc.mjs, which is a machine-local install, and
// a GitHub runner has no home-directory tooling. `scripts/check-store-listing.mjs`
// and `scripts/ios-store.mjs --check` are wired into `.github/workflows/store-listing.yml`,
// and on 2026-11-19 all three runs of that workflow failed with
// ERR_MODULE_NOT_FOUND for exactly that path. The alternative — vendoring the
// whole shared file — would put a second copy of a client whose source of truth
// is elsewhere in the tree; this is the small half of it that a read-only check
// needs, with no gcloud and no dependency.
//
// Credentials, in this order:
//   1. the environment: APPLE_ASC_KEY_P8, APPLE_ASC_KEY_ID, APPLE_ASC_ISSUER,
//      and optionally APPLE_TEAM_ID (what a workflow provides);
//   2. otherwise gcloud Secret Manager in APPLE_SECRETS_PROJECT (default
//      dragos-devcloud), the same secrets the other scripts read.
// Nothing secret is ever printed.

import { execFileSync } from 'node:child_process';
import { createPrivateKey, sign } from 'node:crypto';

const PROJECT = process.env.APPLE_SECRETS_PROJECT || 'dragos-devcloud';
const API = 'https://api.appstoreconnect.apple.com';

/** Read a secret from the environment, else from Secret Manager. */
export function secret(name, optional = false) {
  const envName = name.toUpperCase().replace(/-/g, '_');
  if (process.env[envName]) return process.env[envName].trim();
  try {
    const args = [
      'secrets',
      'versions',
      'access',
      'latest',
      `--secret=${name}`,
      `--project=${PROJECT}`,
    ];
    const [bin, argv] =
      process.platform === 'win32'
        ? ['cmd.exe', ['/d', '/c', 'gcloud', ...args]]
        : ['gcloud', args];
    // Out-file would be byte-exact; the CLI's stdout is what the existing
    // scripts use and every secret here is text. A PEM read this way keeps its
    // newlines only because execFileSync returns one string, not an array of
    // lines — the trap that once wrote a six-character p8 into a GitHub secret
    // through PowerShell's pipeline.
    return execFileSync(bin, argv, {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
  } catch {
    if (optional) return '';
    throw new Error(`secret ${name} not readable in ${PROJECT} (env or gcloud auth?)`);
  }
}

const b64u = (b) => Buffer.from(b).toString('base64url');

/** A 15-minute ES256 JWT, and the ids it was made from. */
export function ascToken() {
  const kid = secret('apple-asc-key-id');
  const iss = secret('apple-asc-issuer');
  const key = createPrivateKey(secret('apple-asc-key-p8'));
  const now = Math.floor(Date.now() / 1000);
  const head = b64u(JSON.stringify({ alg: 'ES256', kid, typ: 'JWT' }));
  const body = b64u(
    JSON.stringify({ iss, iat: now, exp: now + 15 * 60, aud: 'appstoreconnect-v1' }),
  );
  const sig = sign('sha256', Buffer.from(`${head}.${body}`), { key, dsaEncoding: 'ieee-p1363' });
  return { token: `${head}.${body}.${b64u(sig)}`, kid, iss };
}

/** One request. Returns { status, json } and never throws on HTTP >= 400. */
export async function asc(method, path, body) {
  const { token } = ascToken();
  const res = await fetch(path.startsWith('http') ? path : API + path, {
    method,
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : typeof body === 'string' ? body : JSON.stringify(body),
    signal: AbortSignal.timeout(60_000),
  });
  const text = await res.text();
  let json;
  try {
    json = text ? JSON.parse(text) : {};
  } catch {
    json = { raw: text };
  }
  return { status: res.status, json };
}
