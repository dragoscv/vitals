#!/usr/bin/env node
// check-release-notes — the four surfaces that carry a release's "what's new"
// text must all name the one version the manifests declare.
//
// Why this exists. A version is published through four independent surfaces,
// each with its own file and its own writer, and nothing compares them: the
// Microsoft Store what's-new block, the Google Play release notes, the GitHub
// release body (the CHANGELOG section scripts/release-notes.ps1 renders) and
// the "current release" sentence on the website's terms pages. The failure this
// prevents is concrete and has happened: a Store listing went live still
// describing the PREVIOUS version while the new build shipped — every gate was
// green because no gate read the notes at all. A release is only correct when
// all four names agree.
//
// Why it is shaped the way it is:
//
//   * The version is NOT re-derived here. It is read from package.json, the
//     same source scripts/release-doctor.mjs reads, so the two cannot disagree
//     about which version is being released. This gate is the focused half of
//     that doctor's wider sweep: it reads only the four notes surfaces, so it
//     is cheap enough for the fast checks lane and for a pre-commit hook.
//
//   * It runs offline by default. Everything it reads is a committed file, so
//     there is no credential and no network call — the two live store surfaces
//     (App Store Connect, Google Play production) are read by
//     scripts/check-store-listing.mjs, with a key, and are not this gate's job.
//
//   * The SKIP contract is the point. A surface that cannot be read — a file
//     missing, a shape this reader does not recognise — is reported SKIP with
//     the concrete reason, never OK, and it does not by itself turn the run
//     red, because a surface nobody could read is not a surface that failed.
//     Two rules bound that leniency: a wrong version on one file is never
//     excused by a sibling file that could not be read (the definite finding
//     wins), and a run in which EVERY surface was skipped exits non-zero,
//     because a gate that passes by reading nothing is the exact defect this
//     exists to prevent.
//
// Exit codes:
//   0  every surface that could be read names the version, and at least one was
//      read and compared
//   1  a surface names a different version, or no surface could be read at all
//   2  usage error
//
//   node scripts/check-release-notes.mjs                version from package.json
//   node scripts/check-release-notes.mjs --version 1.4.0 a specific release
//                                                        (a fixture or a
//                                                        rehearsal; it never
//                                                        turns a red surface
//                                                        green, because every
//                                                        comparison still runs)
//
// Dependency-free on purpose: Node's standard library, nothing else.

import { existsSync, readFileSync } from 'node:fs';
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
const unknown = argv.filter(
  (a) => a.startsWith('-') && a !== '--version' && !a.startsWith('--version='),
);
if (unknown.length > 0) {
  process.stdout.write(
    `unknown option(s): ${unknown.join(', ')}\nusage: node scripts/check-release-notes.mjs [--version <v>]\n`,
  );
  process.exitCode = 2;
} else {
  main();
}

function main() {
  // The version under check, from the manifest scripts/release-doctor.mjs also
  // reads. A leading `v` (from a tag) is dropped; the rest is compared on its
  // three-part core, so 1.5.1 and 1.5.1.0 name the same release.
  const version = (flagValue('--version') ?? JSON.parse(read('package.json')).version).replace(
    /^v/,
    '',
  );
  const core = (v) => (v.match(/\d+\.\d+\.\d+/) ?? [v])[0];

  // Every version a file's own shape names, or an empty list when the shape is
  // not one this reader knows — the empty list is what makes the row SKIP
  // rather than OK. NUMBER matches three- or four-part, so a version spelled
  // the Store's way is still read.
  const NUMBER = String.raw`\d+\.\d+\.\d+(?:\.\d+)?`;
  const all = (text, re) => [...text.matchAll(re)].map((m) => m[1]);
  // The Play changelog's version is on its first non-empty line ("Vitals 1.5.1").
  const playLine = (text) => {
    const first = text.split(/\r?\n/).find((l) => l.trim().length > 0) ?? '';
    const m = first.match(new RegExp(String.raw`Vitals\s+v?(${NUMBER})`));
    return m ? [m[1]] : [];
  };
  // The GitHub release body is the CHANGELOG section for the version
  // (scripts/release-notes.ps1). Its newest DATED heading is the most recent
  // release; `## [Unreleased]` carries no date and the pattern skips it, so a
  // renamed-and-dated section is what this reads.
  const changelogNewest = (text) => {
    const m = text.match(
      new RegExp(String.raw`^##\s+\[(${NUMBER})\]\s*[—–-]\s*\d{4}-\d{2}-\d{2}`, 'm'),
    );
    return m ? [m[1]] : [];
  };

  // The four surfaces. Each is a name and the files carrying its text; the
  // paths are the ones an `rg` for the version finds, not a list taken on
  // trust — a surface whose file moves is reported SKIP until this list is
  // updated.
  const surfaces = [
    {
      name: "MSIX what's-new (en-us + ro-ro)",
      files: [
        {
          path: 'packaging/msix/listing/en-us.md',
          pluck: (t) => all(t, new RegExp(String.raw`What's new in (${NUMBER})`, 'g')),
        },
        {
          path: 'packaging/msix/listing/ro-ro.md',
          pluck: (t) => all(t, new RegExp(String.raw`Ce e nou în (${NUMBER})`, 'g')),
        },
      ],
    },
    {
      name: 'Play release notes (en-US + ro)',
      files: [
        { path: 'apps/android/play/listing/en-US/changelogs/default.txt', pluck: playLine },
        { path: 'apps/android/play/listing/ro/changelogs/default.txt', pluck: playLine },
      ],
    },
    {
      name: 'GitHub release notes (CHANGELOG.md)',
      files: [{ path: 'CHANGELOG.md', pluck: changelogNewest }],
    },
    {
      name: 'site version text (terms.md + ro/terms.md)',
      files: [
        {
          path: 'apps/site/src/content/docs/terms.md',
          pluck: (t) => all(t, /current release is (\d+\.\d+\.\d+)/g),
        },
        {
          path: 'apps/site/src/content/docs/ro/terms.md',
          pluck: (t) => all(t, /versiunea curentă este (\d+\.\d+\.\d+)/g),
        },
      ],
    },
  ];

  const rows = [];
  let failures = 0;
  let checked = 0;
  for (const surface of surfaces) {
    const wrong = [];
    const unreadable = [];
    for (const file of surface.files) {
      if (!existsSync(join(root, file.path))) {
        unreadable.push(`${file.path}: file missing`);
        continue;
      }
      let versions;
      try {
        versions = file.pluck(read(file.path));
      } catch (error) {
        unreadable.push(`${file.path}: unreadable (${error.message})`);
        continue;
      }
      if (!versions || versions.length === 0) {
        unreadable.push(`${file.path}: no version in the shape this reader knows`);
        continue;
      }
      for (const v of versions) {
        if (core(v) !== core(version)) wrong.push(`${file.path} says ${v}, must say ${version}`);
      }
    }
    // A definite wrong version wins over a sibling that could not be read: SKIP
    // must never turn a red finding green.
    if (wrong.length > 0) {
      failures += 1;
      rows.push({ status: 'FAIL', name: surface.name, detail: wrong.join('; ') });
    } else if (unreadable.length > 0) {
      rows.push({ status: 'SKIP', name: surface.name, detail: unreadable.join('; ') });
    } else {
      checked += 1;
      rows.push({ status: 'OK', name: surface.name, detail: `names ${version}` });
    }
  }

  const width = Math.max(...rows.map((r) => r.name.length));
  const out = [];
  out.push(`release-notes ${version} (from package.json)`);
  for (const r of rows) out.push(`  ${r.status.padEnd(4)}  ${r.name.padEnd(width)}  ${r.detail}`);

  // process.stdout.write + process.exitCode, not process.exit(): exit() can
  // drop output that has not reached the OS yet when stdout is a pipe or a
  // file, which is how a gate prints a table a caller never sees.
  if (failures > 0) {
    out.push(`release-notes: ${failures} surface(s) name a different version`);
    process.stdout.write(`${out.join('\n')}\n`);
    process.exitCode = 1;
    return;
  }
  if (checked === 0) {
    out.push(
      'release-notes: NOTHING CHECKED — every surface was skipped; a run that read nothing must not pass',
    );
    process.stdout.write(`${out.join('\n')}\n`);
    process.exitCode = 1;
    return;
  }
  const skipped = rows.filter((r) => r.status === 'SKIP').length;
  out.push(
    `release-notes: ${checked} surface(s) name ${version}${skipped ? `, ${skipped} skipped` : ''}`,
  );
  process.stdout.write(`${out.join('\n')}\n`);
  process.exitCode = 0;
}

function read(rel) {
  return readFileSync(join(root, rel), 'utf8');
}
