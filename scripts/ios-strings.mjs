#!/usr/bin/env node
// Builds apps/ios/Vitals/Localizable.xcstrings from the Android phone app's
// strings.xml (EN + RO) plus apps/ios/strings-ios.json, so the two phones
// cannot drift in wording. Run after changing either source:
//   node scripts/ios-strings.mjs           write the catalog
//   node scripts/ios-strings.mjs --check   exit 1 when the committed catalog is stale (check-drift)
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const res = join(root, 'apps/android/app/src/main/res');
const out = join(root, 'apps/ios/Vitals/Localizable.xcstrings');

const unescape = (s) =>
  s
    .replace(/\\'/g, "'")
    .replace(/\\"/g, '"')
    .replace(/\\n/g, '\n')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');

// Android positional format -> Foundation's (%1$s -> %1$@, %1$d -> %1$lld).
const toApple = (s) =>
  s
    .replace(/%(\d+\$)?s/g, (_, p) => `%${p ?? ''}@`)
    .replace(/%(\d+\$)?d/g, (_, p) => `%${p ?? ''}lld`);

function parse(file) {
  const text = readFileSync(join(res, file), 'utf8');
  const strings = new Map();
  for (const m of text.matchAll(/<string name="([^"]+)"([^>]*)>([\s\S]*?)<\/string>/g)) {
    if (/translatable="false"/.test(m[2])) continue;
    strings.set(m[1], toApple(unescape(m[3].trim())));
  }
  const plurals = new Map();
  for (const m of text.matchAll(/<plurals name="([^"]+)">([\s\S]*?)<\/plurals>/g)) {
    const forms = {};
    for (const it of m[2].matchAll(/<item quantity="([a-z]+)">([\s\S]*?)<\/item>/g))
      forms[it[1]] = toApple(unescape(it[2].trim()));
    plurals.set(m[1], forms);
  }
  return { strings, plurals };
}

const en = parse('values/strings.xml');
const ro = parse('values-ro/strings.xml');
const ios = JSON.parse(readFileSync(join(root, 'apps/ios/strings-ios.json'), 'utf8'));

const unit = (value) => ({ stringUnit: { state: 'translated', value } });
const catalog = { sourceLanguage: 'en', strings: {}, version: '1.0' };
const add = (key, e, r) => {
  if (r === undefined) throw new Error(`missing Romanian for ${key}`);
  catalog.strings[key] = { extractionState: 'manual', localizations: { en: unit(e), ro: unit(r) } };
};
for (const [k, v] of en.strings) add(k, v, ro.strings.get(k));
for (const [k, forms] of en.plurals) {
  const roForms = ro.plurals.get(k);
  if (!roForms) throw new Error(`missing Romanian plural ${k}`);
  const variation = (f) => ({
    variations: { plural: Object.fromEntries(Object.entries(f).map(([q, v]) => [q, unit(v)])) },
  });
  catalog.strings[k] = {
    extractionState: 'manual',
    localizations: { en: variation(forms), ro: variation(roForms) },
  };
}
for (const [k, v] of Object.entries(ios)) {
  if (k.startsWith('_')) continue;
  if (catalog.strings[k]) throw new Error(`${k} is in both strings.xml and strings-ios.json`);
  add(k, v.en, v.ro);
}
const sorted = Object.fromEntries(
  Object.entries(catalog.strings).sort(([a], [b]) => a.localeCompare(b)),
);
const text = JSON.stringify({ ...catalog, strings: sorted }, null, 2) + '\n';

if (process.argv.includes('--check')) {
  const current = existsSync(out) ? readFileSync(out, 'utf8') : '';
  if (current !== text) {
    console.error(
      'apps/ios/Vitals/Localizable.xcstrings is stale: run node scripts/ios-strings.mjs',
    );
    process.exit(1);
  }
  console.log(`ios strings ok (${Object.keys(sorted).length} keys)`);
} else {
  writeFileSync(out, text);
  console.log(`wrote ${Object.keys(sorted).length} keys`);
}
