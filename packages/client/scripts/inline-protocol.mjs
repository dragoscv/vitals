// Makes the published declarations self-contained.
//
// `@vitals/protocol` is private: its types are generated from the Rust model
// and never published on their own. tsc emits them beside the client's own
// declarations (dist/types/protocol/...), so every `@vitals/protocol` import
// is rewritten to that relative path, and dist/index.d.ts re-exports the
// client entry. A consumer installs one package and gets the full types.
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const dist = join(dirname(fileURLToPath(import.meta.url)), '..', 'dist');
const types = join(dist, 'types');
const protocolEntry = join(types, 'protocol', 'src', 'index.js');

function walk(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? walk(path) : [path];
  });
}

let rewritten = 0;
for (const file of walk(types).filter((f) => f.endsWith('.d.ts'))) {
  const source = readFileSync(file, 'utf8');
  let target = relative(dirname(file), protocolEntry).replaceAll('\\', '/');
  if (!target.startsWith('.')) target = `./${target}`;
  // The source is written for `moduleResolution: bundler`, which allows
  // `from './client'`. A consumer on `nodenext` rejects that (TS2834), so
  // relative specifiers get the extension the emitted file actually has —
  // `.js` for a module, `/index.js` for a directory.
  const next = source
    .replaceAll(/(['"])@vitals\/protocol\1/g, `'${target}'`)
    .replaceAll(/(from\s+|import\()(['"])(\.{1,2}\/[^'"]+?)\2/g, (whole, lead, quote, spec) => {
      if (/\.(js|mjs|cjs|json)$/.test(spec)) return whole;
      const base = join(dirname(file), spec);
      const fixed = existsSync(`${base}.d.ts`) ? `${spec}.js` : `${spec}/index.js`;
      return `${lead}${quote}${fixed}${quote}`;
    });
  if (next !== source) {
    writeFileSync(file, next);
    rewritten += 1;
  }
}

writeFileSync(join(dist, 'index.d.ts'), "export * from './types/client/src/index.js';\n");

const leftover = walk(types).filter(
  // Specifiers only: doc comments mention the package by name on purpose.
  (f) => f.endsWith('.d.ts') && /(['"])@vitals\/protocol\1/.test(readFileSync(f, 'utf8')),
);
if (leftover.length > 0) {
  console.error('still importing @vitals/protocol:', leftover);
  process.exit(1);
}
console.log(`declarations self-contained (${rewritten} files rewritten)`);
