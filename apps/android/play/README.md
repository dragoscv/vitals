# Google Play

What the Play Console is told about Vitals, kept here so the answers change
in the same commit as the behaviour they describe. `listing/` holds the store
text per locale in the layout Gradle Play Publisher and fastlane read
(`title.txt` ≤ 30, `short_description.txt` ≤ 80, `full_description.txt` ≤ 4000).

- **App:** `app.vitals`, id 4973500332718114251. One listing, three form
  factors: the phone, Wear OS and Android TV builds share the application id
  and the signing key, which the Wearable Data Layer requires (ADR-0033,
  ADR-0036). versionCodes: phone = run number, watch 1 000 000 +, TV 2 000 000 +.
- **Privacy policy:** <https://vitals.dragoscatalin.ro/privacy/> (the URL
  Play has, also linked from Settings → About), summarising
  [PRIVACY.md](../../../PRIVACY.md), whose "Vitals for Android and Wear OS"
  section is authoritative.
- **Bundles:** the `play-bundles` artefact of `release.yml` (signed AABs).
  Play App Signing holds the app key; the key in `ANDROID_KEYSTORE_B64` is the
  upload key, which also signs the APKs published on GitHub for Obtainium.

## Publishing

| What                   | Where                                                                                                                                                                                                                      |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Internal testers       | Email list "Hobby apps testers", shared with the other hobby apps; attached to both the phone and the Wear OS internal tracks                                                                                              |
| Phone internal track   | Track 4701269647694141701 (`qa` in the API); first release versionCode 8, 2026-09-29                                                                                                                                       |
| Wear OS internal track | Track 4697898405601153351 (`wear:qa`); first release versionCode 1000008, 2026-09-29; Wear OS form factor opted in                                                                                                         |
| Android TV track       | `tv:qa` (internal testing); Android TV form factor opted in; banner `tv/src/main/res/drawable/tv_banner.xml`                                                                                                               |
| CI upload              | `play` job in `release.yml`, `v*` tags with `PUBLISH_PLAY=true`: stable tag → `production` / `wear:production` / `tv:production` as a staged rollout (`PLAY_ROLLOUT`, default 0.2); pre-release tag → internal tracks      |
| CI identity            | `vitals-play@hai-small-apps.iam.gserviceaccount.com` through Workload Identity Federation, provider `github/vitals`; no key exists                                                                                         |
| Its Play permissions   | This app only: view app information, release to testing tracks **and release to production** (owner decision 2026-10-14; granted in Play Console → Users and permissions). A staged rollout can be halted from the Console |
| Release notes          | `listing/<locale>/changelogs/default.txt` (≤ 500 chars), copied to `whatsnew-<locale>` by the `play` job                                                                                                                   |

The provider's attribute condition is
`assertion.repository == 'dragoscv/vitals' && assertion.ref.startsWith('refs/tags/v')`,
so a branch push or a pull request cannot obtain a token even if the job were
changed to run there.

## App content answers

| Question              | Answer                                                                                                                                                                                                  |
| --------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ads                   | No                                                                                                                                                                                                      |
| App access            | All functionality is available without an account or special access; pairing a PC is optional                                                                                                           |
| Target audience       | 18 and over (a system utility, not designed for children)                                                                                                                                               |
| Content rating (IARC) | Utility; no violence, sexuality, language, controlled substances, gambling, user interaction or sharing of location; expected: Everyone / 3+                                                            |
| News app              | No                                                                                                                                                                                                      |
| Government app        | No                                                                                                                                                                                                      |
| Financial features    | None                                                                                                                                                                                                    |
| Health app            | **Yes, since 1.3.0** — the optional Health tab shows the owner's own health and nutrition summary, read from a vmui server the user runs. Nothing reaches the developer; see _Health declaration_ below |
| Category              | Tools                                                                                                                                                                                                   |
| Foreground service    | `specialUse`: "Monitoring a paired PC on the local network", only while the user chooses to watch a PC in the background                                                                                |

## Special access declarations

Optional; every reading it gates stays empty until granted (ADR-0035).

- **`PACKAGE_USAGE_STATS`** — core feature: per-app screen time and data use
  on the Apps tab. Not shared, not sent anywhere.
- **`MANAGE_EXTERNAL_STORAGE` — not in the Play build.** 1.0.1 was rejected on
  2026-10-14: "Not a core feature" (All files access). The `play` flavour no
  longer declares it (the release workflow fails if it reappears); the folder
  scan lives only in the `github` APK. If the Console still shows an "All files
  access" declaration, answer that the app no longer uses the permission.

## Data safety answers

| Question                                 | Answer                                                                                                                                                                                                                                                                                                                                                      |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Does the app collect or share user data? | Yes — Google ML Kit (the QR scanner), and the Health tab while it is connected (see the last two rows)                                                                                                                                                                                                                                                      |
| Data types                               | App info and performance → Diagnostics; Device or other IDs (per-installation, not identifying a person)                                                                                                                                                                                                                                                    |
| Collected or shared                      | Collected (ML Kit sends to Google as a service provider; not shared with third parties)                                                                                                                                                                                                                                                                     |
| Processed ephemerally                    | No                                                                                                                                                                                                                                                                                                                                                          |
| Required or optional                     | Optional: only when the user opens the QR scanner                                                                                                                                                                                                                                                                                                           |
| Purpose                                  | Analytics (ML Kit's own diagnostics of the scanner)                                                                                                                                                                                                                                                                                                         |
| Encrypted in transit                     | Yes (HTTPS)                                                                                                                                                                                                                                                                                                                                                 |
| Deletion request                         | Not applicable: the developer receives nothing; uninstalling deletes all local data                                                                                                                                                                                                                                                                         |
| Traffic to the user's own PC             | Not declared: it goes to a device the user controls on their own network, never to the developer or a third party                                                                                                                                                                                                                                           |
| Health tab (vmui)                        | The phone asks a vmui server the user runs themselves (address + token the user pastes) for the owner's health and nutrition summary. The developer hosts no such server, receives nothing and can access nothing, so by Play's definition ("transmitted off the device **in a way that allows you to access it**") this is not data the developer collects |
| Data type if declared                    | Health and fitness → Health info, purpose App functionality, not shared with third parties, encrypted in transit, optional (the tab is hidden until a vmui is connected and can be disconnected at any time), deletion is the user's own server                                                                                                             |

Re-check these whenever a dependency that talks to the network is added, and
whenever [ML Kit's disclosure](https://developers.google.com/ml-kit/android-data-disclosure)
changes.

### What to select in the Console

The Console asks these in **Data collection and security**; the bold word is
the value to choose.

| Console question                                                      | Select                                                                                                                                                                                                                        |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Does your app collect or share any of the required user data types?   | **Yes**                                                                                                                                                                                                                       |
| Is all of the user data collected by your app encrypted in transit?   | **Yes**                                                                                                                                                                                                                       |
| Do you provide a way for users to request that their data is deleted? | **No** — there is no account and no server; the developer holds nothing and uninstalling the app deletes everything on the device. (If the Console insists on a URL for a "Yes", use the controller address in `PRIVACY.md`.) |

Then, in **Data types**, declare exactly these and nothing else:

| Data type                                                     | Collected or shared | Required or optional | Purpose           | Processed ephemerally |
| ------------------------------------------------------------- | ------------------- | -------------------- | ----------------- | --------------------- |
| App info and performance → **Diagnostics**                    | Collected           | Optional             | Analytics         | No                    |
| **Device or other IDs**                                       | Collected           | Optional             | Analytics         | No                    |
| Health and fitness → **Health info** (only if Q4 answers yes) | Collected           | Optional             | App functionality | No                    |

- **Which code backs the rows.** ML Kit is `com.google.mlkit:barcode-scanning`
  17.3.0 (`gradle/libs.versions.toml:21`, used at `app/build.gradle.kts:139`)
  and runs only in `app/src/main/kotlin/app/vitals/phone/ui/add/QrScanner.kt`;
  the Health read is HTTPS-only in
  `core/src/main/kotlin/app/vitals/core/health/HealthClient.kt`
  (`SchemePolicy.Secure`, `fetch` → `GET /api/v1/me/health`).
- **Not shared.** ML Kit sends to Google as a service provider processing on
  the developer's behalf, which Play counts as collection, not sharing.
- **Optional** on both counts: the scanner runs only when the user opens it,
  the Health tab only while a vmui is connected.
- **Leave undeclared.** Play's "collect" test — transmitted off the device in a
  way that allows you to access it — does not apply to anything that stays on
  the phone: App activity → Installed apps and App interactions (the
  launcher-icon list, and per-app screen time and data use under
  `PACKAGE_USAGE_STATS`), and Location, Personal info, Financial info,
  Messages, Photos and videos, Audio, Files and docs, Calendar, Contacts and
  Web browsing, none of which the app reads. The pairing code and this device's
  name reach only the user's own PC on the LAN
  (`core/src/main/kotlin/app/vitals/core/pairing/PairingStore.kt`).

## Health declaration

Play asks for a health declaration before an app that touches health data can
ship. 1.3.0 answers:

| Question                      | Answer                                                                                                                        |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Does the app use health data? | Yes — the optional Health tab (health and nutrition summary)                                                                  |
| Where does it come from?      | A vmui server the user runs; the user pairs it with their own token. There is no developer server and no developer account    |
| Is it required?               | No: the tab appears only while a vmui is connected, and disconnecting removes it                                              |
| Is it advertised or sold?     | No: no ads, no analytics, no research use, no sharing                                                                         |
| Does it give advice?          | No — the tab says "Not medical advice. Readings come from your vmui and may be incomplete." (`health_not_medical` in the app) |

Question Q4 of `docs/legal/LAWYER-CHECKLIST.md` is exactly "does reading data
from a user's own server count as health and fitness data collected". If the
answer is no, the Data safety form stays "not collected" and this table only
needs the declaration above; if it is yes, the two extra Data safety rows
already written here are what the form should say.

### What to select in the Console (Health)

The Console's health section ("App content" → "Health apps") takes these
values; the `health_*` names are the strings the tab renders.

| Console question                                         | Select                                                                                                                                                                                                                                                                                             |
| -------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Does your app access, collect, use or share health data? | **Yes** — the optional Health tab                                                                                                                                                                                                                                                                  |
| Which health data is read?                               | Steps, distance, floors, active and total energy, heart rate, resting heart rate, HRV, blood oxygen, breathing rate, blood pressure, VO2 max, sleep, weight, body fat, resting energy, BMI, muscle, body water, and nutrition totals (`health_*` in `app/src/main/res/values/strings.xml:419-478`) |
| Why is it read?                                          | **App functionality** only — to show the owner their own readings                                                                                                                                                                                                                                  |
| Where does it come from?                                 | A vmui server the user runs; the app makes one request, `GET /api/v1/me/health` (`HealthClient.fetch`)                                                                                                                                                                                             |
| Is it shared with third parties?                         | **No** — straight from the device to the user's own vmui                                                                                                                                                                                                                                           |
| Is it used for advertising, analytics or research?       | **No**                                                                                                                                                                                                                                                                                             |
| Is it required for the app to work?                      | **No** — optional, hidden until a vmui is connected                                                                                                                                                                                                                                                |
| Is your app a medical device?                            | **No** — informational only, with no diagnosis, interpretation or advice, matching `TERMS.md` § "Health readings are not medical advice"                                                                                                                                                           |
| Does the app use Health Connect?                         | **No** — the app neither reads nor writes Health Connect; readings come from the user's own vmui over `GET /api/v1/me/health`                                                                                                                                                                      |

What keeps this outside MDR 2017/745 is question Q8 of
`docs/legal/LAWYER-CHECKLIST.md`, and the copy it turns on is
`health_not_medical` ("Not medical advice. Readings come from your vmui and may
be incomplete.").

## What a reviewer needs to exercise this

The app ships no server of its own, so two things can only come from the owner,
and the Console review step should name them rather than imply they are
reachable:

- **A demo vmui server and account** for the Health tab, to exercise the
  "Health info" row and the health declaration end to end. Without a vmui the
  tab stays hidden (`health_settings_body`, `app/src/main/res/values/strings.xml`)
  and the reviewer cannot reach it. Supply the address and a pairing code or an
  owner-approved token; the readings are the owner's own. This matches the iOS
  review note, "A demo vmui address and account can be supplied on request"
  (`apps/ios/store/listing.json`).
- **A PC running Vitals** on the reviewer's network for the PC pane, the
  widgets and the `specialUse` foreground service — paired by QR code or the
  six-digit code, with remote access on in the PC's Settings.

Everything else is reachable without an account: the device panes, the Apps
tab (after the reviewer grants usage access), and the QR scanner with its ML
Kit request.
