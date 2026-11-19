// Reads Google Play's production-track publishing state, so a pipeline can
// prove the one thing the Play Console holds and no build artefact does: how
// much of the world the live release actually reaches.
//
// Why this exists. `release.yml` uploads a stable tag to the production track
// as a STAGED rollout (PLAY_ROLLOUT, default 0.2), so a bad build reaches a
// fifth of users and can be halted. Nothing in the repository could read that
// number back: on 2026-11-19 the rollout for app.vitals was raised to 100 % by
// hand in the Console and the repository never noticed — a release left at
// 20 % reaches a fifth of users while every gate stays green. This is the
// read-back, the Play half of `scripts/check-store-listing.mjs`.
//
// How it reads it, without the Console UI. The Play Developer API
// (androidpublisher v3) has no "read the track" endpoint on its own: state is
// reached through an *edit*. So: insert an edit, read the track's releases,
// then DELETE the edit to discard it — the read leaves the track exactly as it
// found it. The same API and the same service account the `play` job uploads
// with, but read-only in effect.
//
// Credentials, in this order — the first that is present is used, and nothing
// secret is ever printed:
//   1. PLAY_ACCESS_TOKEN — a ready bearer token. This is what a workflow passes
//      (`google-github-actions/auth`'s `auth_token` output), and what a local run
//      passes from `gcloud auth print-access-token`.
//   2. PLAY_SERVICE_ACCOUNT_JSON — a service-account key, as the JSON text. A
//      repository secret lets a job that cannot use Workload Identity Federation
//      (see apps/android/play/README.md: the provider only accepts `refs/tags/v`)
//      still read the track.
//   3. GOOGLE_APPLICATION_CREDENTIALS — a file path, when it holds a
//      `service_account` key.
// When none is present the caller prints SKIPPED and decides nothing about the
// exit code, exactly like the App Store Connect half.
//
// Deliberately NOT used: the machine's ambient gcloud Application Default
// Credentials. ADC is minted with the `cloud-platform` scope, which the Play
// API rejects ("Request had insufficient authentication scopes", 403), so
// treating it as a credential would turn a clean local run into a spurious
// "could not run". A developer who wants the Play half locally runs an ADC
// login with the Play scope and passes the token explicitly:
//   gcloud auth application-default login \
//     --scopes=https://www.googleapis.com/auth/androidpublisher,https://www.googleapis.com/auth/cloud-platform
//   PLAY_ACCESS_TOKEN=$(gcloud auth print-access-token) node scripts/check-store-listing.mjs

import { createPrivateKey, sign } from 'node:crypto';
import { readFileSync } from 'node:fs';

const API = 'https://androidpublisher.googleapis.com/androidpublisher/v3';
// The Play API calls the phone's production track simply `production`; the
// watch and TV form-factor tracks are `wear:production` and `tv:production`.
export const DEFAULT_PACKAGE = 'app.vitals';
const SCOPE = 'https://www.googleapis.com/auth/androidpublisher';
const TIMEOUT_MS = 30_000;

/** Whether any Play credential is configured. Cheap: reads env and, at most, one file. */
export function playCredentialsAvailable() {
  if ((process.env.PLAY_ACCESS_TOKEN ?? '').trim()) return true;
  if ((process.env.PLAY_SERVICE_ACCOUNT_JSON ?? '').trim()) return true;
  const path = (process.env.GOOGLE_APPLICATION_CREDENTIALS ?? '').trim();
  if (!path) return false;
  try {
    const creds = JSON.parse(readFileSync(path, 'utf8'));
    return (
      creds.type === 'service_account' && Boolean(creds.private_key) && Boolean(creds.client_email)
    );
  } catch {
    return false;
  }
}

const b64u = (b) => Buffer.from(b).toString('base64url');

/** Exchange a service-account key for an access token (JWT bearer flow). */
async function serviceAccountToken(creds) {
  const now = Math.floor(Date.now() / 1000);
  const tokenUri = creds.token_uri || 'https://oauth2.googleapis.com/token';
  const head = b64u(JSON.stringify({ alg: 'RS256', typ: 'JWT' }));
  const body = b64u(
    JSON.stringify({
      iss: creds.client_email,
      scope: SCOPE,
      aud: tokenUri,
      iat: now,
      exp: now + 3600,
    }),
  );
  const key = createPrivateKey(creds.private_key);
  const sig = sign('sha256', Buffer.from(`${head}.${body}`), { key, dsaEncoding: 'ieee-p1363' });
  const assertion = `${head}.${body}.${b64u(sig)}`;
  const res = await fetch(tokenUri, {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      grant_type: 'urn:ietf:params:oauth:grant-type:jwt-bearer',
      assertion,
    }),
    signal: AbortSignal.timeout(TIMEOUT_MS),
  });
  const json = await res.json().catch(() => ({}));
  if (res.status !== 200 || !json.access_token) {
    throw new Error(
      `service-account token exchange answered HTTP ${res.status}: ${json.error ?? ''}`,
    );
  }
  return json.access_token;
}

/** A bearer token with the Play scope, from whichever credential is present. */
export async function playAccessToken() {
  const explicit = (process.env.PLAY_ACCESS_TOKEN ?? '').trim();
  if (explicit) return explicit;

  const inlineJson = (process.env.PLAY_SERVICE_ACCOUNT_JSON ?? '').trim();
  if (inlineJson) return serviceAccountToken(JSON.parse(inlineJson));

  const path = (process.env.GOOGLE_APPLICATION_CREDENTIALS ?? '').trim();
  if (path) {
    const creds = JSON.parse(readFileSync(path, 'utf8'));
    if (creds.type === 'service_account') return serviceAccountToken(creds);
    throw new Error(
      `GOOGLE_APPLICATION_CREDENTIALS is a ${creds.type ?? 'unknown'} credential, not a service_account key; ` +
        'pass the scoped token through PLAY_ACCESS_TOKEN instead',
    );
  }
  throw new Error(
    'no Play credential (PLAY_ACCESS_TOKEN, PLAY_SERVICE_ACCOUNT_JSON or a service-account file)',
  );
}

/** One authenticated Play API call. Returns { status, json }; never throws on HTTP >= 400. */
async function call(method, path, token) {
  const res = await fetch(`${API}${path}`, {
    method,
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    signal: AbortSignal.timeout(TIMEOUT_MS),
  });
  const text = await res.text();
  let json = {};
  try {
    json = text ? JSON.parse(text) : {};
  } catch {
    json = { raw: text };
  }
  return { status: res.status, json };
}

/**
 * A release is one entry of a track. `userFraction` is present ONLY for a
 * staged rollout: Play omits it for a full rollout, so "no fraction" is the
 * good state, not "fraction 0". It is not the only spelling of "everyone"
 * though — `PLAY_ROLLOUT` may be set to `1`, which the upload action sends as an
 * explicit `userFraction: 1` — so 1.0 is full too, and only a fraction strictly
 * below 1 is staged. `status` is one of draft, inProgress, halted, completed.
 */
const normaliseRelease = (release) => ({
  name: release.name ?? null,
  status: release.status ?? null,
  versionCodes: (release.versionCodes ?? []).map((c) => String(c)),
  userFraction: typeof release.userFraction === 'number' ? release.userFraction : null,
  releaseNotesLocales: (release.releaseNotes ?? []).map((n) => n.language).slice(0, 8),
});

/**
 * Whether a set of releases puts the app in front of everyone.
 *
 * `completed` is everyone; `inProgress` with no fraction is everyone (Play's
 * own encoding of a 100 % rollout), and so is an explicit `userFraction` of 1.
 * Anything else has not reached everyone: a fraction strictly below 1 is a
 * staged rollout, `halted` is stopped, `draft` was never published, and an
 * empty track was never released to production.
 */
export function rolloutVerdict(releases) {
  const live = releases.filter((r) => r.status === 'inProgress' || r.status === 'completed');
  const full = live.find(
    (r) => r.status === 'completed' || r.userFraction === null || r.userFraction >= 1,
  );
  if (full) {
    return {
      full: true,
      state: full.status,
      fraction: null,
      summary: `fully rolled out (${full.name ?? 'release'} ${full.status})`,
    };
  }
  const staged = live.find(
    (r) => r.status === 'inProgress' && r.userFraction !== null && r.userFraction < 1,
  );
  if (staged) {
    const pct = Math.round((staged.userFraction ?? 0) * 100);
    return {
      full: false,
      state: 'staged',
      fraction: staged.userFraction,
      summary: `staged rollout to ${staged.userFraction} (${pct} % of users), release ${staged.name ?? 'unnamed'}`,
    };
  }
  const halted = releases.find((r) => r.status === 'halted');
  if (halted) {
    return {
      full: false,
      state: 'halted',
      fraction: null,
      summary: `the production rollout is halted (${halted.name ?? 'unnamed'})`,
    };
  }
  const draft = releases.find((r) => r.status === 'draft');
  if (draft) {
    return {
      full: false,
      state: 'draft',
      fraction: null,
      summary: `the production release is still a draft (${draft.name ?? 'unnamed'})`,
    };
  }
  return {
    full: false,
    state: 'none',
    fraction: null,
    summary: 'the production track has no release',
  };
}

/**
 * Read the production track. Creates an edit, reads the track, discards the
 * edit in a `finally` so a failed read cannot leave a dangling edit behind.
 * Returns { releases, verdict }; throws only when the check could not run.
 */
export async function readProductionTrack({
  packageName = DEFAULT_PACKAGE,
  track = 'production',
} = {}) {
  const token = await playAccessToken();

  const opened = await call('POST', `/applications/${packageName}/edits`, token);
  if (opened.status !== 200 || !opened.json.id) {
    const detail = opened.json?.error?.message ?? `HTTP ${opened.status}`;
    throw new Error(`could not open a Play edit for ${packageName}: ${detail}`);
  }
  const editId = opened.json.id;

  try {
    const res = await call(
      'GET',
      `/applications/${packageName}/edits/${editId}/tracks/${track}`,
      token,
    );
    if (res.status === 404) {
      return { releases: [], verdict: rolloutVerdict([]), trackMissing: true };
    }
    if (res.status !== 200) {
      const detail = res.json?.error?.message ?? `HTTP ${res.status}`;
      throw new Error(`could not read the ${track} track of ${packageName}: ${detail}`);
    }
    const releases = (res.json.releases ?? []).map(normaliseRelease);
    return { releases, verdict: rolloutVerdict(releases), trackMissing: false };
  } finally {
    // Discarding the edit changes nothing on the track; ignoring its result is
    // safe (a stale edit expires on its own) and must not mask a read error.
    try {
      await call('DELETE', `/applications/${packageName}/edits/${editId}`, token);
    } catch {
      /* the edit expires by itself; nothing to do */
    }
  }
}
