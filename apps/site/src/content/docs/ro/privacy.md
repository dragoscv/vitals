---
title: Confidențialitate
description: Ce colectează Vitals (nimic), singura cerere automată pe care o face și cum te tratează acest site.
---

Această pagină este un rezumat. Textul complet, cel care face fapt, este
[politica de confidențialitate din depozit](https://github.com/dragoscv/vitals/blob/main/PRIVACY.md);
dacă cele două diferă vreodată, se aplică aceea.

## Aplicația

- **Zero telemetrie.** Vitals nu are statistici, date de utilizare sau raportare automată a erorilor.
  Nu există niciun cont de creat.
- **O singură cerere automată.** La circa 20 de secunde după pornire, Vitals întreabă GitHub dacă
  există o versiune mai nouă. GitHub vede adresa ta IP și versiunea aplicației, ca la orice
  descărcare. Poți dezactiva asta din **Setări → Despre**.
- **Accesul la distanță este oprit implicit.** Nimic nu ascultă în rețeaua ta până nu îl pornești din
  **Setări → Acces la distanță**. Vezi [Acces la distanță](/ro/guides/remote-access/).
- **API-ul local este mereu pornit, dar doar local.** Ascultă pe `127.0.0.1`, unde pot ajunge doar
  programele de pe același calculator.
- **Datele tale rămân pe calculatorul tău.** Setările, istoricul, jurnalele și detaliile căderilor
  sunt stocate în `%LOCALAPPDATA%\Vitals` și nu sunt încărcate nicăieri. Pleacă de pe calculator doar
  dacă le trimiți tu, de exemplu atașând un jurnal la un raport de eroare.

## Acest site

Site-ul este găzduit pe GitHub Pages. **Nu setează cookie-uri** și **nu folosește statistici de
trafic**. GitHub, ca gazdă, prelucrează datele tehnice pe care le primește orice server web, precum
adresa IP, conform
[declarației de confidențialitate GitHub](https://docs.github.com/site-policy/privacy-policies/github-general-privacy-statement).

## Cine răspunde

Operatorul de date este **Dragos Catalin Vladulescu**. Pentru orice întrebare sau cerere legată de
confidențialitate, scrie la [dragoscv12@gmail.com](mailto:dragoscv12@gmail.com).
