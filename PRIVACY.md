# Privacy Policy

**Effective date:** 29 September 2026 · **Applies to:** Vitals 0.9.0-beta.1 and
later, for Windows, Android and Wear OS · [Politica de confidențialitate în limba română](#politica-de-confidențialitate)

**For app store reviewers:** Vitals collects no personal data, has no
telemetry, analytics, accounts or crash reporting, and the developer receives
no data. The only automatic network request is an update check to GitHub,
which the user can turn off. On Android, the QR scanner (Google ML Kit) sends
Google anonymous diagnostics about itself; see
[Vitals for Android and Wear OS](#vitals-for-android-and-wear-os).

## In short

- Vitals is a free, open-source (MIT) system monitor and task manager for
  Windows. Everything it measures stays on your computer.
- There is no telemetry, no analytics, no advertising, no account and no
  crash reporting. **The developer does not receive any data from you or
  your computer.**
- The one automatic connection is an update check to GitHub about 20 seconds
  after launch. You can turn it off in Settings → About.
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

| Connection                           | When                                                                                                                       | What the other side receives                                              | How to stop it                                              |
| ------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- | ----------------------------------------------------------- |
| Update check to GitHub               | About 20 s after launch, when "Install updates automatically" is on (default: on); also when you click "Check for updates" | Your IP address, the User-Agent, the app version, target and architecture | Settings → About → turn off "Install updates automatically" |
| Update download from GitHub          | When an update is available                                                                                                | The same as above                                                         | As above                                                    |
| External links (GitHub, the website) | Only when you click one                                                                                                    | What your browser sends to any website                                    | Do not click                                                |
| "Search online" for a process        | Only when you click it                                                                                                     | The process **name**, sent to DuckDuckGo in your browser                  | Do not click                                                |
| Microsoft WebView2 download          | Only by the installer, only if WebView2 is missing                                                                         | What Microsoft's download service receives                                | Install WebView2 beforehand                                 |

Updates are verified with a minisign signature before they are installed,
downloaded in the background, and installed when you quit the app.

GitHub is an independent controller for the data it receives. See the
[GitHub General Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
The developer does not receive GitHub's logs of these requests.

## What listens on your computer

| Listener                                  | State                       | Who can reach it                                                                                                                                                                                                         |
| ----------------------------------------- | --------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Local API on `127.0.0.1:7330`             | Always on while Vitals runs | Only programs on this computer. It is bound to loopback and cannot be reached from the network. Programs running as your user can read metrics and control processes through it; the `vitals` command-line tool uses it. |
| Named pipe `vitals-<USERNAME>`            | Always on while Vitals runs | Your user account and administrators                                                                                                                                                                                     |
| Remote access (LAN server, TCP port 7331) | **Off by default**          | Devices on your local network that you have paired                                                                                                                                                                       |

When remote access is on:

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

| File                                         | Contents                                                                                         | When                                         | Retention                                                               |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------ | -------------------------------------------- | ----------------------------------------------------------------------- |
| `history.sqlite`                             | Machine-wide metrics only                                                                        | Only when history is on (**off by default**) | 7 days by default; 1, 7, 30 or 90 days; capped at 512 MB                |
| Flight recorder (in memory)                  | The last 120 frames: process names and resource use, without user account names or MAC addresses | While running                                | Cleared at each launch; saved only if you export it                     |
| `app-history.json`                           | Per-app usage, including executable paths                                                        | Only when history is on                      | Until you delete it                                                     |
| `startup-impact.json`                        | Executable paths of programs running during the boot window                                      | Automatically                                | Until you delete it                                                     |
| `storage-index\<drive>.idx`                  | Folder names and sizes of the last whole-drive storage scan, plus the names of its largest files | After you scan a whole drive                 | Replaced by the next scan of that drive; delete the folder to remove it |
| `lan-tokens.json`                            | SHA-256 hashes of remote-access tokens                                                           | When you pair a device                       | Until you revoke the token                                              |
| `local-api.json`                             | Local API port, process ID and version                                                           | While running                                | Overwritten at each launch                                              |
| `logs\vitals.log`                            | Diagnostic log                                                                                   | While running                                | Capped at about 2–4 MB and rotated                                      |
| `logs\crash.txt`                             | Details of the last crash                                                                        | After a crash                                | Only the last crash is kept                                             |
| `settings.json` (in the app's config folder) | Your settings                                                                                    | When you change a setting                    | Until you delete it                                                     |
| Exports (CSV, JSON, flight recordings)       | What you chose to export                                                                         | Only when you export                         | Wherever you saved them                                                 |

Nothing in these files is sent anywhere by Vitals.

## Vitals for Android and Wear OS

The Android phone app and the Wear OS watch app monitor the device they run
on and, if you pair one, a PC running Vitals on your local network.

**What the apps read on the device.** Processor, graphics, memory, battery,
temperature, storage, network and sensor readings, through Android's public
interfaces. Two readings need special access that you grant yourself in
Android's settings, and each stays empty until you do:

| Access                                       | What it enables                                                  | Where it stays |
| -------------------------------------------- | ---------------------------------------------------------------- | -------------- |
| Usage access (`PACKAGE_USAGE_STATS`)         | Time in each app and each app's data use, on the Apps tab        | On the device  |
| All-files access (`MANAGE_EXTERNAL_STORAGE`) | The storage map, largest folders and cleanup of files you choose | On the device  |

The watch app does not request either. Deleting a file is always your
explicit action, confirmed on screen.

**Network connections.**

| Connection                            | When                                  | What the other side receives                                                                                                                                                                                                                                                                                       |
| ------------------------------------- | ------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| To a PC you paired                    | While a screen or widget shows it     | The pairing token; plain HTTP on your local network, as described under remote access above                                                                                                                                                                                                                        |
| mDNS discovery (`_vitals._tcp`)       | On the Add PC screen                  | Devices on your local network see the query                                                                                                                                                                                                                                                                        |
| Wake-on-LAN                           | Only when you tap Wake up             | A broadcast packet on your local network containing the PC's MAC address                                                                                                                                                                                                                                           |
| Google ML Kit diagnostics             | When you open the QR scanner          | Google receives device model and OS version, app package and version, a per-installation identifier not meant to identify you, and performance and error data about the scanner. The camera image never leaves the device. See [Google's disclosure](https://developers.google.com/ml-kit/android-data-disclosure) |
| Phone to watch (Google Play services) | When a watch is paired with the phone | Your paired PCs and their latest readings, sent through the Wearable Data Layer, device to device                                                                                                                                                                                                                  |

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

**Permissions.** Camera (only to scan the pairing QR code), notifications
(alerts you set up), network state and Wi-Fi multicast (discovery),
vibration, and a foreground service that runs only while you watch a PC in
the background.

## Changes Vitals makes to Windows, only when you ask

- **Replace Task Manager:** writes the `Debugger` value for `taskmgr.exe`
  under `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File
Execution Options`. The uninstaller removes it if it names Vitals.
- **Start with Windows:** writes a value under the current user's `Run` key
  (`HKCU`).
- **Elevated process actions:** run through a Windows UAC prompt.
- **Uninstalling an app:** runs that app's own uninstaller.

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

Uninstalling Vitals keeps your data. To delete it, remove the
`%LOCALAPPDATA%\Vitals` folder after uninstalling. Revoke any remote-access
tokens first, and clear the site data for Vitals in your phone's browser.
On Android and Wear OS, uninstalling the app deletes everything it stored.

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

**Data intrării în vigoare:** 29 septembrie 2026 · **Se aplică:** Vitals
0.9.0-beta.1 și versiunile ulterioare, pentru Windows, Android și Wear OS

**Pentru evaluatorii magazinelor de aplicații:** Vitals nu colectează date
personale, nu are telemetrie, analiză, conturi sau raportare a erorilor, iar
dezvoltatorul nu primește nicio dată. Singura cerere automată în rețea este
verificarea actualizărilor pe GitHub, pe care utilizatorul o poate dezactiva.
Pe Android, scanerul de coduri QR (Google ML Kit) trimite către Google date de
diagnosticare anonime despre el însuși; vedeți
[Vitals pentru Android și Wear OS](#vitals-pentru-android-și-wear-os).

## Pe scurt

- Vitals este un monitor de sistem și manager de activități gratuit și open
  source (MIT) pentru Windows. Tot ce măsoară rămâne pe calculatorul dvs.
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

| Conexiune                                         | Când                                                                                                                                          | Ce primește cealaltă parte                                        | Cum o opriți                                                     |
| ------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- | ---------------------------------------------------------------- |
| Verificarea actualizărilor pe GitHub              | La aproximativ 20 s după pornire, când „Instalează actualizările automat” este activă (implicit: activă); și când apăsați „Caută actualizări” | Adresa IP, User-Agent, versiunea, ținta și arhitectura aplicației | Setări → Despre → dezactivați „Instalează actualizările automat” |
| Descărcarea actualizării de pe GitHub             | Când există o actualizare                                                                                                                     | La fel ca mai sus                                                 | Ca mai sus                                                       |
| Linkuri externe (GitHub, site-ul)                 | Doar când apăsați pe unul                                                                                                                     | Ce trimite browserul oricărui site                                | Nu apăsați                                                       |
| „Search online” (căutare online) pentru un proces | Doar când apăsați                                                                                                                             | **Numele** procesului, trimis către DuckDuckGo în browser         | Nu apăsați                                                       |
| Descărcarea Microsoft WebView2                    | Doar de către programul de instalare, doar dacă WebView2 lipsește                                                                             | Ce primește serviciul de descărcare Microsoft                     | Instalați WebView2 în prealabil                                  |

Actualizările sunt verificate cu o semnătură minisign înainte de instalare,
sunt descărcate în fundal și se instalează când închideți aplicația.

GitHub este operator independent pentru datele pe care le primește. Vedeți
[Declarația generală de confidențialitate GitHub](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement).
Dezvoltatorul nu primește jurnalele GitHub ale acestor cereri.

## Ce ascultă pe calculatorul dvs.

| Punct de ascultare                            | Stare                           | Cine îl poate accesa                                                                                                                                                                                                                                |
| --------------------------------------------- | ------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| API local pe `127.0.0.1:7330`                 | Mereu pornit cât rulează Vitals | Doar programele de pe acest calculator. Este legat de loopback și nu poate fi accesat din rețea. Programele care rulează sub utilizatorul dvs. pot citi valori și controla procese prin el; instrumentul în linie de comandă `vitals` îl folosește. |
| Named pipe `vitals-<USERNAME>`                | Mereu pornit cât rulează Vitals | Contul dvs. de utilizator și administratorii                                                                                                                                                                                                        |
| Acces la distanță (server LAN, port TCP 7331) | **Oprit implicit**              | Dispozitivele din rețeaua locală pe care le-ați asociat                                                                                                                                                                                             |

Când accesul la distanță este pornit:

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
| `settings.json` (în dosarul de configurare al aplicației) | Setările dvs.                                                                                        | Când schimbați o setare                              | Până îl ștergeți                                         |
| Exporturi (CSV, JSON, înregistrări de zbor)               | Ce ați ales să exportați                                                                             | Doar când exportați                                  | Unde le-ați salvat                                       |

Vitals nu trimite nicăieri nimic din aceste fișiere.

## Vitals pentru Android și Wear OS

Aplicația pentru telefon Android și aplicația pentru ceas Wear OS
monitorizează dispozitivul pe care rulează și, dacă asociați unul, un PC cu
Vitals din rețeaua locală.

**Ce citesc aplicațiile pe dispozitiv.** Procesor, placă grafică, memorie,
baterie, temperatură, stocare, rețea și senzori, prin interfețele publice ale
Android. Două citiri au nevoie de acces special, pe care îl acordați
dumneavoastră din setările Android; fiecare rămâne goală până atunci:

| Acces                                                | Ce permite                                                                       | Unde rămâne   |
| ---------------------------------------------------- | -------------------------------------------------------------------------------- | ------------- |
| Acces la utilizare (`PACKAGE_USAGE_STATS`)           | Timpul petrecut în fiecare aplicație și datele folosite de ea, în fila Aplicații | Pe dispozitiv |
| Acces la toate fișierele (`MANAGE_EXTERNAL_STORAGE`) | Harta stocării, cele mai mari dosare și curățarea fișierelor alese de dvs.       | Pe dispozitiv |

Aplicația pentru ceas nu le cere. Ștergerea unui fișier este mereu o acțiune
explicită a dvs., confirmată pe ecran.

**Conexiuni în rețea.**

| Conexiune                                 | Când                                        | Ce primește cealaltă parte                                                                                                                                                                                                                                                                                                   |
| ----------------------------------------- | ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Către un PC asociat                       | Cât timp un ecran sau un widget îl afișează | Tokenul de asociere; HTTP simplu în rețeaua locală, ca la accesul la distanță de mai sus                                                                                                                                                                                                                                     |
| Descoperire mDNS (`_vitals._tcp`)         | Pe ecranul Adaugă PC                        | Dispozitivele din rețeaua locală văd interogarea                                                                                                                                                                                                                                                                             |
| Wake-on-LAN                               | Doar când apăsați Trezește                  | Un pachet broadcast în rețeaua locală, cu adresa MAC a PC-ului                                                                                                                                                                                                                                                               |
| Diagnosticare Google ML Kit               | Când deschideți scanerul QR                 | Google primește modelul și versiunea sistemului, pachetul și versiunea aplicației, un identificator per instalare care nu vă identifică și date de performanță și erori ale scanerului. Imaginea camerei nu părăsește dispozitivul. Vedeți [declarația Google](https://developers.google.com/ml-kit/android-data-disclosure) |
| Telefon către ceas (servicii Google Play) | Când un ceas este asociat cu telefonul      | PC-urile asociate și ultimele lor valori, prin Wearable Data Layer, de la dispozitiv la dispozitiv                                                                                                                                                                                                                           |

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

**Permisiuni.** Camera (doar pentru scanarea codului QR de asociere),
notificări (alertele pe care le configurați), starea rețelei și multicast
Wi-Fi (descoperire), vibrații și un serviciu în prim-plan care rulează doar
cât urmăriți un PC în fundal.

## Modificări pe care Vitals le face în Windows, doar la cererea dvs.

- **Înlocuirea Managerului de activități:** scrie valoarea `Debugger` pentru
  `taskmgr.exe` în `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image
File Execution Options`. Programul de dezinstalare o elimină dacă indică
  spre Vitals.
- **Pornire odată cu Windows:** scrie o valoare în cheia `Run` a
  utilizatorului curent (`HKCU`).
- **Acțiuni cu drepturi ridicate asupra proceselor:** trec prin solicitarea
  UAC a Windows.
- **Dezinstalarea unei aplicații:** rulează programul de dezinstalare al
  acelei aplicații.

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

Dezinstalarea Vitals păstrează datele. Pentru a le șterge, eliminați dosarul
`%LOCALAPPDATA%\Vitals` după dezinstalare. Revocați mai întâi tokenurile de
acces la distanță și ștergeți datele site-ului Vitals din browserul
telefonului.
Pe Android și Wear OS, dezinstalarea aplicației șterge tot ce a stocat.

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
