#!/usr/bin/env node
// Packs @vitals/client and releases it to npm as the name in
// publishConfig.name (@vitals-app/client — the @vitals scope belongs to an
// unrelated project).
//
// npm ignores `publishConfig.name`, so the manifest inside the tarball is
// rewritten after `pnpm pack` (which already swapped in the dist entry points
// and resolved workspace: ranges). `--dry-run` stops before the upload, which
// is how CI checks this path on every run without releasing anything.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const dryRun = process.argv.includes('--dry-run');
const pkgDir = resolve(import.meta.dirname, '..', 'packages', 'client');
const source = JSON.parse(readFileSync(join(pkgDir, 'package.json'), 'utf8'));
const publicName = source.publishConfig?.name;
if (!publicName) throw new Error('packages/client publishConfig.name is not set');

const work = mkdtempSync(join(tmpdir(), 'vitals-npm-'));
const run = (cmd, args, cwd) =>
  execFileSync(cmd, args, { cwd, stdio: 'inherit', shell: process.platform === 'win32' });

run('pnpm', ['pack', '--pack-destination', work], pkgDir);
const tgz = readdirSync(work).find((f) => f.endsWith('.tgz'));
if (!tgz) throw new Error('pnpm pack produced no tarball');
run('tar', ['-xzf', tgz], work);

const manifestPath = join(work, 'package', 'package.json');
const manifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
manifest.name = publicName;
delete manifest.devDependencies;
delete manifest.scripts;
delete manifest.publishConfig?.name;
writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(
  `${publicName}@${manifest.version}  deps: ${Object.keys(manifest.dependencies ?? {}).length}`,
);

const args = ['publish', '--access', 'public'];
if (!dryRun) args.push('--provenance');
if (dryRun) args.push('--dry-run');
// A prerelease must not become `latest` for people who install without a
// version; `next` is npm's convention for it.
if (manifest.version.includes('-')) args.push('--tag', 'next');
run('npm', args, join(work, 'package'));
