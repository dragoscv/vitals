#!/usr/bin/env node
// Fetches (creating once) the Mac App Store provisioning profile for
// app.vitals.desktop and writes it where tauri.appstore.conf.json embeds it.
//
//   node scripts/macos/appstore-profile.mjs [out-path]
//
// Uses the shared App Store Connect client (~/.copilot/bin/apple-asc.mjs, key
// in Secret Manager). A profile is not a secret, but it is per-team and
// expires, so it is fetched at build time rather than committed.
import { writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const { asc } = await import(
  pathToFileURL(join(homedir(), '.copilot', 'bin', 'apple-asc.mjs')).href
);

const BUNDLE = 'app.vitals.desktop';
const NAME = 'Vitals Mac App Store';
const out = resolve(process.argv[2] ?? 'apps/desktop/src-tauri/macos/embedded.provisionprofile');

const ok = (r, what) => {
  if (r.status >= 400)
    throw new Error(`${what}: HTTP ${r.status} ${JSON.stringify(r.json?.errors ?? r.json)}`);
  return r.json;
};

const bundle = ok(
  await asc('GET', `/v1/bundleIds?filter[identifier]=${BUNDLE}`),
  'bundle id',
).data.find((b) => b.attributes.identifier === BUNDLE);
if (!bundle) throw new Error(`bundle id ${BUNDLE} is not registered`);

const certs = ok(await asc('GET', '/v1/certificates?limit=200'), 'certificates').data.filter(
  (c) =>
    c.attributes.certificateType === 'DISTRIBUTION' ||
    c.attributes.certificateType === 'MAC_APP_DISTRIBUTION',
);
if (certs.length === 0) throw new Error('no Apple Distribution certificate on the team');

const existing = ok(
  await asc('GET', `/v1/bundleIds/${bundle.id}/profiles?limit=200`),
  'profiles',
).data.find(
  (p) =>
    p.attributes.profileType === 'MAC_APP_STORE' &&
    p.attributes.profileState === 'ACTIVE' &&
    Date.parse(p.attributes.expirationDate) > Date.now() + 7 * 864e5,
);

let profile = existing;
if (!profile) {
  profile = ok(
    await asc('POST', '/v1/profiles', {
      data: {
        type: 'profiles',
        attributes: {
          name: `${NAME} ${new Date().toISOString().slice(0, 10)}`,
          profileType: 'MAC_APP_STORE',
        },
        relationships: {
          bundleId: { data: { type: 'bundleIds', id: bundle.id } },
          certificates: { data: certs.map((c) => ({ type: 'certificates', id: c.id })) },
        },
      },
    }),
    'create profile',
  ).data;
  console.log(`created profile ${profile.id}`);
} else {
  console.log(`reusing profile ${profile.id}`);
}

writeFileSync(out, Buffer.from(profile.attributes.profileContent, 'base64'));
console.log(`wrote ${out} (expires ${profile.attributes.expirationDate})`);
