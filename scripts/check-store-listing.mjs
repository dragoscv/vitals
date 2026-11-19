// Guards the two store states no build can prove.
//
// First, that the app is actually FOR SALE. On 2026-11-18 the iPhone app had
// two versions READY_FOR_SALE and no availability record at all, so App Store
// Connect read "This app was removed from sale from the App Store", the product
// page 404'd and `itunes.apple.com/lookup` returned resultCount 0 — for weeks,
// unnoticed, because every gate watched the build rather than the listing.
//
// Second, that the live release actually REACHES everyone. `release.yml` ships
// a stable tag to the Play production track as a staged rollout
// (PLAY_ROLLOUT, default 0.2); on 2026-11-19 that rollout for app.vitals was
// raised from 20 % to 100 % by hand in the Play Console and nothing in the
// repository could tell. A release left at 20 % reaches a fifth of users while
// every gate stays green; `--require-full-rollout` makes a staged production
// rollout exit 1, so a release cannot be declared complete at a fifth of users.
// Read back through `scripts/play-rollout.mjs` (the Play Developer API's
// edits/tracks, discarded immediately).
//
// Two layers, because the first one needs no credential at all:
//   1. the public lookup and the storefront pages, which is what a customer
//      sees: an app that is not for sale answers resultCount 0 with HTTP 200;
//   2. the store's own record: the App Store Connect availability record for
//      Apple, and the production track's release state for Google Play.
//
// Credentials: for Apple the team's ASC key, read from the env vars
// APPLE_ASC_KEY_P8, APPLE_ASC_KEY_ID and APPLE_ASC_ISSUER (what the workflow
// provides), else from Secret Manager in APPLE_SECRETS_PROJECT
// (dragos-devcloud) — the client is `scripts/lib/asc.mjs`, a repo-local copy on
// purpose: this script runs in CI, where the machine-local
// ~/.copilot/bin/apple-asc.mjs does not exist (all three runs of the first
// version of this workflow died on that import). For Play see the credential
// order in `scripts/play-rollout.mjs`. Nothing secret is printed.
// Without readable credentials either half prints one SKIPPED line and the
// public half still decides the exit code; the workflow that calls this treats
// the missing Apple secret as a failure of its own, so a scheduled run cannot
// look green while checking nothing.
//
//   node scripts/check-store-listing.mjs                       human output
//   node scripts/check-store-listing.mjs --json                machine-readable
//   node scripts/check-store-listing.mjs --fail-on-unlisted    explicit at a call site
//   node scripts/check-store-listing.mjs --require-full-rollout  a staged Play rollout fails
//   node scripts/check-store-listing.mjs --check               accepted no-op (see below)
//
// Exit codes: 0 the app is for sale (and fully rolled out, under the flag), 1 it
// is not (a lookup that finds nothing, an availability record that does not
// exist, or — under the flag — a Play rollout that does not cover everyone), 2
// the check could not run. A verdict outranks a check that could not run, so a
// finding is never masked by an unrelated failure.
import { writeSync } from 'node:fs';
import { asc, ascToken } from './lib/asc.mjs';
import { playCredentialsAvailable, readProductionTrack } from './play-rollout.mjs';

// The iPhone app is the product: a released version of it that nobody can
// install is the failure this script exists for. The Mac app is watched too,
// but it is not on the Mac App Store by decision (D6: it ships a notarised
// DMG), so a missing availability record there is reported, never failed.
// Promote that one branch to a failure on the day the Mac store build ships.
const IPHONE = { id: '6827931747', label: 'iPhone' };
const MAC = { id: '6827932703', label: 'Mac' };
const LOOKUP = `https://itunes.apple.com/lookup?id=${IPHONE.id}`;
const PAGES = [
  `https://apps.apple.com/app/id${IPHONE.id}`,
  `https://apps.apple.com/app/id${MAC.id}`,
  'https://play.google.com/store/apps/details?id=app.vitals',
];
// The storefronts answer a request without a browser User-Agent with a 403 or a
// consent interstitial, which would read as "the page is down". They also
// rate-limit, which is why a page status is a warning and never a verdict.
const USER_AGENT =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36';
const TIMEOUT_MS = 20_000;

// Every option this script answers to. An unknown one is refused rather than
// ignored: `--require-full-rollou` (one letter short) would otherwise pass a
// release gate silently — the same "every gate stays green" failure this script
// exists to prevent. `--check` is listed for symmetry with
// `scripts/ios-store.mjs --check` and because docs/releasing.md documents that
// invocation; this script has no apply mode, so it is a no-op.
const KNOWN_FLAGS = new Set(['--json', '--fail-on-unlisted', '--require-full-rollout', '--check']);
const unknownFlags = process.argv
  .slice(2)
  .filter((argument) => argument.startsWith('-') && !KNOWN_FLAGS.has(argument));
if (unknownFlags.length > 0) {
  // writeSync, not console.error: process.exit() can drop output that has not
  // reached the OS yet when stdout/stderr is a pipe or a file — the trap the
  // note below records. This is the one place the script does exit early.
  writeSync(
    2,
    `unknown option(s): ${unknownFlags.join(', ')}\nknown options: ${[...KNOWN_FLAGS].join(', ')}\n`,
  );
  process.exit(2);
}

const json = process.argv.includes('--json');
// The default already fails on a missing availability record; the flag exists
// so a release step states that reliance at its call site rather than
// inheriting it. It changes no outcome, and the result echoes it so a reader of
// the JSON can see which policy was asked for.
const failOnUnlisted = process.argv.includes('--fail-on-unlisted');
// Off by default: a staged production rollout is normal while a release is
// being watched, so it is only a warning unless a call site demands the rollout
// have finished. `release.yml` and `store-listing.yml` pass it so a release
// cannot go green while the app reaches a fifth of users.
const requireFullRollout = process.argv.includes('--require-full-rollout');

// Human lines are suppressed under --json so stdout stays parseable.
const say = (line) => {
  if (!json) console.log(line);
};

// process.exit() truncates stdout that has not reached the OS yet when stdout
// is a pipe or a file, which is how CI — and this script's own proofs — capture
// it. Nothing here exits early, so the exitCode is set at the end and the
// process ends by itself, after the writes have drained.

// Reads the key id, the issuer and the .p8 — no network. A throw means the
// credentials are absent, not that the store is broken.
let hasCredentials = true;
try {
  ascToken();
} catch {
  hasCredentials = false;
}

// The client returns the raw response (status + parsed JSON) and throws only on
// a transport failure, which is a "could not run", never a verdict.
const request = async (path) => {
  try {
    return await asc('GET', path);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return { status: 0, json: {}, error: message };
  }
};

const failures = [];
const warnings = [];
const couldNotRun = [];
const versions = [];
const macVersions = [];
const pages = [];

// Layer 1, no credential involved. While the app was unlisted this answered
// HTTP 200 with `resultCount: 0`, so the status alone proves nothing.
const lookup = { status: null, resultCount: null, name: null };
try {
  const response = await fetch(LOOKUP, {
    headers: { 'user-agent': USER_AGENT, accept: 'application/json' },
    signal: AbortSignal.timeout(TIMEOUT_MS),
  });
  lookup.status = response.status;
  const body = response.status === 200 ? await response.json() : {};
  lookup.resultCount = body.resultCount ?? null;
  lookup.name = body.results?.[0]?.trackName ?? null;
  if (response.status !== 200) {
    couldNotRun.push(`the App Store lookup answered HTTP ${response.status}`);
    say(`lookup: HTTP ${response.status} (could not be read)`);
  } else if (lookup.resultCount === 0) {
    failures.push(
      `the App Store lookup finds no app for id ${IPHONE.id}, so nobody can install it`,
    );
    say('lookup: 0 results — the app is not listed');
  } else {
    say(`lookup: ${lookup.resultCount} result(s), ${lookup.name ?? 'unnamed'}`);
  }
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  couldNotRun.push(`the App Store lookup could not be fetched: ${message}`);
  say('lookup: could not be fetched');
}

for (const url of PAGES) {
  const page = { url, status: null, ok: false };
  try {
    const response = await fetch(url, {
      headers: { 'user-agent': USER_AGENT, 'accept-language': 'en-US,en;q=0.9' },
      redirect: 'follow',
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    page.status = response.status;
    page.ok = response.status === 200;
    if (page.ok) {
      say(`page ${url}: 200`);
    } else {
      // Deliberately not a failure: a storefront page can be briefly
      // unavailable or rate-limited, and the checks above are the verdict.
      warnings.push(`${url} answered HTTP ${response.status} — a warning, not a failure`);
      say(`page ${url}: HTTP ${response.status} (WARNING, not a failure)`);
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    warnings.push(`${url} could not be fetched (${message}) — a warning, not a failure`);
    say(`page ${url}: could not be fetched (WARNING, not a failure)`);
  }
  pages.push(page);
}

// What the store itself believes it has, so a human reads the state rather than
// guessing it from the version in the repository.
const listVersions = async (app, into) => {
  const response = await request(`/v1/apps/${app.id}/appStoreVersions?limit=10`);
  const prefix = app.label.toLowerCase();
  if (response.status === 0) {
    couldNotRun.push(`${prefix} appStoreVersions: ${response.error}`);
    say(`${prefix} versions: could not be read`);
    return;
  }
  if (response.status !== 200) {
    couldNotRun.push(`${prefix} appStoreVersions returned HTTP ${response.status}`);
    say(`${prefix} versions: HTTP ${response.status} (could not be read)`);
    return;
  }
  for (const v of response.json.data ?? []) {
    into.push({
      version: v.attributes?.versionString ?? null,
      state: v.attributes?.appStoreState ?? null,
    });
  }
  const listed = into.map((v) => `${v.version} ${v.state}`).join(', ');
  say(`${prefix} versions: ${listed || 'none'}`);
};

const availability = { status: null, availableInNewTerritories: null, territories: null };
const macAvailability = { status: null };

if (!hasCredentials) {
  say('SKIPPED: no App Store Connect credentials');
} else {
  // The territory count comes from the relationship's own paging total, so one
  // request answers both "is there a record" and "does it cover anywhere".
  const response = await request(
    `/v1/apps/${IPHONE.id}/appAvailabilityV2?include=territoryAvailabilities&limit[territoryAvailabilities]=1`,
  );
  availability.status = response.status;
  if (response.status === 0) {
    couldNotRun.push(`appAvailabilityV2: ${response.error}`);
    say('availability: could not be read');
  } else if (response.status === 404) {
    const detail = `appAvailabilityV2 returned 404: app ${IPHONE.id} has NO availability record, so nobody can install it`;
    failures.push(detail);
    say('availability: 404 — no availability record, so the app is not for sale');
  } else if (response.status !== 200) {
    couldNotRun.push(`appAvailabilityV2 returned HTTP ${response.status}`);
    say(`availability: HTTP ${response.status} (could not be read)`);
  } else {
    const data = response.json.data ?? {};
    availability.availableInNewTerritories = data.attributes?.availableInNewTerritories ?? null;
    availability.territories =
      data.relationships?.territoryAvailabilities?.meta?.paging?.total ?? null;
    say(
      `availability: 200 availableInNewTerritories=${availability.availableInNewTerritories} ` +
        `territories=${availability.territories}`,
    );
    if (availability.territories === 0) {
      warnings.push('the availability record lists no territory, so the app is for sale nowhere');
      say('availability: 0 territories (WARNING, not a failure)');
    }
  }

  await listVersions(IPHONE, versions);

  // Reported, never failed: see the D6 note on MAC above.
  const macResponse = await request(`/v1/apps/${MAC.id}/appAvailabilityV2`);
  macAvailability.status = macResponse.status;
  if (macResponse.status === 0) {
    couldNotRun.push(`mac appAvailabilityV2: ${macResponse.error}`);
    say('mac availability: could not be read');
  } else if (macResponse.status === 404) {
    warnings.push(
      `app ${MAC.id} (${MAC.label}) has no availability record — expected while it is not on the Mac App Store (D6)`,
    );
    say(`mac availability: 404 — no record (expected: ${MAC.label} ships a notarised DMG, D6)`);
  } else if (macResponse.status !== 200) {
    couldNotRun.push(`mac appAvailabilityV2 returned HTTP ${macResponse.status}`);
    say(`mac availability: HTTP ${macResponse.status} (could not be read)`);
  } else {
    say(
      `mac availability: 200 availableInNewTerritories=${macResponse.json.data?.attributes?.availableInNewTerritories}`,
    );
  }

  await listVersions(MAC, macVersions);
}

// The Play half: what the production track is actually serving. Independent of
// the Apple credentials above — a repository can have one store's key and not
// the other's — so it decides only its own lines, and a missing Play credential
// is SKIPPED exactly like a missing Apple one.
const play = { credentials: false, releases: [], verdict: null, trackMissing: null };
if (!playCredentialsAvailable()) {
  say('SKIPPED: no Google Play credentials');
} else {
  play.credentials = true;
  try {
    const { releases, verdict, trackMissing } = await readProductionTrack();
    play.releases = releases;
    play.verdict = verdict;
    play.trackMissing = trackMissing;
    for (const release of releases) {
      // Play omits userFraction for a full rollout, so print that as "full"
      // rather than leaving the reader to guess what an absent number means.
      const fraction = release.userFraction === null ? 'full' : String(release.userFraction);
      say(
        `play production: ${release.name ?? 'unnamed'} ` +
          `[versionCodes ${release.versionCodes.join(',') || 'none'}] ` +
          `status=${release.status} userFraction=${fraction}`,
      );
    }
    if (releases.length === 0) say('play production: no release on the track');
    if (verdict.full) {
      say(`play rollout: ${verdict.summary}`);
    } else if (requireFullRollout) {
      failures.push(`the Play production rollout does not cover everyone: ${verdict.summary}`);
      say(`play rollout: FAIL — ${verdict.summary}`);
    } else {
      warnings.push(`the Play production rollout does not cover everyone: ${verdict.summary}`);
      say(
        `play rollout: WARNING — ${verdict.summary} (not a failure without --require-full-rollout)`,
      );
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    couldNotRun.push(`the Play production track could not be read: ${message}`);
    say('play rollout: could not be read');
  }
}

const result = {
  checkedAt: new Date().toISOString(),
  appId: IPHONE.id,
  failOnUnlisted,
  requireFullRollout,
  credentials: hasCredentials,
  ok: failures.length === 0 && couldNotRun.length === 0,
  lookup,
  availability,
  versions,
  macAvailability,
  macVersions,
  play,
  pages,
  failures,
  warnings,
  couldNotRun,
};

for (const failure of failures) say(`FAIL ${failure}`);
for (const error of couldNotRun) say(`ERROR ${error}`);
for (const warning of warnings) say(`WARN ${warning}`);
// Only what was proven: a storefront page can be a warning, so the success line
// cannot claim every page answered.
const note = warnings.length ? ` (${warnings.length} warning(s))` : '';
if (result.ok) say(`OK: app ${IPHONE.id} is for sale${note}`);

if (json) console.log(JSON.stringify(result, null, 2));
if (failures.length > 0) process.exitCode = 1;
else if (couldNotRun.length > 0) process.exitCode = 2;
