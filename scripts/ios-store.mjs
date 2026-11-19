// Applies a store listing to an App Store Connect record: version and
// app-info localisations (EN + RO), URLs, copyright, category, content
// rights, age rating, review details and free pricing. iPhone by default
// (apps/ios/store/listing.json); `--platform macos` applies
// apps/desktop/store/macos-listing.json to the Mac App Store record.
// Credentials: the team's ASC key, from the env vars APPLE_ASC_KEY_P8,
// APPLE_ASC_KEY_ID and APPLE_ASC_ISSUER, else Secret Manager
// (APPLE_SECRETS_PROJECT, default dragos-devcloud). The client is
// `scripts/lib/asc.mjs`, repo-local because `--check` runs in CI, where the
// machine-local ~/.copilot/bin/apple-asc.mjs does not exist. Nothing secret is
// printed.
//
//   node scripts/ios-store.mjs            apply and print what changed
//   node scripts/ios-store.mjs --check    prove the record is complete (read-only)
//   node scripts/ios-store.mjs --check --all           both records, one report
//   node scripts/ios-store.mjs --check --json          the same report, machine-readable
//   node scripts/ios-store.mjs --platform macos
//
// `--check` reads back every record a submission needs — availability, whether
// the version has a build, the per-locale version text and screenshots (and
// whether that text still matches the listing file), the review contact and a
// price schedule — prints them as a table and exits non-zero listing whatever is
// missing. It writes nothing, so it is safe to run at any time. A released
// version with no availability record makes the app uninstallable (2026-11-18),
// which is why that probe exists. The audit mints one App Store Connect token
// for the whole run rather than one per request.
import { readFileSync } from 'node:fs';
import { asc as raw, ascToken } from './lib/asc.mjs';
const asc = async (method, path, body) => {
  const r = await raw(method, path, body);
  if (r.status >= 300) {
    const detail = (r.json.errors ?? []).map((e) => `${e.code}: ${e.detail}`).join('; ');
    throw new Error(`${method} ${path} -> ${r.status} ${detail}`);
  }
  return r.json;
};

const argv = process.argv;
const check = argv.includes('--check');
const asJson = argv.includes('--json');
const all = argv.includes('--all');
const mac = argv.includes('--platform') && argv.includes('macos');
// In `--json` mode stdout carries the report and nothing else, so a consumer
// can parse it; the human lines go through `log` and are suppressed.
const log = (...a) => {
  if (!asJson) console.log(...a);
};
const emit = (...a) => console.log(...a);

// One entry per App Store record this script can touch: `--check` audits one, or
// both with `--all`, and the apply path edits exactly one. `optional` marks a
// record whose gaps are reported but never fail a combined run — the Mac app is
// not on the Mac App Store by decision (D6, a notarised DMG), which
// scripts/check-store-listing.mjs states the same way.
const RECORDS = {
  ios: { app: '6827931747', platform: 'IOS', listing: '../apps/ios/store/listing.json' },
  macos: {
    app: '6827932703',
    platform: 'MAC_OS',
    listing: '../apps/desktop/store/macos-listing.json',
    optional: true,
  },
};
const targets = all ? ['ios', 'macos'] : [mac ? 'macos' : 'ios'];
if (all && !check) throw new Error('--all audits both records and needs --check');
const APP = RECORDS[targets[0]].app;
const PLATFORM = RECORDS[targets[0]].platform;
const loadListing = (path) => JSON.parse(readFileSync(new URL(path, import.meta.url), 'utf8'));
const listing = loadListing(RECORDS[targets[0]].listing);

// The limits App Store Connect enforces. Discovering one of them at submission
// costs a whole upload round, so they are checked here first. Characters for
// the prose fields (a Romanian ǎ is one character but two bytes), bytes for
// keywords, which is what the listing file's own comment states.
const LIMITS = { subtitle: 30, promotionalText: 170, description: 4000 };
const MAX_KEYWORD_BYTES = 100;
const MAX_NAME_CHARS = 30;
const checkLimits = (spec) => {
  for (const [locale, l] of Object.entries(spec.locales)) {
    for (const [field, max] of Object.entries(LIMITS)) {
      const chars = [...(l[field] ?? '')].length;
      if (chars > max) throw new Error(`${locale}: ${field} is ${chars} characters, limit ${max}`);
    }
    const keywords = Buffer.byteLength(l.keywords ?? '', 'utf8');
    if (keywords > MAX_KEYWORD_BYTES)
      throw new Error(`${locale}: keywords are ${keywords} bytes, limit ${MAX_KEYWORD_BYTES}`);
    const name = spec.names?.[locale];
    if (name && [...name].length > MAX_NAME_CHARS)
      throw new Error(`${locale}: name is over ${MAX_NAME_CHARS} characters`);
  }
  log(`listing limits ok for ${Object.keys(spec.locales).length} locale(s)`);
};
for (const key of targets) checkLimits(loadListing(RECORDS[key].listing));

// The states in which App Store Connect still lets us change a version. The
// apply path edits the first one; that selection is unchanged.
const EDITABLE = ['PREPARE_FOR_SUBMISSION', 'DEVELOPER_REJECTED', 'REJECTED', 'METADATA_REJECTED'];

// The apply path: the editable version, and the app info that is not yet live.
// Read-only runs skip both, so a record with nothing editable — a build waiting
// for review, as on 2026-11-18 — still gets audited instead of exiting on
// "no editable version", which is what the incident needed and did not get.
const version = check
  ? undefined
  : (
      await asc('GET', `/v1/apps/${APP}/appStoreVersions?filter[platform]=${PLATFORM}&limit=5`)
    ).data.find((v) => EDITABLE.includes(v.attributes.appStoreState));
if (!check && !version) throw new Error(`no editable ${PLATFORM} version`);
const info = check
  ? undefined
  : (await asc('GET', `/v1/apps/${APP}/appInfos`)).data.find(
      (i) => i.attributes.appStoreState !== 'READY_FOR_SALE',
    );
if (!check) log('version', version.attributes.versionString, version.attributes.appStoreState);

if (check) {
  // A released version is not a listed app. On 2026-11-18 the iPhone record had
  // versions READY_FOR_SALE and no availability at all, so App Store Connect read
  // "removed from sale" and the product page 404'd. Everything below is a
  // read-only probe: each record is printed as a table and any missing item
  // makes the run exit non-zero.
  //
  // apple-asc.mjs mints a fresh token — three Secret Manager reads — for every
  // request it makes, so an audit of a dozen requests spawns ~36 gcloud
  // processes and has been seen to stall for minutes on a busy machine. One
  // token, read once, serves the whole audit.
  const ASC_API = 'https://api.appstoreconnect.apple.com';
  const { token } = ascToken();
  const call = async (path) => {
    const res = await fetch(ASC_API + path, {
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(60_000),
    });
    const text = await res.text();
    let body = {};
    try {
      body = text ? JSON.parse(text) : {};
    } catch {
      body = { raw: text };
    }
    return { status: res.status, json: body };
  };
  const need = async (path) => {
    const r = await call(path);
    if (r.status >= 300) {
      const detail = (r.json.errors ?? []).map((e) => `${e.code}: ${e.detail}`).join('; ');
      throw new Error(`GET ${path} -> ${r.status} ${detail}`);
    }
    return r.json;
  };

  const audit = async (key) => {
    const { app, platform, listing: listingPath } = RECORDS[key];
    const spec = loadListing(listingPath);
    // A gap in an `optional` record is reported, but only fails the run when
    // that record is the one that was asked for.
    const enforced = !RECORDS[key].optional || !all;
    const checks = [];
    const drift = [];
    const add = (name, value, ok) => checks.push({ name, value, ok });

    // The version about to go live. One request answers both this and whether
    // any version is live, which decides if "what's new" is required.
    const versions = (
      await need(`/v1/apps/${app}/appStoreVersions?filter[platform]=${platform}&limit=50`)
    ).data;
    const version =
      versions.find((v) => EDITABLE.includes(v.attributes.appStoreState)) ??
      versions.find((v) => v.attributes.appStoreState === 'WAITING_FOR_REVIEW');
    if (!version) throw new Error(`no editable or in-review ${platform} version`);
    const state = version.attributes.appStoreState;
    const live = versions.some((v) => v.attributes.appStoreState === 'READY_FOR_SALE');

    // Availability. A 404 here means nobody can install the app.
    const avail = await call(`/v1/apps/${app}/appAvailabilityV2`);
    const listed = avail.status === 200 && Boolean(avail.json.data);
    add(
      'availability',
      listed ? 'present' : 'MISSING (appAvailabilityV2 → 404: nobody can install the app)',
      listed,
    );

    // A version with no build cannot be reviewed, however complete its text is.
    const buildRes = await call(`/v1/appStoreVersions/${version.id}/build`);
    const build = buildRes.status === 200 ? buildRes.json.data : undefined;
    add(
      'build',
      build
        ? `build ${build.attributes.version} (${build.attributes.processingState})`
        : 'MISSING (no build attached — the version cannot be reviewed)',
      Boolean(build),
    );

    const localisations = (
      await need(`/v1/appStoreVersions/${version.id}/appStoreVersionLocalizations`)
    ).data;
    const required = ['description', 'keywords', 'promotionalText', ...(live ? ['whatsNew'] : [])];
    for (const locale of Object.keys(spec.locales)) {
      const l = spec.locales[locale];
      const v = localisations.find((x) => x.attributes.locale === locale);
      const attrs = v?.attributes ?? {};
      const blank = required.filter((f) => !(attrs[f] ?? '').trim());
      add(
        `${locale} text`,
        !v
          ? 'no localisation'
          : blank.length
            ? `missing ${blank.join(', ')}`
            : `set (${required.join(', ')})`,
        Boolean(v) && blank.length === 0,
      );

      // An apply that silently failed leaves the record holding older copy.
      // Reported, never counted as missing: staleness is not incompleteness.
      if (v) {
        const differs = required.filter((f) => l[f] !== undefined && (attrs[f] ?? '') !== l[f]);
        drift.push({
          name: `${locale} copy`,
          value: differs.length
            ? `differs from the listing file: ${differs.join(', ')}`
            : 'in step with the listing file',
          ok: differs.length === 0,
        });
      }

      // At least one screenshot set with at least one image in it. The display
      // types are named so the required device sizes are visible, not just a count.
      const sets = v
        ? (await need(`/v1/appStoreVersionLocalizations/${v.id}/appScreenshotSets?limit=50`)).data
        : [];
      const types = [];
      let shots = 0;
      for (const set of sets) {
        const n = (await need(`/v1/appScreenshotSets/${set.id}/appScreenshots?limit=50`)).data
          .length;
        shots += n;
        if (n > 0) types.push(set.attributes.screenshotDisplayType);
      }
      add(
        `${locale} screenshots`,
        types.length ? `${types.join(', ')} (${shots} image(s))` : 'none',
        shots > 0,
      );
    }

    // Review contact: a version cannot be submitted without an email and phone.
    const rd = await call(`/v1/appStoreVersions/${version.id}/appStoreReviewDetail`);
    const rda = rd.status === 200 ? (rd.json.data?.attributes ?? {}) : {};
    add(
      'review contact',
      rda.contactEmail && rda.contactPhone ? 'email + phone' : 'MISSING email or phone',
      Boolean(rda.contactEmail && rda.contactPhone),
    );

    // Price schedule. The schedule object always exists; a set price shows only
    // in its manualPrices, which answer 404 until one is.
    const prices = await call(`/v1/appPriceSchedules/${app}/manualPrices?limit=1`);
    const priced = prices.status === 200 && prices.json.data?.length > 0;
    add('price schedule', priced ? 'set' : 'MISSING (no price set)', priced);

    return {
      key,
      platform,
      version: version.attributes.versionString,
      state,
      live,
      enforced,
      checks,
      drift,
      missing: checks.filter((c) => !c.ok).map(({ name, value }) => ({ name, value })),
    };
  };

  const results = [];
  for (const key of targets) results.push(await audit(key));

  if (asJson) {
    emit(
      JSON.stringify(
        {
          records: results,
          complete: results.every((r) => !r.enforced || r.missing.length === 0),
        },
        null,
        2,
      ),
    );
  } else {
    for (const r of results) {
      log(
        `\n${r.platform} ${r.version} · ${r.state} · ${r.live ? 'has a live version' : 'first version'}`,
      );
      const width = Math.max(...r.checks.map((c) => c.name.length));
      for (const c of r.checks) log(`  ${c.name.padEnd(width)}  ${c.value}`);
      for (const d of r.drift) log(`  ${d.name.padEnd(width)}  ${d.value}`);
      if (r.missing.length) {
        const note = r.enforced ? 'INCOMPLETE' : 'INCOMPLETE (reported, does not fail this run)';
        log(`\n${note} — ${r.missing.length} item(s) missing:`);
        for (const m of r.missing) log(`  - ${m.value}`);
      } else {
        log('\nCOMPLETE — every App Store record the submission needs is present.');
      }
    }
  }

  // The exit code follows the records that were enforced, so `--all` does not
  // fail on the Mac record that is deliberately not on the Mac App Store.
  process.exit(results.some((r) => r.enforced && r.missing.length) ? 1 : 0);
}

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
    // App Store Connect refuses "What's new" while the app has no live
    // version, so it is sent only when the listing carries one. Without this
    // line the field is silently ignored: apps/ios/store/listing.json has
    // always had a whatsNew and nothing ever applied it.
    ...(l.whatsNew ? { whatsNew: l.whatsNew } : {}),
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
