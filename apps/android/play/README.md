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
