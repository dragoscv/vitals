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

| What                   | Where                                                                                                                               |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Internal testers       | Email list "Hobby apps testers", shared with the other hobby apps; attached to both the phone and the Wear OS internal tracks       |
| Phone internal track   | Track 4701269647694141701 (`qa` in the API); first release versionCode 8, 2026-09-29                                                |
| Wear OS internal track | Track 4697898405601153351 (`wear:qa`); first release versionCode 1000008, 2026-09-29; Wear OS form factor opted in                  |
| Android TV track       | `tv:qa` (internal testing); Android TV form factor opted in; banner `tv/src/main/res/drawable/tv_banner.xml`                        |
| CI upload              | `play` job in `release.yml`: `v*` tags only, when repo variable `PUBLISH_PLAY` is `true`; all three tracks, status completed        |
| CI identity            | `vitals-play@hai-small-apps.iam.gserviceaccount.com` through Workload Identity Federation, provider `github/vitals`; no key exists  |
| Its Play permissions   | This app only: view app information, release to testing tracks. It cannot release to production — promotion stays a person's action |

The provider's attribute condition is
`assertion.repository == 'dragoscv/vitals' && assertion.ref.startsWith('refs/tags/v')`,
so a branch push or a pull request cannot obtain a token even if the job were
changed to run there.

## App content answers

| Question              | Answer                                                                                                                                       |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Ads                   | No                                                                                                                                           |
| App access            | All functionality is available without an account or special access; pairing a PC is optional                                                |
| Target audience       | 18 and over (a system utility, not designed for children)                                                                                    |
| Content rating (IARC) | Utility; no violence, sexuality, language, controlled substances, gambling, user interaction or sharing of location; expected: Everyone / 3+ |
| News app              | No                                                                                                                                           |
| Government app        | No                                                                                                                                           |
| Financial features    | None                                                                                                                                         |
| Health app            | No (it measures the device, not the person)                                                                                                  |
| Category              | Tools                                                                                                                                        |
| Foreground service    | `specialUse`: "Monitoring a paired PC on the local network", only while the user chooses to watch a PC in the background                     |

## Special access declarations

Both are optional; every reading they gate stays empty until granted (ADR-0035).

- **`PACKAGE_USAGE_STATS`** — core feature: per-app screen time and data use
  on the Apps tab. Not shared, not sent anywhere.
- **`MANAGE_EXTERNAL_STORAGE`** — core feature of a storage analyser: the
  storage map, largest folders and files, and user-reviewed cleanup, which
  needs to see every folder. This is the "file manager / disk analyser"
  permitted use. Play reviews this declaration by hand and may ask for a
  video: record the Storage tab granting access, scanning, and deleting a
  reviewed file.

## Data safety answers

| Question                                 | Answer                                                                                                            |
| ---------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| Does the app collect or share user data? | Yes — only through Google ML Kit (the QR scanner)                                                                 |
| Data types                               | App info and performance → Diagnostics; Device or other IDs (per-installation, not identifying a person)          |
| Collected or shared                      | Collected (ML Kit sends to Google as a service provider; not shared with third parties)                           |
| Processed ephemerally                    | No                                                                                                                |
| Required or optional                     | Optional: only when the user opens the QR scanner                                                                 |
| Purpose                                  | Analytics (ML Kit's own diagnostics of the scanner)                                                               |
| Encrypted in transit                     | Yes (HTTPS)                                                                                                       |
| Deletion request                         | Not applicable: the developer receives nothing; uninstalling deletes all local data                               |
| Traffic to the user's own PC             | Not declared: it goes to a device the user controls on their own network, never to the developer or a third party |

Re-check these whenever a dependency that talks to the network is added, and
whenever [ML Kit's disclosure](https://developers.google.com/ml-kit/android-data-disclosure)
changes.
