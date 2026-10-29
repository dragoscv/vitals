// Applies a store listing to an App Store Connect record: version and
// app-info localisations (EN + RO), URLs, copyright, category, content
// rights, age rating, review details and free pricing. iPhone by default
// (apps/ios/store/listing.json); `--platform macos` applies
// apps/desktop/store/macos-listing.json to the Mac App Store record.
// Credentials: the team's ASC key via ~/.copilot/bin/apple-asc.mjs (Secret
// Manager, dragos-devcloud). Nothing secret is printed.
//
//   node scripts/ios-store.mjs            apply and print what changed
//   node scripts/ios-store.mjs --check    print the current state only
//   node scripts/ios-store.mjs --platform macos
import { readFileSync } from 'node:fs';
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

const mac = process.argv.includes('--platform') && process.argv.includes('macos');
const APP = mac ? '6827932703' : '6827931747';
const PLATFORM = mac ? 'MAC_OS' : 'IOS';
const listing = JSON.parse(
  readFileSync(
    new URL(
      mac ? '../apps/desktop/store/macos-listing.json' : '../apps/ios/store/listing.json',
      import.meta.url,
    ),
    'utf8',
  ),
);
const check = process.argv.includes('--check');
const log = (...a) => console.log(...a);

const version = (
  await asc('GET', `/v1/apps/${APP}/appStoreVersions?filter[platform]=${PLATFORM}&limit=5`)
).data.find((v) =>
  ['PREPARE_FOR_SUBMISSION', 'DEVELOPER_REJECTED', 'REJECTED', 'METADATA_REJECTED'].includes(
    v.attributes.appStoreState,
  ),
);
if (!version) throw new Error(`no editable ${PLATFORM} version`);
const info = (await asc('GET', `/v1/apps/${APP}/appInfos`)).data.find(
  (i) => i.attributes.appStoreState !== 'READY_FOR_SALE',
);
log('version', version.attributes.versionString, version.attributes.appStoreState);
if (check) process.exit(0);

// Version: copyright, and the version string the build carries.
await asc('PATCH', `/v1/appStoreVersions/${version.id}`, {
  data: {
    type: 'appStoreVersions',
    id: version.id,
    attributes: {
      copyright: listing.copyright,
      ...(listing.versionString !== undefined && { versionString: listing.versionString }),
    },
  },
});

// Localisations, created when missing.
const vlocs = (await asc('GET', `/v1/appStoreVersions/${version.id}/appStoreVersionLocalizations`))
  .data;
for (const [locale, l] of Object.entries(listing.locales)) {
  const vattrs = {
    description: l.description,
    keywords: l.keywords,
    promotionalText: l.promotionalText,
    supportUrl: listing.supportUrl,
    marketingUrl: listing.marketingUrl,
  };
  // "What's new" is refused on a first version.
  const v = vlocs.find((x) => x.attributes.locale === locale);
  if (v) {
    await asc('PATCH', `/v1/appStoreVersionLocalizations/${v.id}`, {
      data: { type: 'appStoreVersionLocalizations', id: v.id, attributes: vattrs },
    });
  } else {
    await asc('POST', '/v1/appStoreVersionLocalizations', {
      data: {
        type: 'appStoreVersionLocalizations',
        attributes: { locale, ...vattrs },
        relationships: { appStoreVersion: { data: { type: 'appStoreVersions', id: version.id } } },
      },
    });
  }
  const iattrs = { subtitle: l.subtitle, privacyPolicyUrl: listing.privacyUrl };
  // Re-read: creating a version localisation creates the app-info one too.
  const ilocs = (await asc('GET', `/v1/appInfos/${info.id}/appInfoLocalizations`)).data;
  const i = ilocs.find((x) => x.attributes.locale === locale);
  if (i) {
    await asc('PATCH', `/v1/appInfoLocalizations/${i.id}`, {
      data: { type: 'appInfoLocalizations', id: i.id, attributes: iattrs },
    });
  } else {
    await asc('POST', '/v1/appInfoLocalizations', {
      data: {
        type: 'appInfoLocalizations',
        attributes: {
          locale,
          name: listing.names?.[locale] ?? 'Vitals: System Monitor',
          ...iattrs,
        },
        relationships: { appInfo: { data: { type: 'appInfos', id: info.id } } },
      },
    });
  }
  log('localised', locale);
}

// Category: Utilities (primary), Developer Tools (secondary).
await asc('PATCH', `/v1/appInfos/${info.id}`, {
  data: {
    type: 'appInfos',
    id: info.id,
    relationships: {
      primaryCategory: { data: { type: 'appCategories', id: 'UTILITIES' } },
      secondaryCategory: { data: { type: 'appCategories', id: 'DEVELOPER_TOOLS' } },
    },
  },
});
log('category UTILITIES / DEVELOPER_TOOLS');

// No third-party content.
await asc('PATCH', `/v1/apps/${APP}`, {
  data: {
    type: 'apps',
    id: APP,
    attributes: { contentRightsDeclaration: 'DOES_NOT_USE_THIRD_PARTY_CONTENT' },
  },
});
log('content rights: no third-party content');

// Age rating: a system monitor has none of the rated content.
const age = (await asc('GET', `/v1/appInfos/${info.id}/ageRatingDeclaration`)).data;
const NONE = 'NONE';
await asc('PATCH', `/v1/ageRatingDeclarations/${age.id}`, {
  data: {
    type: 'ageRatingDeclarations',
    id: age.id,
    attributes: {
      alcoholTobaccoOrDrugUseOrReferences: NONE,
      contests: NONE,
      gamblingSimulated: NONE,
      horrorOrFearThemes: NONE,
      matureOrSuggestiveThemes: NONE,
      medicalOrTreatmentInformation: NONE,
      profanityOrCrudeHumor: NONE,
      sexualContentGraphicAndNudity: NONE,
      sexualContentOrNudity: NONE,
      violenceCartoonOrFantasy: NONE,
      violenceRealistic: NONE,
      violenceRealisticProlongedGraphicOrSadistic: NONE,
      gunsOrOtherWeapons: NONE,
      gambling: false,
      lootBox: false,
      unrestrictedWebAccess: false,
      messagingAndChat: false,
      userGeneratedContent: false,
      advertising: false,
      healthOrWellnessTopics: false,
      parentalControls: false,
      ageAssurance: false,
      socialMedia: false,
    },
  },
}).then(
  () => log('age rating: none'),
  (e) => log('age rating NOT set:', e.message),
);

// Review details. The contact email/phone come from the environment so they
// never sit in the repository.
const rd = listing.review;
const reviewAttrs = {
  contactFirstName: rd.contactFirstName,
  contactLastName: rd.contactLastName,
  contactEmail: process.env.VITALS_REVIEW_EMAIL,
  contactPhone: process.env.VITALS_REVIEW_PHONE,
  notes: rd.notes,
  demoAccountRequired: rd.demoAccountRequired,
};
if (!reviewAttrs.contactEmail || !reviewAttrs.contactPhone)
  throw new Error('set VITALS_REVIEW_EMAIL and VITALS_REVIEW_PHONE');
const existing = await raw('GET', `/v1/appStoreVersions/${version.id}/appStoreReviewDetail`);
if (existing.status === 200 && existing.json.data) {
  await asc('PATCH', `/v1/appStoreReviewDetails/${existing.json.data.id}`, {
    data: { type: 'appStoreReviewDetails', id: existing.json.data.id, attributes: reviewAttrs },
  });
} else {
  await asc('POST', '/v1/appStoreReviewDetails', {
    data: {
      type: 'appStoreReviewDetails',
      attributes: reviewAttrs,
      relationships: { appStoreVersion: { data: { type: 'appStoreVersions', id: version.id } } },
    },
  });
}
log('review details set');

// Free, every territory.
// The schedule object always exists; whether a price is set shows only in
// its manualPrices, which answer 404 until one is.
const prices = await raw('GET', `/v1/appPriceSchedules/${APP}/manualPrices?limit=1`);
if (prices.status !== 200 || !prices.json.data?.length) {
  const points = await asc('GET', `/v1/apps/${APP}/appPricePoints?filter[territory]=USA&limit=200`);
  const free = points.data.find(
    (p) => p.attributes.customerPrice === '0.0' || p.attributes.customerPrice === '0',
  );
  await asc('POST', '/v1/appPriceSchedules', {
    data: {
      type: 'appPriceSchedules',
      relationships: {
        app: { data: { type: 'apps', id: APP } },
        baseTerritory: { data: { type: 'territories', id: 'USA' } },
        manualPrices: { data: [{ type: 'appPrices', id: '${price}' }] },
      },
    },
    included: [
      {
        type: 'appPrices',
        id: '${price}',
        attributes: { startDate: null },
        relationships: { appPricePoint: { data: { type: 'appPricePoints', id: free.id } } },
      },
    ],
  });
  log('price: free');
} else {
  log('price schedule already set');
}
