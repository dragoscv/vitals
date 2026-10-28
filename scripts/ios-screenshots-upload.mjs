// Uploads the iPhone screenshots made by .github/workflows/ios-screenshots.yml
// (artefact `ios-screenshots`, unpacked as <dir>/<lang>/NN-name.png) to the
// editable App Store version, replacing whatever set is there. The display
// type follows the image size: 1320x2868 -> APP_IPHONE_67 (the 6.9" slot),
// 1242x2688 -> APP_IPHONE_65.
//
//   node scripts/ios-screenshots-upload.mjs <dir>
//   node scripts/ios-screenshots-upload.mjs <dir> --platform macos
// The second form uploads the Mac App Store shots from
// .github/workflows/macos-screenshots.yml (folders en/ and ro/) to the Mac
// app; every accepted 16:10 Mac size maps to the single APP_DESKTOP slot.
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const { asc: raw } = await import(
  pathToFileURL(join(homedir(), '.copilot/bin/apple-asc.mjs')).href
);
const asc = async (method, path, body) => {
  const r = await raw(method, path, body);
  if (r.status >= 300) {
    const detail = (r.json.errors ?? []).map((e) => `${e.code}: ${e.detail}`).join('; ');
    throw new Error(`${method} ${path} -> ${r.status} ${detail}`);
  }
  return r.json;
};

const mac = process.argv.includes('macos');
const APP = mac ? '6827932703' : '6827931747';
const PLATFORM = mac ? 'MAC_OS' : 'IOS';
// artefact folder -> ASC locale
const LOCALES = mac ? { en: 'en-GB', ro: 'ro' } : { 'en-GB': 'en-GB', ro: 'ro' };
const dir = process.argv[2];
if (!dir) throw new Error('usage: ios-screenshots-upload.mjs <dir>');

/** PNG width/height from the IHDR chunk. */
function pngSize(buf) {
  return { w: buf.readUInt32BE(16), h: buf.readUInt32BE(20) };
}
function displayType({ w, h }) {
  if (mac) {
    const ok = [
      [1280, 800],
      [1440, 900],
      [2560, 1600],
      [2880, 1800],
    ].some(([a, b]) => a === w && b === h);
    if (ok) return 'APP_DESKTOP';
    throw new Error(`no Mac App Store slot for ${w}x${h}`);
  }
  if (w === 1320 && h === 2868) return 'APP_IPHONE_67';
  if (w === 1290 && h === 2796) return 'APP_IPHONE_67';
  if (w === 1242 && h === 2688) return 'APP_IPHONE_65';
  throw new Error(`no App Store slot for ${w}x${h}`);
}

const version = (
  await asc('GET', `/v1/apps/${APP}/appStoreVersions?filter[platform]=${PLATFORM}&limit=5`)
).data.find((v) => v.attributes.appStoreState === 'PREPARE_FOR_SUBMISSION');
if (!version) throw new Error('no version in PREPARE_FOR_SUBMISSION');
const locs = (await asc('GET', `/v1/appStoreVersions/${version.id}/appStoreVersionLocalizations`))
  .data;

for (const [folder, locale] of Object.entries(LOCALES)) {
  const loc = locs.find((l) => l.attributes.locale === locale);
  if (!loc) throw new Error(`no ${locale} localisation`);
  const files = readdirSync(join(dir, folder))
    .filter((f) => f.endsWith('.png'))
    .sort();
  if (files.length === 0) throw new Error(`no screenshots in ${folder}`);
  const type = displayType(pngSize(readFileSync(join(dir, folder, files[0]))));

  // Replace: delete every existing set of this type, then create a fresh one.
  const sets = (await asc('GET', `/v1/appStoreVersionLocalizations/${loc.id}/appScreenshotSets`))
    .data;
  for (const s of sets.filter((x) => x.attributes.screenshotDisplayType === type)) {
    await asc('DELETE', `/v1/appScreenshotSets/${s.id}`);
  }
  const set = (
    await asc('POST', '/v1/appScreenshotSets', {
      data: {
        type: 'appScreenshotSets',
        attributes: { screenshotDisplayType: type },
        relationships: {
          appStoreVersionLocalization: {
            data: { type: 'appStoreVersionLocalizations', id: loc.id },
          },
        },
      },
    })
  ).data;

  for (const name of files) {
    const bytes = readFileSync(join(dir, folder, name));
    const shot = (
      await asc('POST', '/v1/appScreenshots', {
        data: {
          type: 'appScreenshots',
          attributes: { fileName: name, fileSize: bytes.length },
          relationships: { appScreenshotSet: { data: { type: 'appScreenshotSets', id: set.id } } },
        },
      })
    ).data;
    for (const op of shot.attributes.uploadOperations) {
      const part = bytes.subarray(op.offset, op.offset + op.length);
      const headers = Object.fromEntries(op.requestHeaders.map((h) => [h.name, h.value]));
      const res = await fetch(op.url, { method: op.method, headers, body: part });
      if (!res.ok) throw new Error(`upload part of ${name}: ${res.status}`);
    }
    await asc('PATCH', `/v1/appScreenshots/${shot.id}`, {
      data: {
        type: 'appScreenshots',
        id: shot.id,
        attributes: {
          uploaded: true,
          sourceFileChecksum: createHash('md5').update(bytes).digest('hex'),
        },
      },
    });
    console.log(locale, type, name);
  }
}

// Apple processes asynchronously; report the state of what was just sent.
await new Promise((r) => setTimeout(r, 15_000));
for (const [, locale] of Object.entries(LOCALES)) {
  const loc = locs.find((l) => l.attributes.locale === locale);
  const s = await asc(
    'GET',
    `/v1/appStoreVersionLocalizations/${loc.id}/appScreenshotSets?include=appScreenshots`,
  );
  const states = (s.included ?? []).map((x) => x.attributes.assetDeliveryState?.state ?? '?');
  console.log(locale, 'states', states.join(','));
}
