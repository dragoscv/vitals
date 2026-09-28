# Privacy Policy

**Effective date:** 28 September 2026 · **Applies to:** Vitals 0.9.0-beta.1 and
later, for Windows · [Politica de confidențialitate în limba română](#politica-de-confidențialitate)

**For app store reviewers:** Vitals collects no personal data, has no
telemetry, analytics, accounts or crash reporting, and the developer receives
no data. The only automatic network request is an update check to GitHub,
which the user can turn off.

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
  the phone the token is kept in the browser's local storage.
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
| `settings.json` (in the app's config folder) | Your settings                                                                                    | When you change a setting                    | Until you delete it                                      |
| Exports (CSV, JSON, flight recordings)       | What you chose to export                                                                         | Only when you export                         | Wherever you saved them                                  |

Nothing in these files is sent anywhere by Vitals.

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

**Data intrării în vigoare:** 28 septembrie 2026 · **Se aplică:** Vitals
0.9.0-beta.1 și versiunile ulterioare, pentru Windows

**Pentru evaluatorii magazinelor de aplicații:** Vitals nu colectează date
personale, nu are telemetrie, analiză, conturi sau raportare a erorilor, iar
dezvoltatorul nu primește nicio dată. Singura cerere automată în rețea este
verificarea actualizărilor pe GitHub, pe care utilizatorul o poate dezactiva.

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
  browserului.
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
