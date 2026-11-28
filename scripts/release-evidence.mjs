#!/usr/bin/env node
// release-evidence — one honest evidence sheet per shipped version.
//
// Why this exists. A release is claimed by a dozen surfaces — the GitHub
// release, winget, Scoop, Chocolatey, npm, Google Play — and "1.5.1 is out" is
// asserted far more often than it is read. The failure this prevents is the
// quiet one: an absent reading (a channel nobody checked, an artefact never
// downloaded, a job that has not finished) gets rendered as a blank or a dash
// and read as success. Here every cell is either a value WITH its source
// (GitHub, the readings file, the disk) or the literal words `not read` with
// the reason, and the verdict counts both, so an unread row can never be
// mistaken for a pass.
//
// It is read-only against GitHub. GitHub is read through `gh api`, which is
// already authenticated in every environment that cuts a release, and nothing
// is written except the sheet itself. `--offline` reads nothing from GitHub and
// works entirely from a readings file, so a sheet can be regenerated on a
// machine with no network.
//
//   node scripts/release-evidence.mjs --tag v1.5.2
//   node scripts/release-evidence.mjs --run 46096442294 -Assets .copilot-tmp/release-1-5-1/artifact
//   node scripts/release-evidence.mjs --tag v1.5.2 -Channels readings.json --offline
//
// The sheet is appended (newest version first) to docs/release-evidence.md,
// replacing that version's section in place if it is already there and leaving
// every other version untouched. The head of that file carries the same
// explanation for a reader, and the generator never touches another section.
//
// The channel readings come from a file another tool produced, because no
// single command can read `winget`, `Chocolatey` and Play at once. The shape:
//
//   {
//     "readAt": "2026-11-28T12:00:00Z",
//     "run":       { "id": 1, "url": "…", "event": "push", "status": "…",
//                    "conclusion": "…", "headSha": "…", "headBranch": "v1.5.2",
//                    "tag": "v1.5.2", "tagCommit": "…" },
//     "jobs":      [ { "name": "Plan", "status": "completed", "conclusion": "success" } ],
//     "artifacts": [ { "name": "vitals-windows-x64", "sizeInBytes": 1, "digest": "sha256:…" } ],
//     "channels":  { "winget": { "version": "1.5.2", "read": "manifest merged",
//                                "readAt": "…", "command": "gh api …" } }
//   }
//
// Any of `run`, `jobs`, `artifacts` and `channels` may be absent; what is
// absent is reported `not read`, never guessed.
//
// Exit codes: 0 the sheet was produced (whatever it says); 2 the arguments or
// the readings file are unusable, so no sheet was produced. A `not read` row
// never moves the exit code: it is the honest absence of evidence, not a
// failure of the generator.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync, writeSync } from 'node:fs';
import { dirname, isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(dirname(fileURLToPath(import.meta.url)));

// ── arguments ──────────────────────────────────────────────────────────────
// Both `--flag` and `-flag` are accepted (the release docs and a PowerShell
// habit both reach for the single dash), the name is matched WITHOUT regard to
// case (`-Assets` is what the docs write and `--assets` is what a shell writes),
// and both `--flag value` and `--flag=value` work. A flag whose value is
// missing reads as the empty string, which the checks below reject rather than
// treating as "not given".
const argv = process.argv.slice(2);
const lc = (s) => s.toLowerCase();
function flagValue(name) {
  const wanted = new Set([lc(name), lc(`-${name.replace(/^--/, '')}`)]);
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const eq = arg.indexOf('=');
    const key = lc(eq >= 0 ? arg.slice(0, eq) : arg);
    if (!wanted.has(key)) continue;
    if (eq >= 0) return arg.slice(eq + 1);
    const next = argv[i + 1];
    return next === undefined || next.startsWith('-') ? '' : next;
  }
  return null;
}
// A bare positional is either a run id (digits) or a tag (v<version>). Values
// consumed by a flag are excluded so `--tag v1.5.1` does not also read `v1.5.1`
// as a positional.
const consumed = new Set();
for (let i = 0; i < argv.length; i += 1) {
  if (argv[i].startsWith('-') && argv[i + 1] !== undefined && !argv[i + 1].startsWith('-')) {
    consumed.add(i + 1);
  }
}
const positional = argv.filter((a, i) => !a.startsWith('-') && a !== '' && !consumed.has(i));
// A bare positional is either a run id (digits) or a tag (v<version>).
const bareRun = positional.find((a) => /^\d+$/.test(a)) ?? null;
const bareTag = positional.find((a) => /^v\d/.test(a)) ?? null;

const hasFlag = (name) => argv.some((a) => lc(a) === lc(name));
const offline = hasFlag('--offline');
const stdoutOnly = hasFlag('--stdout-only');
const help = hasFlag('--help') || hasFlag('-h');

let runId = flagValue('--run') ?? bareRun;
const tagArg = flagValue('--tag') ?? bareTag;
const channelsPath = flagValue('--channels');
const assetsArg = flagValue('--assets');
const outArg = flagValue('--out') ?? 'docs/release-evidence.md';
const repoArg = flagValue('--repo');
const nowArg = flagValue('--now');

// Matched by BARE name, so `-Assets`, `--assets` and `--Assets` are all the
// same flag; the leading dashes are spelling, not identity.
const knownFlags = new Set([
  'offline',
  'stdout-only',
  'help',
  'h',
  'run',
  'tag',
  'channels',
  'assets',
  'out',
  'repo',
  'now',
]);
const unknown = argv.filter((a, i) => {
  if (!a.startsWith('-') || consumed.has(i)) return false;
  const key = lc(a.includes('=') ? a.slice(0, a.indexOf('=')) : a);
  return !knownFlags.has(key.replace(/^-+/, ''));
});
// stopper(): writeSync, not console.error — a process.exit() can drop output
// that has not reached the OS yet when stdout is a pipe, which is exactly how a
// CI step ends up reporting nothing at all.
function stopper(message) {
  writeSync(2, `${message}\n`);
  process.exit(2);
}
if (unknown.length > 0) {
  stopper(`unknown option(s): ${unknown.join(', ')}\nknown: ${[...knownFlags].join(', ')}`);
}
if (help) {
  console.log(
    [
      'usage: node scripts/release-evidence.mjs (--tag vX.Y.Z | --run <id>) [options]',
      '',
      '  --tag <tag>         a released tag; its run is found on GitHub (or read offline)',
      '  --run <id>          a GitHub Actions run id',
      '  -Assets <dir>       a directory to look in for downloaded artefacts to hash',
      '  -Channels <file>    a JSON readings file another tool produced (see the header)',
      '  --offline           read nothing from GitHub; use the readings file only',
      '  --out <file>        where to write the sheet (default docs/release-evidence.md)',
      '  --stdout-only       print the sheet; write no file',
      '  --repo <owner/name> override the repository (default: the origin remote)',
      '  --now <iso>         stamp the sheet with this time instead of the clock',
    ].join('\n'),
  );
  process.exit(0);
}
if (!runId && !tagArg) {
  stopper('give a --tag or a --run (or a bare run id / tag as the first argument)');
}
if (runId !== null && !/^\d+$/.test(runId)) {
  stopper(`--run must be a numeric run id, got '${runId}'`);
}

const assetsDir = assetsArg ? resolve(root, assetsArg) : null;
const channelsFile = channelsPath ? resolve(root, channelsPath) : null;
const outPath = isAbsolute(outArg) ? outArg : resolve(root, outArg);
const now = nowArg ?? new Date().toISOString();

// ── the repository ─────────────────────────────────────────────────────────
// Derived from the origin remote so a fork's sheet points at the fork. Falls
// back to the canonical repository when there is no remote to read.
function detectRepo() {
  if (repoArg) return repoArg;
  try {
    const url = execFileSync('git', ['-C', root, 'remote', 'get-url', 'origin'], {
      encoding: 'utf8',
    }).trim();
    const m = /github\.com[:/]([^/]+)\/([^/]+?)(?:\.git)?$/.exec(url);
    if (m) return `${m[1]}/${m[2]}`;
  } catch {
    // No git, no remote: the fallback below is still a real repository.
  }
  return 'dragoscv/vitals';
}
const repo = detectRepo();

// ── the claim ledger ───────────────────────────────────────────────────────
// Every cell that asserts a value passes through here. `read` is a value with
// a source; `unread` is the reason a value could not be read. The verdict at
// the foot of the sheet is these two totals, so a reader can see at a glance
// how much of the sheet is evidence and how much is a hole.
let verified = 0;
let unreadCount = 0;
function read(value) {
  verified += 1;
  return value;
}
function unread(why) {
  unreadCount += 1;
  return `not read — ${why}`;
}

// ── GitHub, read-only ──────────────────────────────────────────────────────
function ghApi(path) {
  const out = execFileSync('gh', ['api', path], {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  return JSON.parse(out);
}

// ── the readings file ──────────────────────────────────────────────────────
let readings = {};
let readingsNote = null;
if (channelsFile) {
  if (!existsSync(channelsFile)) {
    stopper(`the readings file ${channelsFile} does not exist`);
  }
  try {
    readings = JSON.parse(readFileSync(channelsFile, 'utf8'));
  } catch (error) {
    stopper(`the readings file ${channelsFile} is not valid JSON: ${error.message}`);
  }
} else {
  readingsNote = 'no -Channels readings file was given';
}

// The readings file is written by hand or by another tool, so it may spell the
// run and its artefacts the way the header documents (camelCase: `url`,
// `headSha`, `sizeInBytes`) or the way `gh api` returns them (snake_case:
// `html_url`, `head_sha`, `size_in_bytes`). Normalise once, here, so the render
// reads ONE shape: a field the file actually carries must never be reported
// `not read` merely because of spelling.
function firstDefined(...values) {
  return values.find((v) => v !== undefined && v !== null);
}
function normaliseRun(r) {
  if (!r || typeof r !== 'object') return null;
  return {
    ...r,
    id: firstDefined(r.id, r.run_id, r.runId),
    html_url: firstDefined(r.html_url, r.url),
    head_sha: firstDefined(r.head_sha, r.headSha),
    head_branch: firstDefined(r.head_branch, r.headBranch),
    tagCommit: firstDefined(r.tagCommit, r.tag_commit),
  };
}
function normaliseArtifact(a) {
  return {
    ...a,
    id: firstDefined(a.id, a.artifact_id),
    size_in_bytes: firstDefined(a.size_in_bytes, a.sizeInBytes),
  };
}

// ── resolve the run and the tag ────────────────────────────────────────────
// Online, GitHub is the source of truth and the readings file is ignored for
// these fields; offline (or when `gh` cannot answer) the file is used, and what
// the file lacks is `not read`. Either way the sheet says which.
function pickLatestRun(runs, tag) {
  const forTag = (runs ?? []).filter((r) => !tag || r.head_branch === tag);
  if (forTag.length === 0) return null;
  return forTag.sort((a, b) => (b.run_number ?? 0) - (a.run_number ?? 0))[0];
}

let runRec = null;
let liveOk = false;
let liveWhy = null;
if (!offline) {
  try {
    if (!runId) {
      const listed = ghApi(
        `repos/${repo}/actions/runs?head_branch=${encodeURIComponent(tagArg)}&per_page=30`,
      );
      const chosen = pickLatestRun(listed.workflow_runs, tagArg);
      if (chosen) runId = String(chosen.id);
    }
    if (runId) {
      runRec = ghApi(`repos/${repo}/actions/runs/${runId}`);
      liveOk = true;
    }
  } catch (error) {
    liveWhy = `${error.message ?? error}`;
  }
}
const fileRun = normaliseRun(readings.run);
const fromFile = !liveOk && fileRun ? fileRun : null;
const run = runRec ?? fromFile ?? null;
const runSource = runRec ? 'GitHub' : fromFile ? 'the readings file' : null;

const tag = tagArg ?? run?.tag ?? (run?.head_branch?.startsWith('v') ? run.head_branch : null);

// The tag's own commit, which is not the same claim as the run's head sha: a
// re-pointed tag leaves the run where it was. Read live when online; otherwise
// taken from the file; otherwise `not read`. Deliberately NOT read from the
// local `git tag`, which is stale exactly when it matters — a re-pointed tag
// was the very case that motivated this sheet.
let tagCommit = null;
let tagCommitWhy = null;
if (offline) {
  tagCommit = fileRun?.tagCommit ?? null;
  if (!tagCommit) tagCommitWhy = 'offline, and the readings file does not carry the tag commit';
} else if (tag) {
  try {
    const ref = ghApi(`repos/${repo}/git/ref/tags/${encodeURIComponent(tag)}`);
    if (ref.object?.type === 'tag') {
      const annotated = ghApi(`repos/${repo}/git/tags/${ref.object.sha}`);
      tagCommit = annotated.object?.sha ?? null;
    } else {
      tagCommit = ref.object?.sha ?? null;
    }
    if (!tagCommit) tagCommitWhy = `GitHub returned no commit for the tag ${tag}`;
  } catch (error) {
    tagCommit = fileRun?.tagCommit ?? null;
    if (!tagCommit)
      tagCommitWhy = `the tag ${tag} could not be read from GitHub (${error.message})`;
  }
} else {
  tagCommitWhy = 'no tag was given and none could be derived from the run';
}

// ── jobs ───────────────────────────────────────────────────────────────────
let jobs = null;
let jobsWhy = null;
if (liveOk && runId) {
  try {
    jobs = ghApi(`repos/${repo}/actions/runs/${runId}/jobs`).jobs;
  } catch (error) {
    jobs = null;
    jobsWhy = `the run's jobs could not be read from GitHub (${error.message})`;
  }
} else if (Array.isArray(readings.jobs)) {
  jobs = readings.jobs;
}
if (!jobs) jobsWhy = jobsWhy ?? readingsNote ?? 'no jobs were read';

// ── artefacts ──────────────────────────────────────────────────────────────
let artifacts = null;
let artifactsWhy = null;
if (liveOk && runId) {
  try {
    artifacts = ghApi(`repos/${repo}/actions/runs/${runId}/artifacts`).artifacts;
  } catch (error) {
    artifacts = null;
    artifactsWhy = `the run's artefacts could not be read from GitHub (${error.message})`;
  }
} else if (Array.isArray(readings.artifacts)) {
  artifacts = readings.artifacts.map(normaliseArtifact);
}
if (!artifacts) artifactsWhy = artifactsWhy ?? readingsNote ?? 'no artefacts were read';

// ── helpers shared by the render ───────────────────────────────────────────
function countFiles(dir) {
  try {
    return readdirSync(dir, { recursive: true }).filter((p) => {
      try {
        return statSync(join(dir, p)).isFile();
      } catch {
        return false;
      }
    }).length;
  } catch {
    return 0;
  }
}

// The local SHA-256 is read from a real file only. An extracted directory has
// no single hash, and saying so is more useful than hashing a manifest of it
// and implying a comparison that does not exist.
function localHashFor(name) {
  if (!assetsDir) {
    return { kind: 'unread', why: 'no -Assets directory was given, so nothing on disk was read' };
  }
  if (!existsSync(assetsDir)) {
    return { kind: 'unread', why: `the assets directory ${assetsDir} does not exist` };
  }
  for (const candidate of [join(assetsDir, `${name}.zip`), join(assetsDir, name)]) {
    if (!existsSync(candidate)) continue;
    const stat = statSync(candidate);
    if (stat.isFile()) {
      const sha = createHash('sha256').update(readFileSync(candidate)).digest('hex');
      return { kind: 'file', path: candidate, sha, size: stat.size };
    }
    if (stat.isDirectory()) {
      return {
        kind: 'unread',
        why: `present as an extracted directory (${countFiles(candidate)} file(s)); GitHub's digest covers the archive, not the extracted tree`,
      };
    }
  }
  return { kind: 'unread', why: `no file named ${name} or ${name}.zip under ${assetsDir}` };
}

const cell = (value) => String(value ?? '').replace(/\|/g, '\\|');
// A prettier-clean pipe table: columns are padded to the widest cell, the
// separator row is as wide as that — the exact shape `prettier --check` wants,
// so the generated page never needs reformatting afterwards.
function table(headers, rows) {
  const all = [headers, ...rows];
  const widths = headers.map((_, i) => Math.max(3, ...all.map((r) => cell(r[i]).length)));
  const line = (cells) => `| ${cells.map((c, i) => cell(c).padEnd(widths[i])).join(' | ')} |`;
  const sep = `| ${widths.map((w) => '-'.repeat(w)).join(' | ')} |`;
  return [line(headers), sep, ...rows.map(line)].join('\n');
}

const CHANNELS = [
  { id: 'github-release', label: 'GitHub release assets' },
  { id: 'winget', label: 'winget (dragoscv/winget-pkgs)' },
  { id: 'scoop', label: 'Scoop' },
  { id: 'chocolatey', label: 'Chocolatey' },
  { id: 'npm', label: 'npm (@vitals-app/client)' },
  { id: 'google-play', label: 'Google Play' },
];
const channelReadings = readings.channels ?? {};

// ── render the section ─────────────────────────────────────────────────────
function renderSection() {
  const out = [];
  const versionLabel = tag ? tag.replace(/^v/, '') : runId ? `run ${runId}` : 'unknown';
  out.push(`## Vitals ${versionLabel}`);
  out.push('');
  const link = run?.html_url ? `[${run.id}](${run.html_url})` : `${runId ?? 'an unnamed run'}`;
  out.push(`_Generated ${now} by \`scripts/release-evidence.mjs\` against run ${link}._`);
  out.push('');
  // The exact command that produced this section, so reproducing it — or
  // re-running it once publish finishes — is one copy-paste, not a memory test.
  const regen = ['node scripts/release-evidence.mjs'];
  if (offline) regen.push('--offline');
  if (tag) regen.push(`--tag ${tag}`);
  else if (runId) regen.push(`--run ${runId}`);
  if (assetsArg) regen.push(`-Assets ${assetsArg}`);
  if (channelsPath) regen.push(`-Channels ${channelsPath}`);
  out.push(`_Reproduce with \`${regen.join(' ')}\`._`);
  out.push('');

  // 1. the run
  out.push('### The run');
  out.push('');
  const runRows = [];
  runRows.push([
    'Run id',
    run?.id
      ? read(`${run.id}`)
      : unread(
          runSource
            ? 'the readings file carries no run id'
            : (liveWhy ?? readingsNote ?? 'no run was read'),
        ),
  ]);
  runRows.push([
    'Run URL',
    run?.html_url ? read(run.html_url) : unread('GitHub returned no URL for this run'),
  ]);
  runRows.push(['Event', run?.event ? read(run.event) : unread('the event was not read')]);
  runRows.push([
    'Status',
    run?.status
      ? read(run.status)
      : unread(runSource ? 'the readings file carries no status' : 'the status was not read'),
  ]);
  runRows.push([
    'Conclusion',
    run && run.conclusion
      ? read(run.conclusion)
      : unread(
          run?.status && run.status !== 'completed'
            ? `the run has not finished (status: ${run.status})`
            : 'the conclusion was not read',
        ),
  ]);
  runRows.push([
    'Head sha',
    run?.head_sha
      ? read(run.head_sha)
      : unread(runSource ? 'the readings file carries no head sha' : 'the head sha was not read'),
  ]);
  runRows.push([
    'Head branch',
    run?.head_branch ? read(run.head_branch) : unread('the head branch was not read'),
  ]);
  runRows.push([
    'Tag',
    tag ? read(tag) : unread('no tag was given and none could be derived from the run'),
  ]);
  runRows.push([
    'Tag commit',
    tagCommit ? read(tagCommit) : unread(tagCommitWhy ?? 'the tag commit was not read'),
  ]);
  out.push(table(['Field', 'Value'], runRows));
  out.push('');
  if (run && run.head_sha && tagCommit && run.head_sha !== tagCommit) {
    out.push(
      `> The tag's commit (${tagCommit}) and the run's head sha (${run.head_sha}) differ: the tag was moved after the run. The artefacts below belong to the run, not to the current tag.`,
    );
    out.push('');
  }
  if (run && run.status && run.status !== 'completed') {
    out.push(
      `> The run is \`${run.status}\`, so later jobs (the publish, readback and channel jobs) do not appear in the table below yet.`,
    );
    out.push('');
  }

  // 2. the jobs
  out.push('#### Jobs');
  out.push('');
  if (jobs && jobs.length > 0) {
    const rows = jobs.map((j) => [
      j.name ?? '(unnamed)',
      j.status ? read(j.status) : unread('the job status was not read'),
      j.conclusion
        ? read(j.conclusion)
        : unread(
            j.status && j.status !== 'completed'
              ? `the job has not finished (status: ${j.status})`
              : 'the job concluded with no conclusion',
          ),
    ]);
    out.push(table(['Job', 'Status', 'Conclusion'], rows));
  } else {
    const why = jobsWhy ?? 'the run has no jobs to read';
    unread(why);
    out.push(`_No jobs to show: ${why}._`);
  }
  out.push('');

  // 3. the artefacts
  out.push('### Artefacts');
  out.push('');
  if (artifacts && artifacts.length > 0) {
    const rows = artifacts.map((a) => {
      const name = a.name ? read(a.name) : unread('the artefact has no name');
      const size = Number.isFinite(a.size_in_bytes)
        ? read(String(a.size_in_bytes))
        : unread('GitHub reported no size for this artefact');
      const digest = a.digest
        ? read(a.digest)
        : unread('GitHub reports no digest for this artefact');
      const local = localHashFor(a.name ?? '');
      let localCell;
      if (local.kind === 'file') {
        localCell = read(local.sha);
        if (a.digest) {
          const want = String(a.digest).replace(/^sha256:/, '');
          localCell +=
            local.sha === want
              ? ' (matches GitHub)'
              : ' (differs from GitHub — likely the extracted payload, not the archive)';
        }
      } else {
        localCell = unread(local.why);
      }
      return [name, size, digest, localCell];
    });
    out.push(table(['Artefact', 'Size (bytes)', 'GitHub digest', 'Local SHA-256'], rows));
  } else {
    const why = artifactsWhy ?? 'the run has no artefacts to read';
    unread(why);
    out.push(`_No artefacts to show: ${why}._`);
  }
  out.push('');

  // 4. the channels
  out.push('### Channels');
  out.push('');
  const nothingClaimed = Object.keys(channelReadings).length === 0;
  if (nothingClaimed) {
    out.push(
      `No channel readings are available (${readingsNote ?? 'the readings file carries no channels'}), so **nothing about the channels is claimed** here: every row below is \`not read\`.`,
    );
    out.push('');
  }
  const channelRows = CHANNELS.map((c) => {
    const r = channelReadings[c.id];
    if (!r) {
      return [
        c.label,
        unread('no reading in the file'),
        unread('no reading'),
        unread('no reading'),
        unread('no command was run'),
      ];
    }
    const reading = typeof r === 'string' ? { read: r } : r;
    return [
      c.label,
      reading.version ? read(reading.version) : unread('the reading names no published version'),
      reading.read ? read(reading.read) : unread('the reading carries no detail'),
      reading.readAt ? read(reading.readAt) : unread('the reading carries no timestamp'),
      reading.command ? read(reading.command) : unread('the reading names no command'),
    ];
  });
  // Any channel the file names that is not one of the six known ones is still
  // shown — a reading nobody expected is exactly what should not be dropped.
  for (const [id, r] of Object.entries(channelReadings)) {
    if (CHANNELS.some((c) => c.id === id)) continue;
    const reading = typeof r === 'string' ? { read: r } : r;
    channelRows.push([
      `${id} (unexpected)`,
      reading.version ? read(reading.version) : unread('the reading names no published version'),
      reading.read ? read(reading.read) : unread('the reading carries no detail'),
      reading.readAt ? read(reading.readAt) : unread('the reading carries no timestamp'),
      reading.command ? read(reading.command) : unread('the reading names no command'),
    ]);
  }
  out.push(
    table(['Channel', 'Published version', 'What was read', 'When', 'Command'], channelRows),
  );
  out.push('');

  // 5. the verdict
  out.push('### Verdict');
  out.push('');
  out.push(
    `**${verified} claim(s) VERIFIED, ${unreadCount} unread.** A \`not read\` row is not a pass: it means this sheet holds no evidence for it yet.`,
  );
  out.push('');
  return out.join('\n');
}

// Trimmed: renderSection ends with a newline, and the composition below adds one
// more, which would leave a trailing blank line every regeneration and make the
// committed page differ from the generator's output.
const section = renderSection().trimEnd();

// ── upsert into the living page ────────────────────────────────────────────
const HEAD = `# Release evidence

One section per shipped version, newest first. Each section is written by
\`scripts/release-evidence.mjs\`, which reads the release run and its artefacts
from GitHub (read-only, through \`gh\`) and takes the channel readings from a
file another tool produced. Nothing on this page is a claim unless it says so:

- **VERIFIED** — the value was read, from GitHub (the run, its jobs, its
  artefacts) or from the channel readings file or from a file on disk, and its
  source is named in the row.
- **not read** — the value could not be read, and the reason is given. A
  \`not read\` row is **not** a pass; it is the honest statement that this
  evidence does not exist yet. The verdict at the foot of each section counts
  both, so the two can never be confused.

Nothing in a section is a claim about the channels when no readings file was
given; the section says so in its own words.

Regenerate one version's section — it replaces only that section and leaves
every other version untouched:

\`\`\`text
node scripts/release-evidence.mjs --tag v1.5.2
\`\`\`

Add the downloaded artefacts so their local SHA-256 is read from disk:

\`\`\`text
node scripts/release-evidence.mjs --tag v1.5.2 -Assets .copilot-tmp/release-1-5-2/artifact
\`\`\`

Offline, from a readings file that holds everything:

\`\`\`text
node scripts/release-evidence.mjs --tag v1.5.2 -Channels readings.json --offline
\`\`\`
`;

function splitSections(text) {
  const marker = /^## Vitals /m;
  const first = text.search(marker);
  const body = first >= 0 ? text.slice(first) : '';
  const sections = [];
  const re = /^## Vitals (\S+)[^\n]*\n/gm;
  let m;
  const starts = [];
  while ((m = re.exec(body)) !== null) starts.push({ version: m[1], index: m.index });
  for (let i = 0; i < starts.length; i += 1) {
    const end = i + 1 < starts.length ? starts[i + 1].index : body.length;
    sections.push({ version: starts[i].version, text: body.slice(starts[i].index, end).trimEnd() });
  }
  return sections;
}

let existingSections = [];
if (existsSync(outPath)) {
  existingSections = splitSections(readFileSync(outPath, 'utf8'));
}
const versionLabel = (tag ?? 'unknown').replace(/^v/, '');
const at = existingSections.findIndex((s) => s.version === versionLabel);
let sections;
if (at >= 0) {
  sections = existingSections.map((s, i) =>
    i === at ? { version: versionLabel, text: section } : s,
  );
} else {
  sections = [{ version: versionLabel, text: section }, ...existingSections];
}
const document = `${HEAD}\n${sections.map((s) => s.text.trimEnd()).join('\n\n')}\n`;

process.stdout.write(`${section}\n`);
if (!stdoutOnly) {
  writeFileSync(outPath, document);
  process.stdout.write(`\nwrote ${outPath}\n`);
}
