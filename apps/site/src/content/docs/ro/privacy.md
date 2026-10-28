---
title: Confidențialitate
description: Ce colectează Vitals (nimic), singura cerere automată pe care o face și cum te tratează acest site.
---

Această pagină este un rezumat. Textul complet, cel care face fapt, este
[politica de confidențialitate din depozit](https://github.com/dragoscv/vitals/blob/main/PRIVACY.md);
dacă cele două diferă vreodată, se aplică aceea.

Acoperă Vitals 1.0.x și versiunile ulterioare: aplicațiile pentru Windows și macOS, instrumentul în
linie de comandă `vitals` (inclusiv `vitals serve`), serviciul opțional de senzori și watchdog-ul
pentru blocaje, aplicațiile Android pentru telefon, Wear OS și Google TV, aplicația pentru iPhone și
aplicația pentru televizoare Samsung (Tizen). În vigoare din 28 octombrie 2026.

## Aplicația

- **Zero telemetrie.** Vitals nu are statistici, date de utilizare sau raportare automată a erorilor.
  Nu există niciun cont de creat.
- **O singură cerere automată.** La circa 20 de secunde după pornire, Vitals întreabă GitHub dacă
  există o versiune mai nouă. GitHub vede adresa ta IP și versiunea aplicației, ca la orice
  descărcare. Poți dezactiva asta din **Setări → Despre**. Versiunea din Microsoft Store nu face
  această cerere: o actualizează Store-ul.
- **Accesul la distanță este oprit implicit.** Nimic nu ascultă în rețeaua ta până nu îl pornești din
  **Setări → Acces la distanță**. Atunci ascultă pe adaptorul ales sau, dacă n-ai ales unul, pe toate
  rețelele (`vitals serve` ascultă mereu pe toate rețelele). Doar `/api/v1/health` și schimbul de
  asociere răspund fără token; restul, inclusiv `/metrics` pentru Prometheus, cere unul. Vezi
  [Acces la distanță](/ro/guides/remote-access/).
- **„Search online” (căutare online) se face doar la clic.** Deschide DuckDuckGo în browser cu
  numele procesului, aplicației, dispozitivului sau intrării de pornire pe care ai apăsat.
- **API-ul local este mereu pornit, dar doar local.** Ascultă pe `127.0.0.1`, unde pot ajunge doar
  programele de pe același calculator.
- **Watchdog-ul pentru blocaje este oprit până îl pornești.** Este un mic program separat care te
  avertizează când o fereastră se blochează și numește programul din spatele ei. Îl alegi din cardul
  de bun venit sau din **Setări → Watchdog pentru blocaje**; apoi pornește când te conectezi,
  își păstrează setările în `%APPDATA%\Vitals\watchdog.json` și un jurnal cu numele proceselor în
  `%LOCALAPPDATA%\Vitals\watchdog\watchdog.log`. Oprirea lui șterge intrarea de la conectare; șterge
  acele fișiere pentru a elimina restul.
- **Serviciul de senzori este opțional.** Instalarea lui (o solicitare de administrator) descarcă
  `PawnIO_setup.exe` de pe [github.com/namazso](https://github.com/namazso/PawnIO.Setup), verificat
  după un hash SHA-256 inclus, care instalează driverul de kernel PawnIO; apoi Vitals adaugă un
  serviciu SYSTEM în `C:\Program Files\Vitals Sensors` care oferă temperatura, puterea și
  ventilatoarele doar pe acest calculator. Dezinstalarea Vitals elimină serviciul (o copie instalată
  doar pentru tine cere întâi drepturi de administrator; dacă refuzi, rămâne, cu comanda pentru a-l
  elimina mai târziu); PawnIO rămâne instalat pentru că îl folosesc și alte instrumente —
  dezinstalează **PawnIO** din Setări Windows → Aplicații ca să-l elimini.
- **Datele tale rămân pe calculatorul tău.** Setările, istoricul, jurnalele și detaliile căderilor
  sunt stocate în `%LOCALAPPDATA%\Vitals`, `%APPDATA%\Vitals` și `%APPDATA%\app.vitals.desktop`
  (datele WebView2 sunt în `%LOCALAPPDATA%\app.vitals.desktop`) și nu sunt încărcate nicăieri. Pleacă
  de pe calculator doar dacă le trimiți tu, de exemplu atașând un jurnal la un raport de eroare.
  Dezinstalarea le păstrează; lista completă de șters este la „Cum ștergeți totul” în politică.

## Aplicațiile pentru Android, Wear OS și Google TV

- **Fără reclame, fără cont, fără analiză proprie.** Aplicațiile comunică doar cu PC-urile asociate de tine, în
  rețeaua locală, și cu ceasul tău prin serviciile Google Play. La asociere, PC-ul primește codul și
  numele acestui dispozitiv.
- **Lista aplicațiilor instalate rămâne pe dispozitiv.** Aplicațiile pentru telefon și TV listează
  aplicațiile cu pictogramă în lansator (nume, versiuni, date de instalare) fără acces special.
- **Accesul special îl acorzi tu.** Accesul la utilizare (timp și date pe aplicație) și accesul la
  toate fișierele (harta stocării și curățarea) sunt oprite până le pornești din setările Android;
  ceasul nu le cere, iar accesul la toate fișierele îl poate cere doar APK-ul de pe GitHub. Ce
  dezvăluie rămâne pe dispozitiv. Lista completă a permisiunilor, pe aplicație, este în politică.
- **O singură componentă Google.** Scanerul QR folosește Google ML Kit, care trimite către Google
  date de diagnosticare anonime despre scaner. Imaginea camerei rămâne pe telefon.
- **La dezinstalare se șterge tot.** PC-urile asociate sunt criptate cu o cheie Android Keystore și
  excluse din backup; istoricul dispozitivului păstrează 7 zile.

## Aplicația pentru televizoare Samsung

Păstrează PC-urile asociate și limba aleasă în stocarea proprie de pe televizor, comunică doar cu
acele PC-uri în rețeaua locală și nu are statistici sau reclame. Dezinstalarea ei șterge tot.

## macOS și iPhone

Aplicația pentru macOS se poartă ca cea pentru Windows: totul rămâne pe Mac, iar singura cerere
automată este verificarea actualizărilor. Aplicația pentru iPhone păstrează calculatoarele asociate în
Keychain-ul iPhone-ului (doar pe acel dispozitiv, niciodată în iCloud), comunică doar cu ele în rețeaua
locală și folosește camera doar pentru a citi pe dispozitiv codul QR de asociere. Fără statistici,
reclame sau SDK-uri terțe; eticheta din App Store este „Data Not Collected”.

## Acest site

Site-ul este găzduit pe GitHub Pages. **Nu setează cookie-uri** și **nu folosește statistici de
trafic**. GitHub, ca gazdă, prelucrează datele tehnice pe care le primește orice server web, precum
adresa IP, conform
[declarației de confidențialitate GitHub](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

## Cine răspunde

Operatorul de date este **Dragos Catalin Vladulescu**. Pentru orice întrebare sau cerere legată de
confidențialitate, scrie la [dragoscv12@gmail.com](mailto:dragoscv12@gmail.com).
