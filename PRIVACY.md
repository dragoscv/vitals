# Privacy Policy

**Effective date:** 17 October 2026 · **Applies to:** Vitals 1.0.x and later:
the Windows desktop app, the `vitals` command-line tool (including
`vitals serve`), the optional sensors service and lag watchdog, the Android
phone, Wear OS and Google TV apps, and the Samsung (Tizen) TV app ·
[Politica de confidențialitate în limba română](#politica-de-confidențialitate)

**For app store reviewers:** Vitals collects no personal data, has no
telemetry, analytics, accounts or crash reporting, and the developer receives
no data. The only automatic network request is an update check to GitHub,
which the user can turn off. On Android, the QR scanner (Google ML Kit) sends
Google anonymous diagnostics about itself; see
[Vitals for Android, Wear OS and Google TV](#vitals-for-android-wear-os-and-google-tv).

## In short

- Vitals is a free, open-source (MIT) system monitor and task manager for
  Windows, with companion apps for Android phones, Wear OS watches, Google TV
  and Samsung TVs. Everything it measures stays on your devices.
- There is no telemetry, no analytics, no advertising, no account and no
  crash reporting. **The developer does not receive any data from you or
  your computer.**
- The one automatic connection is an update check to GitHub about 20 seconds
  after launch. You can turn it off in Settings → About. The Microsoft Store
  version makes no such request; the Store updates it.
- Remote access (viewing your PC from a phone on your network) is off until
  you turn it on.

## Who is responsible

The controller for the only processing described here (the update check) is:

|             |                                                                  |
| ----------- | ---------------------------------------------------------------- |
| Name        | Dragos Catalin Vladulescu (natural person, maintainer of Vitals) |
| Country     | Romania                                                          |
| Contact     | [dragoscv12@gmail.com](mailto:dragoscv12@gmail.com)              |
| Source code | [github.com/dragoscv/vitals](https://github.com/dragoscv/vitals) |
| Website     | [vitals.dragoscatalin.ro](https://vitals.dragoscatalin.ro)       |

## Network connections

| Connection                           | When                                                                                                                       | What the other side receives                                                                                                                                           | How to stop it                                                                                                            |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Update check to GitHub               | About 20 s after launch, when "Install updates automatically" is on (default: on); also when you click "Check for updates" | Your IP address, the User-Agent, the app version, target and architecture                                                                                              | Settings → About → turn off "Install updates automatically"                                                               |
| Update download from GitHub          | When an update is available                                                                                                | The same as above                                                                                                                                                      | As above                                                                                                                  |
| External links (GitHub, the website) | Only when you click one                                                                                                    | What your browser sends to any website                                                                                                                                 | Do not click                                                                                                              |
| "Search online"                      | Only when you click it, on a process, app, device or startup entry                                                         | That **name** (process, app, device or startup entry), sent to DuckDuckGo in your browser                                                                              | Do not click                                                                                                              |
| PawnIO download from GitHub          | Only when you install the optional sensors service (see [Sensors service](#sensors-service-optional))                      | The same as for the update check                                                                                                                                       | Do not install the sensors service                                                                                        |
| Microsoft WebView2 download          | Only by the installer, only if WebView2 is missing                                                                         | What Microsoft's download service receives                                                                                                                             | Install WebView2 beforehand                                                                                               |
| codai account (AI agents page)       | Only after you press "Sign in with codai"; then every 30 s while the AI agents page is open, and once when you sign out    | Your IP address, the app version and a read-only key labelled "Vitals desktop" (not your computer name). codai answers with your connected desktops and cloud sessions | Sign out on the Cloud tab (the key is revoked on codai and deleted from Credential Manager); uninstalling also deletes it |

Updates are verified with a minisign signature before they are installed,
downloaded in the background, and installed when you quit the app or after
ten minutes without keyboard or mouse input while only the tray icon is open.
The idle time is read locally from Windows and never leaves the PC.

GitHub is an independent controller for the data it receives. See the
[GitHub General Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
The developer does not receive GitHub's logs of these requests. DuckDuckGo is
an independent controller for searches you send it; see the
[DuckDuckGo privacy policy](https://duckduckgo.com/privacy).

## What listens on your computer

| Listener                                  | State                       | Who can reach it                                                                                                                                                                                                                                                                          |
| ----------------------------------------- | --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Local API on `127.0.0.1:7330`             | Always on while Vitals runs | Only programs on this computer, under any account on it. It is bound to loopback and cannot be reached from the network. Without a token it only reads metrics and the process list, never controls anything and never serves the AI agents page; the `vitals` command-line tool uses it. |
| Named pipe `vitals-<USERNAME>`            | Always on while Vitals runs | Your user account and administrators                                                                                                                                                                                                                                                      |
| Remote access (LAN server, TCP port 7331) | **Off by default**          | Devices on the network you chose; most of it only with a token from pairing (below)                                                                                                                                                                                                       |

When remote access is on:

- The server listens on the network adapter you chose in Settings → Remote
  access. If you did not choose one, it listens on every network the PC is
  connected to (including VPNs and virtual adapters). `vitals serve` on the
  command line always listens on every network and prints its token once.
- Two addresses answer without a token: `/api/v1/health` (the app version
  and that something is listening) and `/api/v1/pair` (exchanging the
  six-digit code shown on the PC for a token; one code, five guesses, five
  minutes). Everything else, including the Prometheus endpoint `/metrics`
  and the web page's data, needs a valid token.
- Vitals advertises the computer name and app version on your local network
  over mDNS (`_vitals._tcp`).
- A paired device can read process names, the user accounts that own them,
  resource use, hardware and operating system details, the computer name and
  network adapter MAC addresses.
- A token with control scope can also end, suspend and resume processes and
  set their priority. New pairings are read-only by default.
- Tokens are stored on the PC only as SHA-256 hashes (`lan-tokens.json`). On
  the phone the token is kept in the browser's local storage, or, in the
  Android app, in a file encrypted with a key held in the Android Keystore.
- Traffic is plain HTTP and is **not** end-to-end encrypted. Anyone able to
  watch your local network can read it. Use remote access only on networks
  you trust.

## What is stored on your computer

Everything below stays on your computer. Unless noted, files are under
`%LOCALAPPDATA%\Vitals`.

| File                                         | Contents                                                                                         | When                                         | Retention                                                |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------ | -------------------------------------------- | -------------------------------------------------------- |
| `history.sqlite`                             | Machine-wide metrics only                                                                        | Only when history is on (**off by default**) | 7 days by default; 1, 7, 30 or 90 days; capped at 512 MB |
| Flight recorder (in memory)                  | The last 120 frames: process names and resource use, without user account names or MAC addresses | While running                                | Cleared at each launch; saved only if you export it      |
| `app-history.json`                           | Per-app usage, including executable paths                                                        | Only when history is on                      | Until you delete it                                      |
| `startup-impact.json`                        | Executable paths of programs running during the boot window                                      | Automatically                                | Until you delete it                                      |
| `lan-tokens.json`                            | SHA-256 hashes of remote-access tokens                                                           | When you pair a device                       | Until you revoke the token                               |
| `local-api.json`                             | Local API port, process ID and version                                                           | While running                                | Overwritten at each launch                               |
| `logs\vitals.log`                            | Diagnostic log                                                                                   | While running                                | Capped at about 2–4 MB and rotated                       |
| `logs\crash.txt`                             | Details of the last crash                                                                        | After a crash                                | Only the last crash is kept                              |
| `logs\crash.dmp`                             | Thread stacks and loaded modules at the last native crash (no memory contents)                   | After a native crash                         | Only the last crash is kept                              |
| `settings.json` (in the app's config folder) | Your settings                                                                                    | When you change a setting                    | Until you delete it                                      |
| Exports (CSV, JSON, flight recordings)       | What you chose to export                                                                         | Only when you export                         | Wherever you saved them                                  |

Nothing in these files is sent anywhere by Vitals.

## Vitals for Android, Wear OS and Google TV

The Android phone app, the Wear OS watch app and the Google TV app monitor
the device they run on and, if you pair one, a PC running Vitals on your
local network. The phone and TV apps come in two editions: the one from
Google Play (`play`) and the APK published on GitHub (`github`). They differ
only in all-files access, which only the GitHub edition can request.

**What the apps read on the device.** Processor, graphics, memory, battery,
temperature, storage, network and sensor readings, through Android's public
interfaces. The phone and TV apps also list the apps installed on the device
that have a launcher icon (their names, versions, and install and update
times), which Android allows without any special access; the list stays on
the device. Two readings need special access that you grant yourself in
Android's settings, and each stays empty until you do:

| Access                                       | What it enables                                                                        | Where it stays |
| -------------------------------------------- | -------------------------------------------------------------------------------------- | -------------- |
| Usage access (`PACKAGE_USAGE_STATS`)         | Time in each app and each app's data use, on the Apps tab                              | On the device  |
| All-files access (`MANAGE_EXTERNAL_STORAGE`) | The storage map, largest folders and cleanup of files you choose (GitHub edition only) | On the device  |

The watch app does not request either. Deleting a file is always your
explicit action, confirmed on screen.

**Network connections.**

| Connection                            | When                                  | What the other side receives                                                                                                                                                                                                                                                                                       |
| ------------------------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Pairing with a PC                     | When you enter the PC's pairing code  | The code and a name for this device (the device's name unless you type another), so the PC can show which device holds the token                                                                                                                                                                                   |
| To a PC you paired                    | While a screen or widget shows it     | The pairing token; plain HTTP on your local network, as described under remote access above. Sent only while the device is on Wi-Fi, Ethernet or a VPN, never over mobile data; redirects are not followed                                                                                                         |
| mDNS discovery (`_vitals._tcp`)       | On the Add PC screen                  | Devices on your local network see the query                                                                                                                                                                                                                                                                        |
| Wake-on-LAN                           | Only when you tap Wake up             | A broadcast packet on your local network containing the PC's MAC address, sent only to a private-network broadcast address                                                                                                                                                                                         |
| Google ML Kit diagnostics             | When you open the QR scanner          | Google receives device model and OS version, app package and version, a per-installation identifier not meant to identify you, and performance and error data about the scanner. The camera image never leaves the device. See [Google's disclosure](https://developers.google.com/ml-kit/android-data-disclosure) |
| Phone to watch (Google Play services) | When a watch is paired with the phone | Your paired PCs and their latest readings, sent through the Wearable Data Layer, device to device. A PC's token reaches the watch only if the pairing is read-only; a pairing that can end apps keeps its token on the phone, which acts for the watch                                                             |

The apps contain no analytics, advertising or crash reporting of their own
and make no other connections. Google Play and Android may collect data
about app installs under Google's own privacy policy.

**What the apps store.** All in the app's private storage, excluded from
Android backup and deleted when you uninstall:

| Data                                                          | Contents                                                                                                                                                            | Retention                            |
| ------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------ |
| Paired PCs                                                    | Address, name and token, encrypted with an Android Keystore key                                                                                                     | Until you remove the PC or uninstall |
| Device history (on by default, can be turned off in Settings) | Machine-wide readings of this device (processor, memory, battery, network), one a minute while the app is open and every 15 minutes in the background; no app names | 7 days                               |
| Settings and widget cache                                     | Your choices and the last values a widget showed                                                                                                                    | Until you uninstall                  |

**Permissions.** Every permission the apps declare, by app:

| Permission                                                                         | Phone               | TV                  | Watch | Why                                                                                                                 |
| ---------------------------------------------------------------------------------- | ------------------- | ------------------- | ----- | ------------------------------------------------------------------------------------------------------------------- |
| `INTERNET`, `ACCESS_NETWORK_STATE`, `ACCESS_WIFI_STATE`                            | Yes                 | Yes                 | Yes   | Talking to paired PCs on your network; the network readings                                                         |
| `CHANGE_WIFI_MULTICAST_STATE`                                                      | Yes                 | Yes                 | No    | Finding PCs with mDNS                                                                                               |
| `CAMERA`                                                                           | Yes                 | No                  | No    | Only to scan the pairing QR code                                                                                    |
| `POST_NOTIFICATIONS`                                                               | Yes                 | No                  | Yes   | Alerts you set up, and this phone's own alerts while device history is on; asked the first time you open This phone |
| `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_SPECIAL_USE`                             | Yes                 | No                  | No    | Watching a PC in the background, only while you have turned that on                                                 |
| `VIBRATE`                                                                          | Yes                 | No                  | No    | Vibrating on an alert (the watch vibrates through its notification channels)                                        |
| `PACKAGE_USAGE_STATS` (special access)                                             | Yes                 | Yes                 | No    | Time and data per app, once you grant it                                                                            |
| `MANAGE_EXTERNAL_STORAGE` (special access), `READ_EXTERNAL_STORAGE` (≤ Android 10) | GitHub edition only | GitHub edition only | No    | The storage map and cleanup, once you grant it                                                                      |

## Changes Vitals makes to Windows, only when you ask

- **Replace Task Manager:** writes the `Debugger` value for `taskmgr.exe`
  under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File
Execution Options`. The uninstaller removes it if it names Vitals.
- **Start with Windows:** writes a value under the current user's `Run` key
  (`HKCU`).
- **Lag watchdog (off until you turn it on):** a small separate program,
  `vitals-watchdog.exe`, that watches for windows that stop responding and
  tells you which program is behind a freeze. It never ends anything unless
  you press a button. Vitals does not turn it on by itself: you choose it on
  the welcome card or in Settings → Lag watchdog. When it is on:
  - it starts when you sign in, through the `VitalsWatchdog` value under the
    current user's `Run` key (`HKCU`), or the Store package's startup task;
  - its settings are in `%APPDATA%\Vitals\watchdog.json`;
  - it keeps a log of what it noticed, including **process names**, in
    `%LOCALAPPDATA%\Vitals\watchdog\watchdog.log` (started again past 1 MB);
  - if it was installed by hand with `vitals-watchdog --install`, a copy of
    the program is kept in `%LOCALAPPDATA%\Vitals\watchdog`.

  Nothing it records is sent anywhere. To remove it, turn it off in
  Settings (this removes the sign-in entry), sign out or end
  `vitals-watchdog.exe`, and delete the files above.

- **Elevated process actions:** run through a Windows UAC prompt.
- **Uninstalling an app:** runs that app's own uninstaller.

## Sensors service (optional)

Windows does not let a normal program read CPU temperature and power. If you
choose **Devices & sensors → CPU temperature and power → Install** (one
administrator prompt), Vitals:

- downloads `PawnIO_setup.exe` 2.2.0 from
  [github.com/namazso/PawnIO.Setup](https://github.com/namazso/PawnIO.Setup)
  (GitHub receives what it receives for the update check), checks it against
  a SHA-256 hash built into Vitals and refuses any other file;
- runs it, which installs the PawnIO **kernel driver**, published and signed
  by its own author;
- installs a small service, `vitals-sensors`, that runs as **SYSTEM** from
  `C:\Program Files\Vitals Sensors`, starts with Windows, and serves only
  temperature, power and fan readings, read-only, to programs on this
  computer through the `vitals-sensors` named pipe. Its install log is in the
  same folder.

Nothing is sent anywhere. Uninstalling Vitals, or choosing **Remove** there,
stops and deletes the service and its folder. **PawnIO stays installed**,
because other tools (such as FanControl and LibreHardwareMonitor) use the
same driver. To remove it, uninstall **PawnIO** from Windows Settings → Apps →
Installed apps, once no other program needs it. The Microsoft Store edition
does not offer the sensors service.

## The Samsung TV app (Tizen)

The Samsung TV app shows PCs you pair with it. It stores the paired PCs
(address, name and token) and your language choice in the app's own storage
on the TV, deleted when you uninstall the app. It connects only to the PCs
you pair, over plain HTTP on your local network, sending the pairing code and
then the token; it contains no analytics or advertising. Samsung may collect
data about the TV and app installs under its own privacy policy.

## Legal basis

The update check is based on legitimate interest (Article 6(1)(f) GDPR):
keeping software that controls processes on your computer secure and up to
date. You can object at any time by turning the setting off. The controller
carries out no other processing of personal data through Vitals. Local
storage and remote access happen on your own device under your control.

## Your rights

Under the GDPR you have the right to access, rectify and erase your personal
data, to restrict or object to its processing, and to data portability. In
practice the developer holds no data about you; the data Vitals stores is on
your computer and you can view or delete it yourself. To exercise a right or
ask a question, write to [dragoscv12@gmail.com](mailto:dragoscv12@gmail.com).

You may lodge a complaint with the Romanian supervisory authority, ANSPDCP
(Autoritatea Națională de Supraveghere a Prelucrării Datelor cu Caracter
Personal), [www.dataprotection.ro](https://www.dataprotection.ro), or with
the authority in the EU country where you live.

## How to remove everything

Uninstalling Vitals removes the program, the sensors service and the
sign-in entries it created, and keeps your data. To delete everything:

1. In Vitals, revoke any remote-access tokens and turn off the lag watchdog
   and "Start with Windows" (these remove their sign-in entries).
2. Uninstall Vitals from Windows Settings → Apps.
3. Delete these folders:

- `%LOCALAPPDATA%\Vitals` (history, logs, tokens, the watchdog log);
- `%APPDATA%\Vitals` (watchdog settings);
- `%APPDATA%\app.vitals.desktop` (settings and dashboard layout);
- `%LOCALAPPDATA%\app.vitals.desktop` (the WebView2 browser data,
  `EBWebView`);
- `C:\Program Files\Vitals Sensors`, if it is still there.

4. If they are still there, delete the values `Vitals` and `VitalsWatchdog`
   under `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`.
5. If you installed the sensors service and no other program needs it,
   uninstall **PawnIO** from Windows Settings → Apps.
6. Clear the site data for Vitals in your phone's browser.

On Android, Wear OS, Google TV and Samsung TVs, uninstalling the app deletes
everything it stored.

## The website

[vitals.dragoscatalin.ro](https://vitals.dragoscatalin.ro) is hosted on
GitHub Pages. GitHub logs visitors' IP addresses; the site uses no cookies and
no analytics.

## Children

Vitals is not directed at children.

## Changes to this policy

Changes are published in this file in the repository, with a new effective
date, and noted in the [changelog](CHANGELOG.md). A change that adds a new
kind of data or connection will be announced in the release notes before it
ships.

---

# Politica de confidențialitate

**Data intrării în vigoare:** 17 octombrie 2026 · **Se aplică:** Vitals 1.0.x
și versiunile ulterioare: aplicația desktop pentru Windows, instrumentul în
linie de comandă `vitals` (inclusiv `vitals serve`), serviciul opțional de
senzori și watchdogul pentru blocaje, aplicațiile Android pentru telefon,
Wear OS și Google TV, și aplicația pentru televizoare Samsung (Tizen)

**Pentru evaluatorii magazinelor de aplicații:** Vitals nu colectează date
personale, nu are telemetrie, analiză, conturi sau raportare a erorilor, iar
dezvoltatorul nu primește nicio dată. Singura cerere automată în rețea este
verificarea actualizărilor pe GitHub, pe care utilizatorul o poate dezactiva.
Pe Android, scanerul de coduri QR (Google ML Kit) trimite către Google date de
diagnosticare anonime despre el însuși; vedeți
[Vitals pentru Android, Wear OS și Google TV](#vitals-pentru-android-wear-os-și-google-tv).

## Pe scurt

- Vitals este un monitor de sistem și manager de activități gratuit și open
  source (MIT) pentru Windows, cu aplicații însoțitoare pentru telefoane
  Android, ceasuri Wear OS, Google TV și televizoare Samsung. Tot ce măsoară
  rămâne pe dispozitivele dvs.
- Nu există telemetrie, analiză, publicitate, cont sau raportare a erorilor.
  **Dezvoltatorul nu primește nicio dată de la dvs. sau de la calculatorul
  dvs.**
- Singura conexiune automată este verificarea actualizărilor pe GitHub, la
  aproximativ 20 de secunde după pornire. O puteți dezactiva din Setări →
  Despre.
- Accesul la distanță (vizualizarea PC-ului de pe telefon, în rețeaua dvs.)
  este oprit până îl porniți.

## Cine este responsabil

Operatorul pentru singura prelucrare descrisă aici (verificarea
actualizărilor) este:

|           |                                                                    |
| --------- | ------------------------------------------------------------------ |
| Nume      | Dragos Catalin Vladulescu (persoană fizică, întreținătorul Vitals) |
| Țara      | România                                                            |
| Contact   | [dragoscv12@gmail.com](mailto:dragoscv12@gmail.com)                |
| Cod sursă | [github.com/dragoscv/vitals](https://github.com/dragoscv/vitals)   |
| Site      | [vitals.dragoscatalin.ro](https://vitals.dragoscatalin.ro)         |

## Conexiuni în rețea

| Conexiune                             | Când                                                                                                                                          | Ce primește cealaltă parte                                                                                                                                                         | Cum o opriți                                                                                                              |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Verificarea actualizărilor pe GitHub  | La aproximativ 20 s după pornire, când „Instalează actualizările automat” este activă (implicit: activă); și când apăsați „Caută actualizări” | Adresa IP, User-Agent, versiunea, ținta și arhitectura aplicației                                                                                                                  | Setări → Despre → dezactivați „Instalează actualizările automat”                                                          |
| Descărcarea actualizării de pe GitHub | Când există o actualizare                                                                                                                     | La fel ca mai sus                                                                                                                                                                  | Ca mai sus                                                                                                                |
| Linkuri externe (GitHub, site-ul)     | Doar când apăsați pe unul                                                                                                                     | Ce trimite browserul oricărui site                                                                                                                                                 | Nu apăsați                                                                                                                |
| „Search online” (căutare online)      | Doar când apăsați, pe un proces, o aplicație, un dispozitiv sau o intrare de pornire                                                          | Acel **nume** (proces, aplicație, dispozitiv sau intrare de pornire), trimis către DuckDuckGo în browser                                                                           | Nu apăsați                                                                                                                |
| Descărcarea PawnIO de pe GitHub       | Doar când instalați serviciul opțional de senzori (vedeți [Serviciul de senzori](#serviciul-de-senzori-opțional))                             | La fel ca la verificarea actualizărilor                                                                                                                                            | Nu instalați serviciul de senzori                                                                                         |
| Descărcarea Microsoft WebView2        | Doar de către programul de instalare, doar dacă WebView2 lipsește                                                                             | Ce primește serviciul de descărcare Microsoft                                                                                                                                      | Instalați WebView2 în prealabil                                                                                           |
| Contul codai (pagina Agenți AI)       | Doar după ce apăsați „Conectează-te cu codai”; apoi la fiecare 30 s cât pagina Agenți AI e deschisă și o dată la deconectare                  | Adresa IP, versiunea aplicației și o cheie doar pentru citire numită „Vitals desktop” (nu numele calculatorului). codai răspunde cu desktop-urile conectate și sesiunile din cloud | Deconectați-vă din fila Cloud (cheia e revocată la codai și ștearsă din Credential Manager); dezinstalarea o șterge și ea |

Actualizările sunt verificate cu o semnătură minisign înainte de instalare,
sunt descărcate în fundal și se instalează când închideți aplicația.

GitHub este operator independent pentru datele pe care le primește. Vedeți
[Declarația generală de confidențialitate GitHub](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
Dezvoltatorul nu primește jurnalele GitHub ale acestor cereri. DuckDuckGo este
operator independent pentru căutările pe care i le trimiteți; vedeți
[politica de confidențialitate DuckDuckGo](https://duckduckgo.com/privacy).

## Ce ascultă pe calculatorul dvs.

| Punct de ascultare                            | Stare                           | Cine îl poate accesa                                                                                                                                                                                                                                                                        |
| --------------------------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| API local pe `127.0.0.1:7330`                 | Mereu pornit cât rulează Vitals | Doar programele de pe acest calculator, sub orice cont de pe el. Este legat de loopback și nu poate fi accesat din rețea. Fără token doar citește valori și lista de procese, nu controlează nimic și nu servește pagina Agenți AI; instrumentul în linie de comandă `vitals` îl folosește. |
| Named pipe `vitals-<USERNAME>`                | Mereu pornit cât rulează Vitals | Contul dvs. de utilizator și administratorii                                                                                                                                                                                                                                                |
| Acces la distanță (server LAN, port TCP 7331) | **Oprit implicit**              | Dispozitivele din rețeaua aleasă; aproape totul doar cu un token obținut la asociere (mai jos)                                                                                                                                                                                              |

Când accesul la distanță este pornit:

- Serverul ascultă pe adaptorul de rețea ales în Setări → Acces la distanță.
  Dacă nu ați ales unul, ascultă pe toate rețelele la care este conectat
  PC-ul (inclusiv VPN-uri și adaptoare virtuale). `vitals serve` din linia de
  comandă ascultă mereu pe toate rețelele și afișează tokenul o singură dată.
- Două adrese răspund fără token: `/api/v1/health` (versiunea aplicației și
  faptul că ceva ascultă) și `/api/v1/pair` (schimbul codului de șase cifre
  afișat pe PC pe un token; un cod, cinci încercări, cinci minute). Tot
  restul, inclusiv punctul Prometheus `/metrics` și datele paginii web, cere
  un token valid.
- Vitals anunță numele calculatorului și versiunea aplicației în rețeaua
  locală prin mDNS (`_vitals._tcp`).
- Un dispozitiv asociat poate citi numele proceselor, conturile de utilizator
  care le dețin, consumul de resurse, detalii despre hardware și sistemul de
  operare, numele calculatorului și adresele MAC ale adaptoarelor de rețea.
- Un token cu drept de control poate, în plus, să închidă, să suspende și să
  reia procese și să le schimbe prioritatea. Asocierile noi sunt implicit
  doar de citire.
- Pe PC, tokenurile sunt stocate doar ca hash-uri SHA-256
  (`lan-tokens.json`). Pe telefon, tokenul este păstrat în stocarea locală a
  browserului sau, în aplicația Android, într-un fișier criptat cu o cheie
  păstrată în Android Keystore.
- Traficul este HTTP simplu și **nu** este criptat end-to-end. Oricine poate
  urmări rețeaua locală îl poate citi. Folosiți accesul la distanță doar în
  rețele în care aveți încredere.

## Ce se stochează pe calculatorul dvs.

Tot ce urmează rămâne pe calculatorul dvs. Dacă nu se precizează altfel,
fișierele se află în `%LOCALAPPDATA%\Vitals`.

| Fișier                                                    | Conținut                                                                                             | Când                                                 | Păstrare                                                 |
| --------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------- | -------------------------------------------------------- |
| `history.sqlite`                                          | Doar valori la nivelul întregului sistem                                                             | Doar când istoricul este pornit (**oprit implicit**) | Implicit 7 zile; 1, 7, 30 sau 90 de zile; maximum 512 MB |
| Înregistrator de zbor (în memorie)                        | Ultimele 120 de cadre: numele proceselor și consumul de resurse, fără nume de conturi sau adrese MAC | Cât rulează                                          | Golit la fiecare pornire; salvat doar dacă îl exportați  |
| `app-history.json`                                        | Utilizarea pe aplicație, inclusiv căile executabilelor                                               | Doar când istoricul este pornit                      | Până îl ștergeți                                         |
| `startup-impact.json`                                     | Căile executabilelor programelor care rulează în intervalul de pornire                               | Automat                                              | Până îl ștergeți                                         |
| `lan-tokens.json`                                         | Hash-uri SHA-256 ale tokenurilor de acces la distanță                                                | Când asociați un dispozitiv                          | Până revocați tokenul                                    |
| `local-api.json`                                          | Portul API-ului local, ID-ul procesului și versiunea                                                 | Cât rulează                                          | Suprascris la fiecare pornire                            |
| `logs\vitals.log`                                         | Jurnal de diagnosticare                                                                              | Cât rulează                                          | Limitat la aproximativ 2–4 MB, cu rotație                |
| `logs\crash.txt`                                          | Detalii despre ultima eroare fatală                                                                  | După o eroare fatală                                 | Se păstrează doar ultima                                 |
| `logs\crash.dmp`                                          | Stivele firelor și modulele încărcate la ultima cădere nativă (fără conținutul memoriei)             | După o cădere nativă                                 | Se păstrează doar ultima                                 |
| `settings.json` (în dosarul de configurare al aplicației) | Setările dvs.                                                                                        | Când schimbați o setare                              | Până îl ștergeți                                         |
| Exporturi (CSV, JSON, înregistrări de zbor)               | Ce ați ales să exportați                                                                             | Doar când exportați                                  | Unde le-ați salvat                                       |

Vitals nu trimite nicăieri nimic din aceste fișiere.

## Vitals pentru Android, Wear OS și Google TV

Aplicația pentru telefon Android, aplicația pentru ceas Wear OS și aplicația
pentru Google TV monitorizează dispozitivul pe care rulează și, dacă asociați
unul, un PC cu Vitals din rețeaua locală. Aplicațiile pentru telefon și TV au
două ediții: cea din Google Play (`play`) și APK-ul publicat pe GitHub
(`github`). Diferă doar prin accesul la toate fișierele, pe care numai ediția
GitHub îl poate cere.

**Ce citesc aplicațiile pe dispozitiv.** Procesor, placă grafică, memorie,
baterie, temperatură, stocare, rețea și senzori, prin interfețele publice ale
Android. Aplicațiile pentru telefon și TV listează și aplicațiile instalate
care au pictogramă în lansator (numele, versiunile și datele de instalare și
actualizare), ceea ce Android permite fără acces special; lista rămâne pe
dispozitiv. Două citiri au nevoie de acces special, pe care îl acordați
dumneavoastră din setările Android; fiecare rămâne goală până atunci:

| Acces                                                | Ce permite                                                                                      | Unde rămâne   |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------------- | ------------- |
| Acces la utilizare (`PACKAGE_USAGE_STATS`)           | Timpul petrecut în fiecare aplicație și datele folosite de ea, în fila Aplicații                | Pe dispozitiv |
| Acces la toate fișierele (`MANAGE_EXTERNAL_STORAGE`) | Harta stocării, cele mai mari dosare și curățarea fișierelor alese de dvs. (doar ediția GitHub) | Pe dispozitiv |

Aplicația pentru ceas nu le cere. Ștergerea unui fișier este mereu o acțiune
explicită a dvs., confirmată pe ecran.

**Conexiuni în rețea.**

| Conexiune                                 | Când                                          | Ce primește cealaltă parte                                                                                                                                                                                                                                                                                                   |
| ----------------------------------------- | --------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Asocierea cu un PC                        | Când introduceți codul de asociere al PC-ului | Codul și un nume pentru acest dispozitiv (numele dispozitivului, dacă nu scrieți altul), ca PC-ul să arate ce dispozitiv deține tokenul                                                                                                                                                                                      |
| Către un PC asociat                       | Cât timp un ecran sau un widget îl afișează   | Tokenul de asociere; HTTP simplu în rețeaua locală, ca la accesul la distanță de mai sus                                                                                                                                                                                                                                     |
| Descoperire mDNS (`_vitals._tcp`)         | Pe ecranul Adaugă PC                          | Dispozitivele din rețeaua locală văd interogarea                                                                                                                                                                                                                                                                             |
| Wake-on-LAN                               | Doar când apăsați Trezește                    | Un pachet broadcast în rețeaua locală, cu adresa MAC a PC-ului                                                                                                                                                                                                                                                               |
| Diagnosticare Google ML Kit               | Când deschideți scanerul QR                   | Google primește modelul și versiunea sistemului, pachetul și versiunea aplicației, un identificator per instalare care nu vă identifică și date de performanță și erori ale scanerului. Imaginea camerei nu părăsește dispozitivul. Vedeți [declarația Google](https://developers.google.com/ml-kit/android-data-disclosure) |
| Telefon către ceas (servicii Google Play) | Când un ceas este asociat cu telefonul        | PC-urile asociate și ultimele lor valori, prin Wearable Data Layer, de la dispozitiv la dispozitiv                                                                                                                                                                                                                           |

Aplicațiile nu conțin instrumente proprii de analiză, publicitate sau
raportare a erorilor și nu fac alte conexiuni. Google Play și Android pot
colecta date despre instalări conform politicii de confidențialitate Google.

**Ce stochează aplicațiile.** Totul în stocarea privată a aplicației, exclusă
din backupul Android și ștearsă la dezinstalare:

| Date                                                                 | Conținut                                                                                                                                                                  | Păstrare                              |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| PC-uri asociate                                                      | Adresa, numele și tokenul, criptate cu o cheie Android Keystore                                                                                                           | Până eliminați PC-ul sau dezinstalați |
| Istoricul dispozitivului (pornit implicit, se poate opri din Setări) | Valori la nivelul întregului dispozitiv (procesor, memorie, baterie, rețea), una pe minut cât aplicația este deschisă și la 15 minute în fundal; fără numele aplicațiilor | 7 zile                                |
| Setări și cache pentru widgeturi                                     | Alegerile dvs. și ultimele valori afișate de un widget                                                                                                                    | Până la dezinstalare                  |

**Permisiuni.** Toate permisiunile declarate de aplicații, pe aplicație:

| Permisiune                                                                        | Telefon            | TV                 | Ceas | De ce                                                         |
| --------------------------------------------------------------------------------- | ------------------ | ------------------ | ---- | ------------------------------------------------------------- |
| `INTERNET`, `ACCESS_NETWORK_STATE`, `ACCESS_WIFI_STATE`                           | Da                 | Da                 | Da   | Comunicarea cu PC-urile asociate din rețea; citirile de rețea |
| `CHANGE_WIFI_MULTICAST_STATE`                                                     | Da                 | Da                 | Da   | Găsirea PC-urilor prin mDNS                                   |
| `CAMERA`                                                                          | Da                 | Nu                 | Nu   | Doar pentru scanarea codului QR de asociere                   |
| `POST_NOTIFICATIONS`                                                              | Da                 | Nu                 | Da   | Alertele pe care le configurați                               |
| `FOREGROUND_SERVICE`, `FOREGROUND_SERVICE_SPECIAL_USE`                            | Da                 | Nu                 | Nu   | Urmărirea unui PC în fundal, doar cât ați pornit-o            |
| `WAKE_LOCK`, `VIBRATE`                                                            | Doar vibrații      | Nu                 | Da   | Menținerea la zi a unei citiri pe ceas; vibrația la o alertă  |
| `PACKAGE_USAGE_STATS` (acces special)                                             | Da                 | Da                 | Nu   | Timp și date pe aplicație, după ce îl acordați                |
| `MANAGE_EXTERNAL_STORAGE` (acces special), `READ_EXTERNAL_STORAGE` (≤ Android 10) | Doar ediția GitHub | Doar ediția GitHub | Nu   | Harta stocării și curățarea, după ce îl acordați              |

## Modificări pe care Vitals le face în Windows, doar la cererea dvs.

- **Înlocuirea Managerului de activități:** scrie valoarea `Debugger` pentru
  `taskmgr.exe` în `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image
File Execution Options`. Programul de dezinstalare o elimină dacă indică
  spre Vitals.
- **Pornire odată cu Windows:** scrie o valoare în cheia `Run` a
  utilizatorului curent (`HKCU`).
- **Watchdog pentru blocaje (oprit până îl porniți):** un mic program separat,
  `vitals-watchdog.exe`, care observă ferestrele care nu mai răspund și vă
  spune ce program se află în spatele unui blocaj. Nu închide nimic decât
  dacă apăsați un buton. Vitals nu îl pornește singur: îl alegeți din cardul
  de bun venit sau din Setări → Watchdog pentru blocaje. Când este pornit:
  - pornește la conectare, prin valoarea `VitalsWatchdog` din cheia `Run` a
    utilizatorului curent (`HKCU`) sau prin sarcina de pornire a pachetului
    din Store;
  - setările lui sunt în `%APPDATA%\Vitals\watchdog.json`;
  - păstrează un jurnal cu ce a observat, inclusiv **numele proceselor**, în
    `%LOCALAPPDATA%\Vitals\watchdog\watchdog.log` (reluat de la zero peste
    1 MB);
  - dacă a fost instalat manual cu `vitals-watchdog --install`, o copie a
    programului este păstrată în `%LOCALAPPDATA%\Vitals\watchdog`.

  Nimic din ce înregistrează nu este trimis nicăieri. Pentru a-l elimina,
  opriți-l din Setări (asta șterge intrarea de la conectare), deconectați-vă
  sau închideți `vitals-watchdog.exe` și ștergeți fișierele de mai sus.

- **Acțiuni cu drepturi ridicate asupra proceselor:** trec prin solicitarea
  UAC a Windows.
- **Dezinstalarea unei aplicații:** rulează programul de dezinstalare al
  acelei aplicații.

## Serviciul de senzori (opțional)

Windows nu permite unui program obișnuit să citească temperatura și puterea
procesorului. Dacă alegeți **Dispozitive și senzori → Temperatura și puterea
procesorului → Instalează** (o singură solicitare de administrator), Vitals:

- descarcă `PawnIO_setup.exe` 2.2.0 de pe
  [github.com/namazso/PawnIO.Setup](https://github.com/namazso/PawnIO.Setup)
  (GitHub primește ce primește la verificarea actualizărilor), îl verifică
  după un hash SHA-256 inclus în Vitals și refuză orice alt fișier;
- îl rulează, ceea ce instalează **driverul de kernel** PawnIO, publicat și
  semnat de autorul lui;
- instalează un mic serviciu, `vitals-sensors`, care rulează ca **SYSTEM**
  din `C:\Program Files\Vitals Sensors`, pornește odată cu Windows și oferă
  doar citiri de temperatură, putere și ventilatoare, numai pentru citire,
  programelor de pe acest calculator, prin named pipe-ul `vitals-sensors`.
  Jurnalul de instalare este în același dosar.

Nu se trimite nimic nicăieri. Dezinstalarea Vitals, sau alegerea
**Elimină** în același loc, oprește și șterge serviciul și dosarul lui.
**PawnIO rămâne instalat**, pentru că alte instrumente (de exemplu FanControl
și LibreHardwareMonitor) folosesc același driver. Pentru a-l elimina,
dezinstalați **PawnIO** din Setări Windows → Aplicații → Aplicații
instalate, când niciun alt program nu mai are nevoie de el. Ediția din
Microsoft Store nu oferă serviciul de senzori.

## Aplicația pentru televizoare Samsung (Tizen)

Aplicația pentru televizoare Samsung afișează PC-urile pe care le asociați cu
ea. Păstrează PC-urile asociate (adresa, numele și tokenul) și limba aleasă
în stocarea proprie a aplicației pe televizor, ștearsă la dezinstalare. Se
conectează doar la PC-urile asociate, prin HTTP simplu în rețeaua locală,
trimițând codul de asociere și apoi tokenul; nu conține instrumente de
analiză sau publicitate. Samsung poate colecta date despre televizor și
instalări conform propriei politici de confidențialitate.

## Temeiul juridic

Verificarea actualizărilor se bazează pe interesul legitim (articolul 6
alineatul (1) litera (f) din GDPR): menținerea în siguranță și la zi a unui
software care controlează procesele de pe calculatorul dvs. Vă puteți opune
oricând dezactivând setarea. Operatorul nu efectuează nicio altă prelucrare de
date cu caracter personal prin Vitals. Stocarea locală și accesul la distanță
au loc pe propriul dispozitiv, sub controlul dvs.

## Drepturile dvs.

Conform GDPR, aveți dreptul de acces, de rectificare și de ștergere a datelor
cu caracter personal, dreptul la restricționarea prelucrării, dreptul de
opoziție și dreptul la portabilitatea datelor. În practică, dezvoltatorul nu
deține date despre dvs.; datele stocate de Vitals se află pe calculatorul dvs.
și le puteți vedea sau șterge singur. Pentru a vă exercita un drept sau
pentru întrebări, scrieți la
[dragoscv12@gmail.com](mailto:dragoscv12@gmail.com).

Puteți depune o plângere la Autoritatea Națională de Supraveghere a
Prelucrării Datelor cu Caracter Personal (ANSPDCP),
[www.dataprotection.ro](https://www.dataprotection.ro), sau la autoritatea
din țara UE în care locuiți.

## Cum ștergeți totul

Dezinstalarea Vitals elimină programul, serviciul de senzori și intrările de
la conectare create de el și păstrează datele. Pentru a șterge totul:

1. În Vitals, revocați tokenurile de acces la distanță și opriți watchdogul
   pentru blocaje și „Pornire odată cu Windows” (acestea își șterg intrările
   de la conectare).
2. Dezinstalați Vitals din Setări Windows → Aplicații.
3. Ștergeți aceste dosare:

- `%LOCALAPPDATA%\Vitals` (istoric, jurnale, tokenuri, jurnalul
  watchdogului);
- `%APPDATA%\Vitals` (setările watchdogului);
- `%APPDATA%\app.vitals.desktop` (setările și aranjamentul panoului);
- `%LOCALAPPDATA%\app.vitals.desktop` (datele browserului WebView2,
  `EBWebView`);
- `C:\Program Files\Vitals Sensors`, dacă mai există.

4. Dacă mai există, ștergeți valorile `Vitals` și `VitalsWatchdog` din
   `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run`.
5. Dacă ați instalat serviciul de senzori și niciun alt program nu are nevoie
   de el, dezinstalați **PawnIO** din Setări Windows → Aplicații.
6. Ștergeți datele site-ului Vitals din browserul telefonului.

Pe Android, Wear OS, Google TV și televizoarele Samsung, dezinstalarea
aplicației șterge tot ce a stocat.

## Site-ul

[vitals.dragoscatalin.ro](https://vitals.dragoscatalin.ro) este găzduit pe
GitHub Pages. GitHub înregistrează adresele IP ale vizitatorilor; site-ul nu
folosește cookie-uri și nici instrumente de analiză.

## Copii

Vitals nu se adresează copiilor.

## Modificări ale acestei politici

Modificările sunt publicate în acest fișier din depozit, cu o nouă dată de
intrare în vigoare, și sunt menționate în [jurnalul de modificări](CHANGELOG.md).
O modificare care adaugă un nou tip de date sau o nouă conexiune va fi
anunțată în notele de lansare înainte de publicare.
