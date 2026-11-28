// Prove the Tracking page's read path against a REAL index: the file agentq
// writes on this machine, parsed by the page's OWN parser.
//
// Nothing here re-implements the parser — it imports `model.ts` and calls it,
// so the numbers printed are what the screen would draw. Run:
//   node scripts/prove-tracking.mjs [index.json]
//
// With no argument it reads the index agentq keeps in its state directory, so
// it can be re-run after any change to the parser or to the index. The exit
// code is non-zero when the parser drops a row or the headline counts disagree
// with the file — that is the whole point of a prover.

import { existsSync, readFileSync } from 'node:fs';
import { homedir } from 'node:os';
import path from 'node:path';

import {
  allTrackers,
  formatAge,
  generatedAtMs,
  groupItems,
  indexTotals,
  openCount,
  parseIndex,
  repoItemCount,
  repoStatusCounts,
  sortedStatuses,
} from '../apps/desktop/src/features/tracking/model.ts';

// agentq keeps its state under `$AGENTQ_HOME`, defaulting to `~/.codai/coord`;
// the index is the same file the Tracking host command hands to the webview.
const stateDir = process.env.AGENTQ_HOME || path.join(homedir(), '.codai', 'coord');
const indexPath = process.argv[2] ?? path.join(stateDir, 'trackers', 'index.json');

if (!existsSync(indexPath)) {
  console.error(`prove-tracking: no index at ${indexPath} (pass a path, or set AGENTQ_HOME)`);
  process.exit(1);
}

const text = readFileSync(indexPath, 'utf8');
const raw = JSON.parse(text);

// What the file itself claims, counted independently of the parser.
let rawItems = 0;
let rawTrackers = 0;
for (const repo of raw.repos ?? []) {
  rawTrackers += (repo.trackers ?? []).length;
  for (const tracker of repo.trackers ?? []) rawItems += (tracker.items ?? []).length;
}

const index = parseIndex(raw);
if (index === null) {
  console.log('parseIndex -> null (the page would show its empty state)');
  process.exit(1);
}

const totals = indexTotals(index);
const age = generatedAtMs(index);
const dropped = rawItems - totals.items;

console.log(`file          ${indexPath} (${text.length} bytes)`);
console.log(
  `raw           ${raw.repos.length} repo(s), ${rawTrackers} tracker(s), ${rawItems} item(s)`,
);
console.log(
  `parsed        ${totals.repos} repo(s), ${totals.trackers} tracker(s), ${totals.items} item(s)`,
);
console.log(`dropped       ${dropped} item(s) the parser refused (must be 0 for a clean file)`);
console.log(`open          ${openCount(totals.byStatus)} row(s) not done/dropped/deferred`);
console.log(`drift         ${totals.drift} finding(s) across ${totals.reposWithDrift} repo(s)`);
console.log(`generatedAt   ${index.generatedAt} -> age ${formatAge(Date.now() - (age ?? 0))}`);
console.log(
  `statuses      ${sortedStatuses(totals.byStatus)
    .map((s) => `${s}=${totals.byStatus[s]}`)
    .join(' ')}`,
);

console.log('--- per repository, as a card would draw it');
for (const repo of index.repos) {
  const counts = repoStatusCounts(repo);
  const git = repo.git === null ? 'no clone info' : `${repo.git.branch}@${repo.git.head}`;
  console.log(
    `  ${repo.repo}  ${repoItemCount(repo)} rows  ${git}  ` +
      `${sortedStatuses(counts)
        .map((s) => `${s}=${counts[s]}`)
        .join(' ')}  drift=${repo.drift.length}`,
  );
  for (const tracker of repo.trackers) {
    const groups = groupItems(tracker.items);
    const titled = groups.filter((g) => g.group !== null).length;
    console.log(
      `    ${tracker.id} (${tracker.file}): ${tracker.items.length} rows in ${groups.length} group(s), ${titled} with a label`,
    );
  }
  for (const finding of repo.drift) {
    console.log(
      `    drift [${finding.kind}] ${finding.tracker} ${finding.ref} - ${finding.detail}`,
    );
  }
}

console.log(`--- statuses across the whole index: ${allTrackers(index).length} tracker(s)`);
console.log(
  `--- sample rows: ${index.repos[0].trackers[0].items
    .slice(0, 3)
    .map((i) => `${i.id}[${i.status}]`)
    .join(', ')}`,
);

// The prover's verdict: anything the parser silently dropped, or a headline
// count that disagrees with the file, is a defect the page would hide.
const problems = [];
if (dropped !== 0) problems.push(`${dropped} item(s) dropped by the parser`);
if (totals.repos !== raw.repos.length) {
  problems.push(`repos ${totals.repos} != ${raw.repos.length}`);
}
if (totals.trackers !== rawTrackers) {
  problems.push(`trackers ${totals.trackers} != ${rawTrackers}`);
}
if (problems.length > 0) {
  console.error(`FAIL  ${problems.join('; ')}`);
  process.exit(1);
}
console.log('OK    0 dropped, counts agree with the file');
