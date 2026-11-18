#!/usr/bin/env node
// Fetches (creating once) the four IOS_APP_STORE provisioning profiles the
// Vitals iPhone archive signs with — the app, the watch app and the two
// widgets — and writes one .mobileprovision per bundle id.
//
//   node scripts/ios-appstore-profiles.mjs <out-dir>
//
// Prints a JSON map { "<bundle id>": { "name": ..., "uuid": ... } } on stdout so
// the driver can sign each target by name.
//
// Why this exists: on 2026-11-18 the guest's Xcode 26.6 could no longer
// regenerate the App Store profiles through -allowProvisioningUpdates (it
// answered "Authentication failed: Make sure a bearer token was provided" on
// every target and fell back to a wildcard profile). Supplying the profiles
// explicitly lets the archive use MANUAL signing, which never calls the
// provisioning service. A profile is not a secret, but it is per-team and
// expires, so it is fetched at build time rather than committed.
//
// Uses the shared App Store Connect client (~/.copilot/bin/apple-asc.mjs, key
// in Secret Manager), same shape as scripts/macos/appstore-profile.mjs.
import { mkdirSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const { asc } = await import(
  pathToFileURL(join(homedir(), '.copilot', 'bin', 'apple-asc.mjs')).href
);

const TARGETS = [
  { bundle: 'app.vitals.ios', name: 'Vitals iOS App Store' },
  { bundle: 'app.vitals.ios.watchkitapp', name: 'Vitals iOS Watch App Store' },
  { bundle: 'app.vitals.ios.widget', name: 'Vitals iOS Widget App Store' },
  { bundle: 'app.vitals.ios.watchkitapp.widget', name: 'Vitals iOS Watch Widget App Store' },
];

const outDir = resolve(process.argv[2] ?? 'apps/ios/build/profiles');
mkdirSync(outDir, { recursive: true });

const ok = (r, what) => {
  if (r.status >= 400)
    throw new Error(`${what}: HTTP ${r.status} ${JSON.stringify(r.json?.errors ?? r.json)}`);
  return r.json;
};

// A distribution identity, iOS or the "Apple Distribution" one: the archive is
// signed with it, so the profile must list it. Expiry > 7 days.
const certs = ok(await asc('GET', '/v1/certificates?limit=200'), 'certificates').data.filter(
  (c) =>
    (c.attributes.certificateType === 'DISTRIBUTION' ||
      c.attributes.certificateType === 'IOS_DISTRIBUTION') &&
    Date.parse(c.attributes.expirationDate) > Date.now() + 7 * 864e5,
);
if (certs.length === 0) throw new Error('no valid Apple Distribution certificate on the team');

const result = {};
for (const { bundle, name } of TARGETS) {
  const id = ok(
    await asc('GET', `/v1/bundleIds?filter[identifier]=${bundle}&limit=200`),
    `bundle id ${bundle}`,
  ).data.find((b) => b.attributes.identifier === bundle);
  if (!id) throw new Error(`bundle id ${bundle} is not registered`);

  const profiles = ok(
    await asc('GET', `/v1/bundleIds/${id.id}/profiles?limit=200`),
    `profiles for ${bundle}`,
  ).data;
  let profile = profiles.find(
    (p) =>
      p.attributes.profileType === 'IOS_APP_STORE' &&
      p.attributes.profileState === 'ACTIVE' &&
      Date.parse(p.attributes.expirationDate) > Date.now() + 7 * 864e5,
  );
  if (profile) {
    console.log(`reusing ${bundle} -> ${profile.id} (${profile.attributes.name})`);
  } else {
    profile = ok(
      await asc('POST', '/v1/profiles', {
        data: {
          type: 'profiles',
          attributes: {
            name: `${name} ${new Date().toISOString().slice(0, 10)}`,
            profileType: 'IOS_APP_STORE',
          },
          relationships: {
            bundleId: { data: { type: 'bundleIds', id: id.id } },
            certificates: { data: certs.map((c) => ({ type: 'certificates', id: c.id })) },
          },
        },
      }),
      `create profile ${bundle}`,
    ).data;
    console.log(`created ${bundle} -> ${profile.id}`);
  }

  // Re-read for profileContent (the list endpoint omits it).
  const full = ok(
    await asc(
      'GET',
      `/v1/profiles/${profile.id}?fields[profiles]=name,uuid,profileContent,expirationDate`,
    ),
    `profile content ${bundle}`,
  ).data.attributes;
  writeFileSync(
    join(outDir, `${bundle}.mobileprovision`),
    Buffer.from(full.profileContent, 'base64'),
  );
  result[bundle] = { name: full.name, uuid: full.uuid };
}

process.stdout.write(JSON.stringify(result) + '\n');
