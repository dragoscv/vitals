#!/usr/bin/env node
// Fetches (creating or regenerating) the four IOS_APP_STORE provisioning profiles the
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
// A profile is baked at creation: the portal warns that adding or removing a
// capability invalidates it. The App Group id is plain text inside the CMS
// blob, so a profile that does not contain it was made before the App ID's
// App Groups were assigned and must be replaced — otherwise the archive fails
// with "doesn't match the entitlements file's value for
// com.apple.security.application-groups". The App Store Connect API does not
// surface the assignment, so the blob is the only honest test.
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

const APP_GROUP = 'group.app.vitals.ios';
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
  const candidate = profiles.find(
    (p) =>
      p.attributes.profileType === 'IOS_APP_STORE' &&
      p.attributes.profileState === 'ACTIVE' &&
      Date.parse(p.attributes.expirationDate) > Date.now() + 7 * 864e5,
  );

  // The list endpoint omits profileContent, so the content is always re-read.
  const fetchProfile = async (pid) =>
    ok(
      await asc(
        'GET',
        `/v1/profiles/${pid}?fields[profiles]=name,uuid,profileContent,expirationDate`,
      ),
      `profile content ${bundle}`,
    ).data.attributes;

  const write = (attrs) => {
    const raw = Buffer.from(attrs.profileContent, 'base64');
    writeFileSync(join(outDir, `${bundle}.mobileprovision`), raw);
    result[bundle] = { name: attrs.name, uuid: attrs.uuid };
    return raw;
  };

  if (candidate) {
    const attrs = await fetchProfile(candidate.id);
    if (write(attrs).includes(APP_GROUP)) {
      console.log(`reusing ${bundle} -> ${candidate.id} (${attrs.name})`);
      continue;
    }
    console.log(`stale ${bundle} -> ${candidate.id} lacks ${APP_GROUP}; replacing it`);
    ok(await asc('DELETE', `/v1/profiles/${candidate.id}`), `delete profile ${bundle}`);
  }

  const created = ok(
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
  console.log(`created ${bundle} -> ${created.id}`);
  const attrs = await fetchProfile(created.id);
  if (!write(attrs).includes(APP_GROUP)) {
    throw new Error(
      `${bundle}: the new profile still lacks ${APP_GROUP} — set App Groups for this App ID in the developer portal`,
    );
  }
}

process.stdout.write(JSON.stringify(result) + '\n');
