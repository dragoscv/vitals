// Proves the Apple half of a release is genuinely held.
//
// Vitals ships one version number in six manifests, but the two App Store
// listing files lag it on purpose. scripts/ios-store.mjs applies a listing to
// whichever version is EDITABLE in App Store Connect, and while a version sits
// in review that editable record is the one being reviewed. So the listing
// text must describe the version under review, not the one the repository has
// already been bumped to: if 1.3.1's "What's new" lands on the 1.3.0 record
// Apple is reading, the store advertises a version nobody can have. On
// 2026-11-20 the 1.3.1 release was cut everywhere except these two files,
// which were left describing 1.3.0 (iPhone) and 1.2.0 (Mac) — a deliberate
// hold that lived only in a handoff note until this guard.
//
// What is checked, offline: the version a listing carries must be strictly
// BEHIND the version the repository is being built at, taken from the
// per-platform manifest — apps/ios/project.yml MARKETING_VERSION for the
// iPhone listing, apps/desktop/src-tauri/tauri.conf.json for the Mac one.
// Equality fails: while a submission is held, a listing naming the
// repository's own version can only have been moved by a well-meaning ripple
// pass, and its text would be applied to the record still under review. This
// is a HOLD assertion, not a permanent rule: the day the new version is
// genuinely submitted to Apple the listing moves with it, and the expectation
// here is re-decided on purpose.
//
// The version under review is NOT recoverable offline — App Store Connect is
// the only place that records it — so this proves the relationship (behind),
// not the absolute number. That is stated rather than guessed.
//
//   node scripts/check-apple-hold.mjs          human output, exit 1 when moved
//   node scripts/check-apple-hold.mjs --json    the same verdict, machine-readable
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const readText = (rel) => readFileSync(join(root, rel), 'utf8');

// In --json mode stdout carries the report and nothing else, so a consumer can
// parse it; the human lines go through log and are suppressed.
const asJson = process.argv.includes('--json');
const log = (...a) => {
  if (!asJson) console.log(...a);
};
const emit = (...a) => console.log(...a);

const SEMVER = /\b\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?\b/g;
const versionsIn = (text) =>
  typeof text === 'string' ? [...text.matchAll(SEMVER)].map((m) => m[0]) : [];

// A plain three-part SemVer compare: the core triple numerically, and a
// pre-release below the release it precedes. Enough for the versions this
// repository ships, and it never throws on a shape it does not expect.
const compare = (a, b) => {
  const [ac, apre] = a.split('-');
  const [bc, bpre] = b.split('-');
  const an = ac.split('.').map(Number);
  const bn = bc.split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    if (an[i] !== bn[i]) return an[i] < bn[i] ? -1 : 1;
  }
  if (apre && !bpre) return -1;
  if (!apre && bpre) return 1;
  return 0;
};

const newest = (versions) =>
  versions.reduce((hi, v) => (hi === null || compare(v, hi) > 0 ? v : hi), null);

// The version each platform is being built at. project.yml is where the iOS
// marketing version genuinely lives (scripts/version.ps1 writes it there and
// nothing else); tauri.conf.json is the desktop app's own version surface.
const parseIosApp = () => {
  const m = readText('apps/ios/project.yml').match(
    /^\s*MARKETING_VERSION:\s*'?([^'\r\n#]+?)'?\s*$/m,
  );
  return m ? m[1].trim() : null;
};

const LISTINGS = [
  {
    name: 'iPhone',
    file: 'apps/ios/store/listing.json',
    app: parseIosApp(),
    // The iPhone listing carries no versionString; its version lives only in
    // the "New in X" prose, one per locale.
    carriedBy: (spec) => Object.values(spec.locales ?? {}).flatMap((l) => versionsIn(l.whatsNew)),
  },
  {
    name: 'Mac',
    file: 'apps/desktop/store/macos-listing.json',
    app: JSON.parse(readText('apps/desktop/src-tauri/tauri.conf.json')).version,
    // The Mac listing's versionString is what ios-store.mjs PATCHes onto the
    // record, so it is the authoritative one; the prose is checked to agree.
    carriedBy: (spec) => [
      ...(typeof spec.versionString === 'string' ? [spec.versionString] : []),
      ...Object.values(spec.locales ?? {}).flatMap((l) => versionsIn(l.whatsNew)),
    ],
  },
];

const risk =
  'scripts/ios-store.mjs would PATCH this text (including whatsNew) onto the ' +
  'record still under review, so the store would describe a version Apple is ' +
  'not reading';

const verdicts = LISTINGS.map((listing) => {
  const base = { name: listing.name, file: listing.file, app: listing.app };
  let spec;
  try {
    spec = JSON.parse(readText(listing.file));
  } catch (error) {
    return { ...base, carried: null, state: 'unreadable', ok: false, detail: `${error.message}` };
  }

  const candidates = [...new Set(listing.carriedBy(spec))];
  const carried = newest(candidates);

  if (carried === null) {
    return {
      ...base,
      carried: null,
      state: 'none',
      ok: true,
      detail: 'names no version, so there is nothing to hold',
    };
  }
  if (candidates.length > 1) {
    return {
      ...base,
      carried,
      state: 'mixed',
      ok: false,
      detail: `names more than one version (${candidates.join(', ')}): a partial ripple — ${risk}`,
    };
  }
  if (!listing.app) {
    return {
      ...base,
      carried,
      state: 'unknown-app',
      ok: false,
      detail: 'the app version could not be read, so the hold cannot be proven',
    };
  }
  if (compare(carried, listing.app) < 0) {
    return {
      ...base,
      carried,
      state: 'held',
      ok: true,
      detail: `describes ${carried} while the app is at ${listing.app}; held for the version App Store Connect is still reviewing`,
    };
  }
  return {
    ...base,
    carried,
    state: 'moved',
    ok: false,
    detail: `names ${carried}, which is not behind the app version ${listing.app}; ${risk}`,
  };
});

const ok = verdicts.every((v) => v.ok);

if (asJson) {
  emit(
    JSON.stringify(
      { ok, appVersions: { iPhone: LISTINGS[0].app, Mac: LISTINGS[1].app }, listings: verdicts },
      null,
      2,
    ),
  );
} else {
  for (const v of verdicts) {
    const tag = v.state === 'held' ? 'held' : v.ok ? v.state : 'MOVED';
    log(`apple hold: ${tag.padEnd(5)} ${v.file} — ${v.detail}`);
  }
  log(
    ok
      ? 'apple hold: OK — every listing is behind the version the app is being built at'
      : 'apple hold: FAILED — the Apple submission is no longer held',
  );
}

process.exit(ok ? 0 : 1);
