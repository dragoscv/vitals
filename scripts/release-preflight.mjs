#!/usr/bin/env node
// Refuses a release cut that is not complete.
//
// Cutting a release is done by hand (docs/releasing.md) and the ORDER is what
// makes it correct: every manifest bumped together, Cargo.lock regenerated, the
// CHANGELOG [Unreleased] section closed and dated with a fresh one opened, the
// annotated tag created and pushed, and only then the release workflow
// dispatched. Skipping a step leaves a cut that LOOKS right — a manifest left
// behind, or a tag whose commit's CHANGELOG carries no section for the version
// — and nothing notices until someone reads the release notes weeks later.
//
// Three checks, and every problem is reported in one run rather than stopping
// at the first:
//
//   1. the tag matches the version in every manifest. This is deliberately NOT
//      re-implemented: scripts/version.ps1 -Check is the repository's existing
//      source of truth for which files carry the version and how they are
//      compared, so a seventh manifest added tomorrow is covered for free.
//   2. CHANGELOG.md has a CLOSED section for the version (one that carries a
//      date) and a fresh [Unreleased] heading above it.
//   3. the tag is annotated and its commit is reachable from the default
//      branch — a tag on a commit the default branch does not contain publishes
//      code nobody reviewed.
//
// Read-only and offline: git read commands, one file read, and the existing
// PowerShell checker. No secret, no network.
//
//   node scripts/release-preflight.mjs --tag v1.3.1
//   node scripts/release-preflight.mjs --version 1.3.1   (tag derived as v1.3.1)
//   node scripts/release-preflight.mjs --ref refs/tags/v1.3.1   (what the workflow passes)
//
// Exit codes: 0 the cut is complete; 1 a cut problem (every one printed); 2 the
// check could not run (git or pwsh missing, no default branch to test against)
// — also a failure, because a gate that cannot run must not pass by staying
// silent. With a ref that is not a v* tag — a build-only `workflow_dispatch`
// run — it prints why and exits 0: such a run publishes nothing, so there is no
// cut to check.
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

// The repository root, one level up from this script in scripts/.
const repoRoot = fileURLToPath(new URL('..', import.meta.url));
const problems = [];
let cannotRun = false;

const argv = process.argv.slice(2);
// A flag whose value is absent, empty or another flag reads as the empty string.
function argValue(flag) {
  const i = argv.indexOf(flag);
  if (i === -1) return undefined;
  const next = argv[i + 1];
  return next === undefined || next.startsWith('--') ? '' : next;
}

let tag = argValue('--tag');
let version = argValue('--version');
const ref = argValue('--ref');

// The workflow passes github.ref rather than the resolved version, so this can
// run as the first step of `plan`, before that job writes the outputs the rest
// of the workflow keys on. The pattern mirrors `plan`'s own `refs/tags/v*`: a
// tag publishes, anything else is a build-only run.
if (!tag && !version) {
  const fromRef = ref ? /^refs\/tags\/(v.+)$/.exec(ref) : null;
  if (fromRef) {
    tag = fromRef[1];
  } else {
    console.log(
      `release preflight: ${ref ? `${ref} is not a release tag` : 'no tag, version or ref given'} — a build-only run publishes nothing, so there is no cut to check.`,
    );
    process.exit(0);
  }
}
if (!version) version = tag.replace(/^v/, '');
if (!tag) tag = `v${version}`;
if (tag.replace(/^v/, '') !== version) {
  problems.push(`the tag ${tag} and the version ${version} disagree; the tag must be v<version>`);
}

const git = (args) => spawnSync('git', ['-C', repoRoot, ...args], { encoding: 'utf8' });

// One probe, so a missing git is reported once rather than by every call below.
let gitUsable = true;
const gitProbe = spawnSync('git', ['--version'], { encoding: 'utf8' });
if (gitProbe.error) {
  gitUsable = false;
  cannotRun = true;
  problems.push(
    `git is not available (${gitProbe.error.code ?? gitProbe.error.message}); the tag checks cannot run, so this is a failure`,
  );
}

// 1. Manifests — delegated to the repository's own version checker.
const versionScript = join(repoRoot, 'scripts', 'version.ps1');
const manifest = spawnSync('pwsh', ['-NoProfile', '-File', versionScript, '-Check', tag], {
  cwd: repoRoot,
  encoding: 'utf8',
});
if (manifest.error) {
  cannotRun = true;
  problems.push(
    `could not run scripts/version.ps1 (${manifest.error.code ?? manifest.error.message}); every manifest must be checked, so this is a failure`,
  );
} else if (manifest.status !== 0) {
  // version.ps1 throws, and PowerShell colourises the thrown error; strip the
  // ANSI codes so the detail reads the same in a terminal and a CI log.
  const detail = `${manifest.stdout ?? ''}${manifest.stderr ?? ''}`
    .replace(/\u001b\[[0-9;]*m/g, '')
    .trim()
    .replace(/\s*\r?\n\s*/g, '; ');
  problems.push(
    `manifests do not all say ${version}: ${detail || 'version.ps1 -Check exited non-zero with no output'}`,
  );
}

// 2. CHANGELOG — a closed, dated section for this version with a fresh
// [Unreleased] above it. An [Unreleased] heading exists so a reader knows where
// the next release's notes go; it has no date until that release is cut.
let changelog = null;
try {
  changelog = readFileSync(join(repoRoot, 'CHANGELOG.md'), 'utf8');
} catch (e) {
  problems.push(`CHANGELOG.md could not be read (${e.code ?? e.message})`);
}

if (changelog !== null) {
  const entries = [];
  changelog.split(/\r?\n/).forEach((line, i) => {
    const m = /^##\s+\[([^\]]+)\]\s*(.*)$/.exec(line);
    if (m) entries.push({ name: m[1].trim(), rest: m[2].trim(), line: i + 1 });
  });

  const unreleased = entries.findIndex((e) => e.name === 'Unreleased');
  const section = entries.findIndex((e) => e.name === version);

  if (unreleased === -1) {
    problems.push(
      'CHANGELOG.md has no `## [Unreleased]` heading; every release opens a fresh one after it closes the last section',
    );
  }
  if (section === -1) {
    problems.push(
      `CHANGELOG.md has no \`## [${version}]\` section; the release notes for this version do not exist`,
    );
  } else {
    if (!/\d{4}-\d{2}-\d{2}/.test(entries[section].rest)) {
      problems.push(
        `CHANGELOG.md \`## [${version}]\` (line ${entries[section].line}) has no date; close the section with e.g. \`## [${version}] — YYYY-MM-DD\` before tagging`,
      );
    }
    if (unreleased !== -1) {
      if (section < unreleased) {
        problems.push(
          `CHANGELOG.md \`## [Unreleased]\` (line ${entries[unreleased].line}) sits below \`## [${version}]\` (line ${entries[section].line}); the fresh [Unreleased] must be above the section just released`,
        );
      } else {
        const between = entries.slice(unreleased + 1, section).map((e) => `[${e.name}]`);
        if (between.length > 0) {
          problems.push(
            `CHANGELOG.md has ${between.join(', ')} between \`## [Unreleased]\` and \`## [${version}]\`; the version just released must be the first section below the fresh [Unreleased]`,
          );
        }
      }
    }
  }
}

// 3. The tag: annotated, and on the default branch.
function resolveDefaultBranch() {
  const candidates = [];
  const sym = git(['symbolic-ref', '--quiet', '--short', 'refs/remotes/origin/HEAD']);
  if (sym.status === 0 && (sym.stdout ?? '').trim()) candidates.push(sym.stdout.trim());
  candidates.push('origin/main', 'origin/master', 'main', 'master');
  for (const b of candidates) {
    if (git(['rev-parse', '--verify', '--quiet', b]).status === 0) return b;
  }
  return null;
}

if (gitUsable) {
  const exists = git(['rev-parse', '--verify', '--quiet', `refs/tags/${tag}`]);
  if (exists.status !== 0) {
    problems.push(
      `tag ${tag} does not exist; create and push the annotated tag (\`git tag -a ${tag} -m ...\`) before the release runs`,
    );
  } else {
    // An annotated tag is an object whose type is `tag`; a lightweight tag is
    // just a name for a commit and carries no message or date.
    const type = git(['cat-file', '-t', `refs/tags/${tag}`]);
    const kind = (type.stdout ?? '').trim();
    if (type.status === 0 && kind !== 'tag') {
      problems.push(
        `tag ${tag} is not annotated (git reports its type as '${kind}'); a release tag must be annotated so it carries its own message and date`,
      );
    }

    const branch = resolveDefaultBranch();
    if (!branch) {
      cannotRun = true;
      problems.push(
        'could not determine the default branch (none of refs/remotes/origin/HEAD, origin/main, origin/master, main, master exist); a release whose tag is not on the default branch would publish an unreviewed commit',
      );
    } else {
      const reachable = git(['merge-base', '--is-ancestor', `${tag}^{commit}`, branch]);
      // merge-base --is-ancestor: 0 ancestor, 1 not, anything else an error.
      if (reachable.status === 1) {
        problems.push(
          `tag ${tag} is not reachable from ${branch}; its commit is not on the default branch, so the release would publish code the default branch does not contain`,
        );
      } else if (reachable.status !== 0) {
        cannotRun = true;
        problems.push(
          `could not test whether ${tag} is on ${branch}: ${(reachable.stderr ?? '').trim() || `git merge-base exited ${reachable.status}`}`,
        );
      }
    }
  }
}

console.log(`release preflight for ${version} (tag ${tag})`);
if (problems.length === 0) {
  console.log(
    '  all checks passed: every manifest says the tag, the CHANGELOG closed this version with a fresh [Unreleased] above it, and the tag is annotated and on the default branch.',
  );
  process.exit(0);
}
console.log(`  FAILED — ${problems.length} problem${problems.length === 1 ? '' : 's'}:`);
for (const p of problems) console.log(`  - ${p}`);
process.exit(cannotRun ? 2 : 1);
