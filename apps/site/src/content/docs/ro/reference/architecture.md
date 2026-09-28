---
title: Arhitectură
description: Cum este construit Vitals — un singur eșantionator Rust care alimentează fereastra, istoricul, API-ul LAN și linia de comandă.
---

Vitals este o aplicație Tauri 2: un backend Rust și o interfață React 19 afișată în WebView2.
Principiul din spatele fiecărei părți este că **un singur eșantionator produce un singur flux de
cadre**, iar fiecare consumator — fereastra, zona de notificare, HUD-ul, telefonul, Prometheus, linia
de comandă — citește același flux, deci nu pot arăta cifre diferite.

## Schema

```text
                        +---------------------------+
                        |  vitals-win (eșantionator)|
                        |  în spatele trait-urilor  |
                        |       vitals-core         |
                        +-------------+-------------+
                                      |
                      cadre: cadru cheie + diferențe
                                      |
        +---------------+-------------+-------------+----------------+
        |               |                           |                |
        v               v                           v                v
+---------------+ +--------------+       +--------------------+ +--------------+
| webview React | | vitals-store |       |   vitals-server    | |  vitals-ipc  |
| (evenimente   | | SQLite       |       |   HTTP axum        | |  named pipe  |
|  Tauri)       | | istoric +    |       | loopback :7330     | |  \\.\pipe\   |
| fereastră,HUD | | înregistrator|       | LAN :7331 (opțiune)| | vitals-user  |
+---------------+ | de zbor      |       +---------+----------+ +------+-------+
                  +--------------+                 |                   |
                                   REST / SSE / WebSocket / metrics    |
                                                   |                   v
                                   telefon, Prometheus, Home       vitals CLI
                                   Assistant, @vitals/client     (top, ps, ...)
```

## Componentele

**`vitals-core`** definește modelul de date și trait-urile pe care le implementează un backend de
platformă. Fiecare valoare care ar putea lipsi este opțională, așa că „nemăsurat” și „zero” rămân
fapte diferite până pe ecran. Se compilează pe Linux în CI, ca să dovedească faptul că niciun detaliu
de Windows nu a ajuns în el.

**`vitals-win`** este backendul pentru Windows. Un fir de execuție dedicat citește contoarele proprii
ale Windows și produce câte un cadru la fiecare pas. Un test de buget oprește build-ul dacă un
eșantion costă prea mult. Backendurile pentru macOS și Linux vor implementa aceleași trait-uri.

**Cadrele** sunt un **cadru cheie** complet urmat de **diferențe** care conțin doar ce s-a schimbat.
Un consumator nou primește întotdeauna întâi un cadru cheie, deci pornește de la imaginea completă
indiferent când se conectează.

**Webview-ul** primește cadrele ca evenimente Tauri și le afișează cu React. Comenzile în sens invers
— oprirea unui proces, schimbarea unei setări — trec prin comenzi Tauri tipizate.

**`vitals-store`** păstrează istoricul în SQLite, cu retenție, plus un **înregistrator de zbor**: un
minut rulant din toate valorile, capturat la cerere pentru un raport de eroare.

**`vitals-server`** este un server HTTP axum care servește REST, Server-Sent Events, WebSocket și
Prometheus. Ascultă mereu pe `127.0.0.1` pentru scripturile locale și în rețea doar când accesul la
distanță este pornit. Autorizarea este verificată înainte de citirea corpului cererii.

**`vitals-ipc`** este un canal (named pipe) per utilizator prin care linia de comandă se atașează la
aplicația pornită și îi citește cadrele, așa că un calculator rulează un singur eșantionator.

**`packages/protocol`** conține tipurile TypeScript generate din modelul Rust cu ts-rs. Interfața,
pagina pentru telefon și SDK-ul le importă, așa că o schimbare în Rust care strică un client eșuează
la compilare, nu la rulare.

**`@vitals/client`** este SDK-ul TypeScript peste același API și aceleași tipuri generate.

## Mai departe

- [`docs/architecture.md`](https://github.com/dragoscv/vitals/blob/main/docs/architecture.md) din
  depozit, versiunea completă (în engleză).
- [Înregistrările deciziilor de arhitectură](https://github.com/dragoscv/vitals/tree/main/docs/adr),
  pentru motivul fiecărei alegeri.
