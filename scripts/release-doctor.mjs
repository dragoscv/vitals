#!/usr/bin/env node
// release-doctor — the rule that a release must update EVERY surface that names
// it, as a command.
//
// The rule lives in AGENTS.md as prose ("A release updates EVERY surface that
// names or describes it"), and the way to obey it is to grep for the old
// version before calling a release done. Prose has not held: measured on
// another project, of 309 commits adding a feature only 6 % of them reached the
// TypeScript SDK, and here it has bitten twice — the MSIX script's usage example
// stayed at 1.3.0.0 after the bump, and the Microsoft Store 0.7.1 listing went
// live with release notes still describing 0.7.0. A rule checked by memory is
// checked only when someone remembers. This is the command.
//
// Why it is shaped the way it is:
//
//   * Everything it reads comes from the repository, so it runs in a pre-commit
//     hook and in CI with no network and no credential. The two surfaces that
//     genuinely need a live store (the App Store Connect availability record and
//     the Play production track) are reported UNKNOWN rather than skipped or
//     guessed; `scripts/check-store-listing.mjs` is what reads those, with a
//     credential, and the exit code here says whether this run was a full check
//     or a partial one.
//   * The manifests are NOT re-parsed here. `scripts/version.ps1 -Check` is the
//     existing source of truth for the six places the version lives (the Cargo
//     workspace, the root and apps/packages package.json files,
//     tauri.conf.json, the XcodeGen spec apps/ios/project.yml and the Tizen
//     widget manifest), and it is re-used rather than re-implemented so the two
//     cannot drift.
//   * Version literals in scripts/, packaging/ and .github/workflows/ are NOT
//     re-derived here. That rule already has exactly one owner —
//     scripts/check-drift.ps1 check 10, which the pre-commit hook and CI run —
//     so this row invokes that gate and reports its verdict. This row used to
//     implement the rule a second time, more loosely, and so flagged four
//     surfaces the gate deliberately permits: the gate's own note enumerating
//     what it does NOT flag, an installer-roundtrip .EXAMPLE that deliberately
//     shows two DIFFERENT versions, and two tool examples whose `-Version`
//     accepts any version. Two implementations of one rule disagree by
//     construction, and a doctor that contradicts the gate it complements is
//     read as noise until nobody looks.
//
// Exit codes (a run always prints the table first, then its findings):
//
//   0          full check, every surface OK
//   1 .. 99    full check, N surfaces STALE
//   100        partial run (a surface needed a credential or the network),
//              every surface that could be read is OK
//   101 .. 199 partial run, N surfaces STALE
//
//   node scripts/release-doctor.mjs                     version from the manifests, tag v<version>
//   node scripts/release-doctor.mjs --version 1.3.1     check a specific release
//   node scripts/release-doctor.mjs --offline           skip the one network call
//                                                       (git ls-remote); the tag
//                                                       push check reads the local
//                                                       remote-tracking ref instead
//   node scripts/release-doctor.mjs --tag-tree          only the tag-vs-main tree
//                                                       check, for the release
//                                                       workflow's Plan job
//   node scripts/release-doctor.mjs --winget-token      only the winget fork probe,
//                                                       which needs one API call
//
// It is dependency-free on purpose: Node's standard library, `git` and the
// repository's own scripts, nothing else. The winget probe uses the
// standard-library `fetch`, so it adds no dependency either.

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = dirname(dirname(fileURLToPath(import.meta.url)));

// ── arguments ──────────────────────────────────────────────────────────────

const argv = process.argv.slice(2);
function flagValue(name) {
  const at = argv.indexOf(name);
  if (at >= 0 && at + 1 < argv.length) return argv[at + 1];
  const prefixed = argv.find((a) => a.startsWith(`${name}=`));
  return prefixed ? prefixed.slice(name.length + 1) : null;
}
const offline = argv.includes('--offline');
// Two focused modes the release workflow's Plan job drives: the tag-vs-main
// tree check on its own (so the Plan job can fail fast on a stale tag without
// running, and possibly failing on, every other surface), and the winget fork
// probe on its own (which needs a token and one network call the ordinary run
// must not make). Both keep the same table, findings and exit-code contract.
const onlyTagTree = argv.includes('--tag-tree');
const onlyWinget = argv.includes('--winget-token');
const knownFlags = ['--version', '--offline', '--tag-tree', '--winget-token'];
const unknownArgs = argv.filter(
  (a) => a.startsWith('-') && !knownFlags.includes(a) && !a.startsWith('--version='),
);
if (unknownArgs.length > 0) {
  // writeSync, not console.error: process.exit() can drop output that has not
  // reached the OS yet when stdout/stderr is a pipe or a file — the trap that
  // made this script print an empty table on its first run.
  writeSync(2, `unknown option(s): ${unknownArgs.join(', ')}\nknown: ${knownFlags.join(', ')}\n`);
  process.exit(2);
}

const read = (rel) => readFileSync(join(root, rel), 'utf8');
const exists = (rel) => existsSync(join(root, rel));

function git(args) {
  return execFileSync('git', args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

// ── the version under check ────────────────────────────────────────────────

// With no --version, the version is read from the manifests and the tag is
// `v<version>`, matching how docs/releasing.md tags a release.
function manifestVersion() {
  return JSON.parse(read('package.json')).version;
}
const rawVersion = (flagValue('--version') ?? manifestVersion()).replace(/^v/, '');
// A four-part Store version is the three-part one with a trailing .0.
const fourPart = (() => {
  const m = rawVersion.match(/^(\d+)\.(\d+)\.(\d+)(?:\.(\d+))?/);
  if (!m) return rawVersion;
  return m[4] ? `${m[1]}.${m[2]}.${m[3]}.${m[4]}` : `${m[1]}.${m[2]}.${m[3]}.0`;
})();
const tag = `v${rawVersion}`;
// A literal counts as current when it is either shape: 1.3.1 or 1.3.1.0.
const currentLiterals = new Set([rawVersion, fourPart]);

// ── shared readers ─────────────────────────────────────────────────────────

function cmpSemver(a, b) {
  const parse = (v) => {
    const m = v.match(/^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?/);
    return m ? { n: [Number(m[1]), Number(m[2]), Number(m[3])], pre: m[4] ?? null } : null;
  };
  const x = parse(a);
  const y = parse(b);
  if (!x || !y) return 0;
  for (let i = 0; i < 3; i += 1) if (x.n[i] !== y.n[i]) return x.n[i] < y.n[i] ? -1 : 1;
  if (x.pre === y.pre) return 0;
  if (x.pre === null) return 1; // a release is newer than a prerelease
  if (y.pre === null) return -1;
  return x.pre < y.pre ? -1 : 1;
}

// Every version CHANGELOG.md records and every `## [x]` heading, so "older than
// the current release" can be told from "a version that never shipped".
function changelogHeadings() {
  const text = read('CHANGELOG.md');
  const out = [];
  const re = /^##\s+\[([^\]]+)\]\s*[—–-]\s*(\d{4}-\d{2}-\d{2})?\s*$/gm;
  let m;
  while ((m = re.exec(text)) !== null) out.push({ version: m[1], date: m[2] ?? null });
  return out;
}
const headings = changelogHeadings();
const released = new Set(headings.map((h) => h.version).filter((v) => v !== 'Unreleased'));

// ── surfaces ───────────────────────────────────────────────────────────────

const rows = [];
const details = [];
const row = (surface, expected, found, status) => rows.push({ surface, expected, found, status });
const ok = (s, e, f) => row(s, e, f, 'OK');
const stale = (s, e, f) => {
  row(s, e, f, 'STALE');
  if (f) details.push(`  ${s}: ${f}`);
};
const held = (s, e, f) => row(s, e, f, 'HELD');
const unknown = (s, e, f) => row(s, e, f, 'UNKNOWN');
// SKIP is a surface the run deliberately did not read — a missing file, no
// credential, an unreachable API. It is reported, never OK, and it does not
// move the exit code, which is the whole point: an API nobody could reach is
// not a bad credential, and a surface nobody read is not a surface that passed.
const skip = (s, e, f) => row(s, e, f, 'SKIP');

// 1. version manifests — re-uses scripts/version.ps1 -Check (the source of
// truth) rather than re-implementing the six places the version lives.
function checkManifests() {
  const shell = 'pwsh';
  let result;
  try {
    result = execFileSync(
      shell,
      ['-NoProfile', '-File', join(root, 'scripts', 'version.ps1'), '-Check', tag],
      { cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] },
    );
  } catch (error) {
    const say = `${error.stdout ?? ''}${error.stderr ?? ''}`.trim().split('\n').filter(Boolean);
    stale(
      'manifests (version.ps1 -Check)',
      rawVersion,
      say.slice(-1)[0] ?? 'version.ps1 -Check failed',
    );
    return;
  }
  const pkg = JSON.parse(read('package.json')).version;
  ok('manifests (version.ps1 -Check)', rawVersion, result.trim() || `every manifest says ${pkg}`);
}

// 2. the CHANGELOG section for the version: present, dated, not empty.
function checkChangelog() {
  const text = read('CHANGELOG.md');
  const heading = new RegExp(
    `^##\\s+\\[${rawVersion.replace(/\./g, '\\.')}\\]\\s*[—–-]\\s*(\\d{4}-\\d{2}-\\d{2})`,
    'm',
  );
  const m = text.match(heading);
  if (!m) {
    stale('CHANGELOG.md section', `[${rawVersion}] dated`, 'no dated heading for this version');
    return;
  }
  const start = m.index + m[0].length;
  const rest = text.slice(start);
  const next = rest.search(/^##\s+\[/m);
  const body = (next >= 0 ? rest.slice(0, next) : rest).trim();
  if (body.length === 0) {
    stale('CHANGELOG.md section', `[${rawVersion}] dated`, `dated ${m[1]} but empty`);
    return;
  }
  const blocks = (body.match(/^###\s+/gm) ?? []).length;
  ok('CHANGELOG.md section', `[${rawVersion}] dated`, `dated ${m[1]}, ${blocks} block(s)`);
}

// 3. version literals that name Vitals' own version in scripts/, packaging/
// and .github/workflows/. This class is owned by exactly one implementation —
// scripts/check-drift.ps1 check 10: a FOUR-part literal bound to a version slot
// (`-Version <v>` or `version=<v>`) that must equal `<version>.0` — and the
// pre-commit hook and CI run it. So this row invokes that gate and reports its
// verdict rather than inventing a second answer; the four surfaces the old
// scan flagged were all things the gate permits on purpose, and the two would
// have disagreed forever. See the header for the full reason.
//
// check-drift.ps1 is a whole-gate script with no per-check switch, so its exit
// code counts failures from every check. Only its "is not the current Store
// version" lines belong to this row, so those are the lines read; a red gate
// from another check makes the row say so rather than misattribute the failure.
const DRIFT_SUMMARY = /drift:\s*(\d+)\s+failure\(s\),\s*(\d+)\s+warning\(s\)/;
const DRIFT_VERSION_FAIL =
  /FAIL\s+(\S+):(\d+)\s+version\s+(\S+)\s+is not the current Store version/;

function checkVersionLiterals() {
  const expected = `${fourPart} (check-drift.ps1 check 10)`;
  let text;
  try {
    text = String(
      execFileSync('pwsh', ['-NoProfile', '-File', join(root, 'scripts', 'check-drift.ps1')], {
        cwd: root,
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
      }),
    );
  } catch (error) {
    // A non-zero exit is the normal failure path, not an absence: the gate
    // exits with the failure count, and its output still arrives on the error.
    text = `${error.stdout ?? ''}${error.stderr ?? ''}`;
  }
  // The summary line is printed on every run, clean or red, so its absence
  // means the gate did not run at all — an absent measurement, never a zero.
  const summary = text.match(DRIFT_SUMMARY);
  if (!summary) {
    unknown(
      'version literals (scripts, packaging, CI)',
      expected,
      'check-drift.ps1 produced no summary (not run, or not installed)',
    );
    return;
  }
  const hits = [];
  text.split(/\r?\n/).forEach((line) => {
    const m = line.match(DRIFT_VERSION_FAIL);
    if (m) hits.push({ where: `${m[1]}:${m[2]}`, literal: m[3] });
  });
  if (hits.length === 0) {
    // The gate's verdict for this class is clean. If the gate is red on its
    // other checks, say so here instead of letting the reader assume all clear.
    const otherFailures = Number(summary[1]);
    ok(
      'version literals (scripts, packaging, CI)',
      expected,
      otherFailures > 0
        ? `check-drift clean on this class, ${otherFailures} unrelated failure(s)`
        : 'check-drift.ps1 check 10 clean',
    );
    return;
  }
  const summaryLine = [...new Set(hits.map((h) => h.literal))].sort().join(', ');
  stale('version literals (scripts, packaging, CI)', expected, summaryLine);
  hits
    .sort((a, b) => (a.where < b.where ? -1 : a.where > b.where ? 1 : 0))
    .forEach((h) => details.push(`  version literal ${h.literal} at ${h.where}`));
}

// 4. the MSIX (Microsoft Store) what's-new strings, both locales.
// The hand-written store texts, read through one pair of readers so the
// per-surface rows (#4, #5) and the cross-surface consistency row (#5b) cannot
// drift about which file names what or how its version is spelled.
const MSIX_LISTING = [
  ['packaging/msix/listing/en-us.md', /What's new in (\d+\.\d+\.\d+(?:\.\d+)?)/],
  ['packaging/msix/listing/ro-ro.md', /Ce e nou în (\d+\.\d+\.\d+(?:\.\d+)?)/],
];
const PLAY_CHANGELOG = [
  'apps/android/play/listing/en-US/changelogs/default.txt',
  'apps/android/play/listing/ro/changelogs/default.txt',
];

function msixWhatsNewVersions() {
  return MSIX_LISTING.map(([file, re]) => {
    if (!exists(file)) return { file, version: null, reason: 'file missing' };
    const m = read(file).match(re);
    return {
      file,
      version: m ? m[1] : null,
      reason: m ? null : 'no "what\'s new" version found',
    };
  });
}

function playChangelogVersions() {
  return PLAY_CHANGELOG.map((file) => {
    if (!exists(file)) return { file, version: null, reason: 'file missing' };
    const first =
      read(file)
        .split(/\r?\n/)
        .find((l) => l.trim().length > 0) ?? '';
    const m = first.match(/Vitals\s+v?(\d+\.\d+\.\d+(?:\.\d+)?)/);
    return {
      file,
      version: m ? m[1] : null,
      reason: m ? null : 'no "Vitals <version>" line found',
    };
  });
}

// 4. the MSIX (Microsoft Store) what's-new strings, both locales.
function checkMsixWhatsNew() {
  const named = msixWhatsNewVersions();
  const missing = named.find((n) => n.reason === 'file missing');
  if (missing) {
    stale("MSIX what's-new (en-us, ro-ro)", rawVersion, `${missing.file} missing`);
    return;
  }
  const wrong = named.filter((n) => n.version !== rawVersion);
  if (wrong.length === 0) {
    ok("MSIX what's-new (en-us, ro-ro)", rawVersion, `both list ${rawVersion}`);
    return;
  }
  stale(
    "MSIX what's-new (en-us, ro-ro)",
    rawVersion,
    wrong.map((n) => `${n.file} says ${n.version ?? 'nothing'}`).join(', '),
  );
}

// 5. the Play (Android) changelogs, both locales.
function checkPlayChangelog() {
  const named = playChangelogVersions();
  const missing = named.find((n) => n.reason === 'file missing');
  if (missing) {
    stale('Play changelog (en-US, ro)', rawVersion, `${missing.file} missing`);
    return;
  }
  const wrong = named.filter((n) => n.version !== rawVersion);
  if (wrong.length === 0) {
    ok('Play changelog (en-US, ro)', rawVersion, `both name ${rawVersion}`);
    return;
  }
  stale(
    'Play changelog (en-US, ro)',
    rawVersion,
    wrong.map((n) => `${n.file} says ${n.version ?? 'nothing'}`).join(', '),
  );
}

// The version the Store package's AppxManifest will declare. The manifest is
// built from AppxManifest.template.xml, whose Identity Version is filled at
// build time from the four-part Store version the packaging holds — there is no
// committed concrete manifest to read, because build-msix.ps1 fills the template
// and the result is a build artefact. So the declared version is the four-part
// literal in packaging/msix and the msix workflow (the one
// scripts/check-drift.ps1 keeps at the Store form), and that is what this reads.
function msixManifestVersion() {
  const direct = 'packaging/msix/AppxManifest.template.xml';
  if (exists(direct)) {
    // The negative lookbehind keeps MinVersion / MaxVersionTested out: they end
    // in `Version="..."` too, and reading one of those as the release version is
    // exactly the sort of silent wrong answer this row exists to prevent.
    const m = read(direct).match(/(?<![A-Za-z])Version="(\d+\.\d+\.\d+\.\d+)"/);
    if (m) return { version: m[1] };
  }
  const sources = [
    ['packaging/msix/build-msix.ps1', /-Version\s+(\d+\.\d+\.\d+\.\d+)/],
    ['packaging/msix/build-and-upload.ps1', /-Version\s+(\d+\.\d+\.\d+\.\d+)/],
    ['.github/workflows/msix.yml', /version=(\d+\.\d+\.\d+\.\d+)/],
  ];
  for (const [file, re] of sources) {
    if (!exists(file)) continue;
    const m = read(file).match(re);
    if (m) return { version: m[1] };
  }
  return { version: null, reason: 'no declared four-part Store version in the packaging' };
}

// A Store version may be three- or four-part; the Store's last part is always 0,
// so 1.5.1 and 1.5.1.0 name the same release.
function sameVersion(a, b) {
  const core = (v) => (v.match(/^\d+\.\d+\.\d+/) ?? [v])[0];
  return core(a) === core(b);
}

// 5b. the store surfaces held to one version. A release reaches a user through
// three store-facing surfaces, each naming a version on its own: the Store
// package's AppxManifest, the Play release notes and the MSIX what-is-new block.
// Rows #4 and #5 check two of them against the manifests; nothing asked whether
// the stores agree with EACH OTHER, so a release where one surface was bumped
// and another was not could ship with the Store serving a different version from
// Play. This row asks the one question a reader has — will every store serve
// this version — and answers SKIP, never OK, when a surface cannot be read,
// because a surface nobody read is not a surface that passed.
function checkStoreConsistency() {
  const label = 'store consistency (the version every store surface names)';
  const expected = `every store surface names ${rawVersion}`;
  const groups = [
    ['MSIX manifest', [msixManifestVersion()]],
    ['Play changelog', playChangelogVersions()],
    ["MSIX what's-new", msixWhatsNewVersions()],
  ];
  const wrong = [];
  const unreadable = [];
  for (const [name, readings] of groups) {
    for (const r of readings) {
      if (r.version === null) {
        unreadable.push(r.file ? `${name} (${r.file}: ${r.reason})` : `${name} (${r.reason})`);
      } else if (!sameVersion(r.version, rawVersion)) {
        wrong.push(`${name} says ${r.version}`);
      }
    }
  }
  // A surface read from several files is named once, so both locales do not
  // repeat the same finding; rows #4 and #5 already name the individual files.
  if (wrong.length > 0) {
    stale(label, expected, [...new Set(wrong)].join(', '));
    return;
  }
  if (unreadable.length > 0) {
    skip(label, expected, unreadable.join('; '));
    return;
  }
  ok(label, expected, `every store surface names ${rawVersion}`);
}

// 6. the two prose surfaces that NAME the version rather than hold it: the API
// README's sample transcripts and the site's terms sentence, in both locales.
// No writer moves them — scripts/version.ps1 rewrites manifests and the
// four-part literals bound to a -Version argument, and neither of these is
// either — so the bump that moves the manifests leaves them behind. Both are
// read by a human, which is why nothing failed: the README's three examples
// were stale for three releases and the gate that caught them was a Rust test
// asserting the file against CARGO_PKG_VERSION, which is what turned the
// v1.5.1 tag's Verify job red after the tag was already pushed. This row is
// deliberately the same rule as that test, plus the sentence no test reads.
function checkNamedProse() {
  const label = 'version prose (api README, site terms)';
  const expected = `every named version is ${rawVersion}`;
  const sources = [
    ['docs/api/README.md', /"version"\s*:\s*"(\d+\.\d+\.\d+)"/g],
    ['apps/site/src/content/docs/terms.md', /current release is (\d+\.\d+\.\d+)/g],
    ['apps/site/src/content/docs/ro/terms.md', /versiunea curentă este (\d+\.\d+\.\d+)/g],
  ];
  const found = [];
  for (const [file, re] of sources) {
    if (!exists(file)) {
      stale(label, expected, `${file} missing`);
      return;
    }
    for (const m of read(file).matchAll(re)) found.push({ file, version: m[1] });
  }
  // A reworded sentence or a reformatted transcript would leave this walk
  // matching nothing, and "nothing found" is an absent measurement rather than
  // a clean one — the row must not pass for ever because it stopped reading.
  if (found.length === 0) {
    unknown(label, expected, 'no version example found in any of the three files');
    return;
  }
  const wrong = found.filter((f) => f.version !== rawVersion);
  if (wrong.length === 0) {
    ok(label, expected, `${found.length} example(s) name ${rawVersion}`);
    return;
  }
  stale(label, expected, wrong.map((f) => `${f.file} says ${f.version}`).join(', '));
}

// 7. the two Apple listing files. These are deliberately held at the version
// Apple is reviewing, and a second version cannot sit in review beside it, so a
// file that names an OLDER RELEASED version is HELD, with the reason in the row
// — not STALE. A file naming a version that never shipped, or one newer than
// this release, is STALE: that is a real error, not a hold.
function appleNamedVersions(file) {
  const json = JSON.parse(read(file));
  const found = [];
  const push = (s) => {
    for (const m of String(s ?? '').matchAll(/(\d+\.\d+\.\d+(?:\.\d+)?)/g)) found.push(m[1]);
  };
  if (typeof json.versionString === 'string') push(json.versionString);
  for (const locale of Object.values(json.locales ?? {})) push(locale?.whatsNew);
  return [...new Set(found)];
}
function checkApple(file, label) {
  if (!exists(file)) {
    stale(label, rawVersion, `${file} missing`);
    return;
  }
  const named = appleNamedVersions(file);
  if (named.length === 0) {
    unknown(label, rawVersion, `${file} names no version`);
    return;
  }
  if (named.every((v) => currentLiterals.has(v))) {
    ok(label, rawVersion, `names ${named.join(', ')}`);
    return;
  }
  const allHeld = named.every((v) => released.has(v) && cmpSemver(v, rawVersion) < 0);
  if (allHeld) {
    held(
      label,
      rawVersion,
      `held at ${named.join(', ')} (awaiting Apple review beside ${rawVersion})`,
    );
    return;
  }
  stale(label, rawVersion, `names ${named.join(', ')}`);
}

// 8. the tag: annotated, and its commit on origin.
function checkTag() {
  const expected = `${tag}, annotated, on origin`;
  let listed;
  try {
    listed = git(['tag', '-l', tag]).trim();
  } catch {
    unknown('git tag', expected, 'git unavailable');
    return;
  }
  if (listed === '') {
    stale('git tag', expected, `no tag ${tag}`);
    return;
  }
  const type = git(['cat-file', '-t', tag]).trim();
  if (type !== 'tag') {
    stale('git tag', expected, `${tag} is a lightweight tag`);
    return;
  }
  const commit = git(['rev-parse', `${tag}^{commit}`]).trim();

  // Whether the tag is on origin, and the tag's commit is on origin/main. The
  // authoritative answer needs the remote (`git ls-remote`); offline, the local
  // remote-tracking ref is the fallback and the row says so.
  let pushed = null;
  let onMain = null;
  let source = 'ls-remote';
  if (!offline) {
    try {
      const remoteTag = git(['ls-remote', '--tags', 'origin', `refs/tags/${tag}`]).trim();
      pushed = remoteTag.length > 0;
      if (pushed) {
        const mainLine = git(['ls-remote', 'origin', 'refs/heads/main']).trim();
        const remoteMain = mainLine.split(/\s+/)[0];
        if (remoteMain) {
          try {
            git(['merge-base', '--is-ancestor', commit, remoteMain]);
            onMain = true;
          } catch (error) {
            onMain = error.status === 1 ? false : null;
          }
        }
      }
    } catch {
      pushed = null;
    }
  }
  if (pushed === null) {
    source = 'local origin/main';
    const branches = git(['branch', '-r', '--contains', commit]).trim();
    onMain = branches.split('\n').some((b) => b.includes('origin/main'));
    pushed = onMain;
    if (!onMain) {
      unknown('git tag', expected, `${tag} exists but could not be compared with origin (offline)`);
      return;
    }
  }
  if (!pushed) {
    stale('git tag', expected, `${tag} exists but is not on origin`);
    return;
  }
  if (onMain === false) {
    stale('git tag', expected, `${tag} is on origin but its commit is not on origin/main`);
    return;
  }
  ok('git tag', expected, `${tag} -> ${commit.slice(0, 7)} (${source})`);
}

// 9. the release tag's tree against origin/main. The release builds the TAG's
// tree, not main's, so a gate-relevant fix that lands on main after the tag is
// silently absent from everything the gates do — the tag's own Verify job runs
// the old tree, fails, and the tag has to be re-pointed afterwards. That is
// exactly what happened on 2026-11-28: v1.5.1 pointed at 58923f5, two
// gate-relevant commits (f94be78, 41dd808) landed on main after it, the tag's
// Verify job went red on the old tree, and the tag was re-pointed at 41dd808.
// Nothing pre-flight warned that the tag was stale, so this row does.
//
// Everything read here is local — the tag, its peeled commit and origin/main
// come from the repository — so `--offline` costs nothing. HEAD is used only
// when origin/main is genuinely absent, and the row names the ref it compared.
// A missing tag, or no way to reach main at all, is UNKNOWN, never OK: an
// unverifiable claim is not a passing one.
const GATE_PATHS = [/^scripts\//, /^packaging\//, /^\.github\/workflows\//];

function checkTagTree() {
  const label = 'tag tree (the release tag carries the fixes)';
  const expected = `${tag} is not behind origin/main on the paths the gates read`;
  // Without the tag there is nothing to compare against main.
  let tagged;
  try {
    tagged = git(['rev-parse', '--verify', '--quiet', `${tag}^{commit}`]).trim();
  } catch {
    unknown(label, expected, `no tag ${tag} to compare against main`);
    return;
  }
  // The remote-tracking ref is enough — no fetch, no network. HEAD is a stated
  // fallback, taken only when origin/main is genuinely absent.
  let refName = 'origin/main';
  let base = '';
  try {
    base = git(['rev-parse', '--verify', '--quiet', 'origin/main']).trim();
  } catch {
    base = '';
  }
  if (!base) {
    try {
      base = git(['rev-parse', '--verify', '--quiet', 'HEAD']).trim();
      refName = 'HEAD (origin/main absent)';
    } catch {
      base = '';
    }
  }
  if (!base) {
    unknown(label, expected, 'no origin/main and no HEAD to compare against');
    return;
  }
  // Identical trees is the pass case ahead of the commit walk: the tag's tree
  // is exactly what main has, so no gate-relevant fix can be missing.
  const taggedTree = git(['rev-parse', `${tagged}^{tree}`]).trim();
  const baseTree = git(['rev-parse', `${base}^{tree}`]).trim();
  if (taggedTree === baseTree) {
    ok(label, expected, `${tag} -> ${tagged.slice(0, 7)} tree matches ${refName}`);
    return;
  }
  // Commits on the base the tag's history does not carry, with the paths each
  // touches. A commit touching a gate path (scripts/, packaging/,
  // .github/workflows/) is a fix the release gates will never see.
  let listed;
  try {
    listed = git(['rev-list', `${tagged}..${base}`]).trim();
  } catch {
    unknown(label, expected, `could not enumerate commits between ${tag} and ${refName}`);
    return;
  }
  const commits = listed ? listed.split('\n').filter(Boolean) : [];
  const paths = new Set();
  let offending = 0;
  for (const commit of commits) {
    let files;
    try {
      files = git(['show', '--pretty=format:', '--name-only', commit]).trim();
    } catch {
      files = '';
    }
    const hits = files
      .split('\n')
      .map((f) => f.trim())
      .filter((f) => f && GATE_PATHS.some((re) => re.test(f)));
    if (hits.length > 0) {
      offending += 1;
      for (const f of hits) paths.add(f);
    }
  }
  if (paths.size === 0) {
    ok(
      label,
      expected,
      commits.length === 0
        ? `${tag} -> ${tagged.slice(0, 7)} is not behind ${refName}`
        : `${commits.length} commit(s) after ${tag} touch no gate path`,
    );
    return;
  }
  const sorted = [...paths].sort();
  stale(
    label,
    expected,
    `${offending} of ${commits.length} commit(s) after ${tag} touch ${sorted.length} gate path(s)`,
  );
  details.push(`  gate paths changed after ${tag}: ${sorted.join(', ')}`);
  details.push(
    `  fix: re-point ${tag} at ${refName} (${base.slice(0, 7)}) and re-tag, or bump the patch version and tag that`,
  );
}

// 10. the tracker row for the release. This repository's convention is one row
// per release (S16-12, S19-04, S24-17, S24-24 …), recorded in docs/tracker.csv.
function checkTracker() {
  if (!exists('docs/tracker.csv')) {
    unknown('tracker row (docs/tracker.csv)', `a row naming ${rawVersion}`, 'no tracker');
    return;
  }
  const records = parseCsv(read('docs/tracker.csv'));
  const header = records.shift() ?? [];
  const at = (name) => header.indexOf(name);
  const match = records.find((cells) => {
    const title = cells[at('title')] ?? '';
    return title.includes(rawVersion) && /release/i.test(cells[at('category')] ?? '');
  });
  if (!match) {
    stale(
      'tracker row (docs/tracker.csv)',
      `a release row naming ${rawVersion}`,
      'no matching row',
    );
    return;
  }
  const id = match[at('id')] ?? '?';
  const status = match[at('status')] ?? '?';
  ok(
    'tracker row (docs/tracker.csv)',
    `a release row naming ${rawVersion}`,
    `${id} (status ${status})`,
  );
}

// A minimal RFC-4180 reader: docs/tracker.csv has quoted fields with embedded
// commas and newlines, which a split(',') turns into garbage.
function parseCsv(text) {
  const rows = [];
  let field = '';
  let cells = [];
  let quoted = false;
  for (let i = 0; i < text.length; i += 1) {
    const c = text[i];
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          field += '"';
          i += 1;
        } else quoted = false;
      } else field += c;
    } else if (c === '"') quoted = true;
    else if (c === ',') {
      cells.push(field);
      field = '';
    } else if (c === '\n') {
      cells.push(field);
      rows.push(cells);
      cells = [];
      field = '';
    } else if (c !== '\r') field += c;
  }
  if (field !== '' || cells.length > 0) {
    cells.push(field);
    rows.push(cells);
  }
  return rows;
}

// 11. the surfaces that need a live store. Reported, never skipped and never
// guessed — scripts/check-store-listing.mjs is what reads them, with a key.
function checkLiveStores() {
  unknown(
    'App Store Connect (live)',
    'the app is for sale, at this version',
    'UNKNOWN - needs credentials (scripts/check-store-listing.mjs)',
  );
  unknown(
    'Google Play (live)',
    'the production track carries this version',
    'UNKNOWN - needs credentials (scripts/check-store-listing.mjs / play-rollout.mjs)',
  );
}

// A winget submission opens a pull request from the contributor's own fork, and
// on 2026-11-28 the 1.5.0 release died at that step with `dragoscv does not have
// the correct permissions to execute CreateRef` after every gate had passed and
// the manifest was correct (S26-30) — a whole release run lost to a credential
// no earlier step could see. This probe is one read-only API call: a GET on the
// fork reports the authenticated token's permissions, and `push` is the ability
// to create a ref. A confirmed inability is STALE and names the rotation; an
// unreachable GitHub is SKIP, because an API nobody could reach is not a bad
// token, and only a fact that was read can stop a run.
const WINGET_FORK = 'dragoscv/winget-pkgs';
const WINGET_ROTATION =
  '  rotate WINGET_TOKEN: a classic PAT with the public_repo scope, or a fine-grained PAT with Contents read+write and Pull requests read+write on the fork';

async function checkWingetToken() {
  const label = 'winget token (can create refs on the fork)';
  const expected = `${WINGET_FORK} accepts a ref from WINGET_TOKEN`;
  const token = process.env.WINGET_TOKEN;
  if (!token) {
    skip(label, expected, 'WINGET_TOKEN not set, so the winget channel will skip');
    return;
  }
  if (offline) {
    skip(label, expected, '--offline, so the token was not probed');
    return;
  }
  let response;
  try {
    response = await fetch(`https://api.github.com/repos/${WINGET_FORK}`, {
      headers: {
        Authorization: `Bearer ${token}`,
        Accept: 'application/vnd.github+json',
        'User-Agent': 'vitals-release-doctor',
      },
      signal: AbortSignal.timeout(8000),
    });
  } catch (error) {
    skip(
      label,
      expected,
      `GitHub API unreachable (${error.name}), so the token was not probed (this is not a failure)`,
    );
    return;
  }
  if (response.status === 200) {
    const body = await response.json().catch(() => null);
    if (body?.permissions?.push === true) {
      ok(label, expected, `WINGET_TOKEN can create a ref on ${WINGET_FORK}`);
      return;
    }
    stale(
      label,
      expected,
      `WINGET_TOKEN cannot create a ref on ${WINGET_FORK} (no push permission)`,
    );
    details.push(WINGET_ROTATION);
    return;
  }
  if ([401, 403, 404].includes(response.status)) {
    stale(
      label,
      expected,
      `GitHub refused WINGET_TOKEN for ${WINGET_FORK} (HTTP ${response.status})`,
    );
    details.push(WINGET_ROTATION);
    return;
  }
  skip(
    label,
    expected,
    `GitHub answered HTTP ${response.status} for ${WINGET_FORK}; the token was not judged`,
  );
}

// ── run ────────────────────────────────────────────────────────────────────

if (onlyTagTree || onlyWinget) {
  // A focused run for the release workflow's Plan job: only the check the step
  // is asking about, so a failure names that one thing and no unrelated surface
  // can stop a tag for a reason the step did not opt into.
  if (onlyTagTree) checkTagTree();
  if (onlyWinget) await checkWingetToken();
} else {
  checkManifests();
  checkChangelog();
  checkVersionLiterals();
  checkMsixWhatsNew();
  checkPlayChangelog();
  checkStoreConsistency();
  checkNamedProse();
  checkApple('apps/ios/store/listing.json', 'Apple listing (iPhone)');
  checkApple('apps/desktop/store/macos-listing.json', 'Apple listing (Mac)');
  checkTag();
  checkTagTree();
  checkTracker();
  checkLiveStores();
}

// ── report ─────────────────────────────────────────────────────────────────

const columns = ['surface', 'expected', 'found', 'status'];
const widths = columns.map((c, i) =>
  Math.max(c.length, ...rows.map((r) => (Object.values(r)[i] ?? '').length)),
);
const line = (cells) => cells.map((c, i) => String(c).padEnd(widths[i])).join('  ');
const headerLine = line(columns);
process.stdout.write(`\nrelease-doctor ${rawVersion} (tag ${tag})\n\n`);
process.stdout.write(`${headerLine}\n${'-'.repeat(headerLine.length)}\n`);
for (const r of rows) process.stdout.write(`${line([r.surface, r.expected, r.found, r.status])}\n`);

const count = (status) => rows.filter((r) => r.status === status).length;
const stales = count('STALE');
const skips = count('SKIP');
const partial = count('UNKNOWN') > 0;
if (details.length > 0) {
  process.stdout.write(`\nfindings:\n${[...new Set(details)].join('\n')}\n`);
}
// SKIP is named only when there is one, so a clean, fully-read run keeps the
// tally a reader already knows (… HELD, STALE, UNKNOWN).
const summaryParts = [`${count('OK')} OK`, `${count('HELD')} HELD`];
if (skips > 0) summaryParts.push(`${skips} SKIP`);
summaryParts.push(`${stales} STALE`, `${count('UNKNOWN')} UNKNOWN`);
const summary = summaryParts.join(', ');
process.stdout.write(
  `\n${summary} - ${partial ? 'PARTIAL check (a live store surface needs a credential)' : 'FULL check'}\n`,
);
// A focused run names what it was asked about; the full run counts surfaces.
const staleReason =
  onlyTagTree && !onlyWinget
    ? 'the tag is behind origin/main on a path the gates read'
    : onlyWinget && !onlyTagTree
      ? 'the winget token cannot create a ref on the fork'
      : `${stales} surface(s) name the wrong version`;
process.stdout.write(
  stales > 0
    ? `exit ${(partial ? 100 : 0) + stales}: ${staleReason}\n\n`
    : `exit ${partial ? 100 : 0}: nothing stale\n\n`,
);

// 1..99 or 101..199 encode the stale count; the +100 says the run was partial.
// process.exitCode, never process.exit(): exit() truncates stdout that has not
// been flushed to a pipe or a file, which is why the first version of this
// script printed nothing at all.
process.exitCode = (partial ? 100 : 0) + stales;
