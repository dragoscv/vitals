---
title: Acces la distanță
description: Urmărește un PC de pe telefon prin rețeaua locală și află ce expune acest lucru.
---

Accesul la distanță permite unui telefon, unei tablete sau altui calculator din aceeași rețea să vadă
încărcarea acestui PC, lista de procese și atenționările. Este **oprit implicit**: până îl pornești,
nimic nu ascultă în rețea și nimic nu este anunțat.

## Pornește-l

1. Deschide **Setări → Acces la distanță** și activează-l.
2. Prima dată, Windows te întreabă dacă permiți lui Vitals să treacă de firewall. Permite-i pe
   rețelele **private**. Dacă refuzi, serverul pornește, dar nimic nu ajunge la el.
3. Alege **Asociază un dispozitiv** și selectează un nivel de acces (vezi mai jos).
4. Scanează codul QR cu camera telefonului. Pagina care se deschide poate fi adăugată pe ecranul de
   pornire.

Poți asocia mai multe PC-uri; telefonul le arată unul lângă altul.

## Citire și control

Fiecare asociere are un nivel de acces:

- **Citire** (implicit) vede tot și nu schimbă nimic.
- **Control** poate și opri, suspenda și relua procese. Fiecare acțiune de pe telefon cere o a doua
  atingere pentru confirmare.

Dă acces de control doar dispozitivelor cărora le-ai încredința PC-ul deblocat.

## Ce expune, sincer

- Funcționează **doar în rețeaua locală**. Vitals nu trece traficul prin niciun server și nu
  deschide porturi pe router.
- Folosește **HTTP simplu** pe portul **7331**. Oricine poate urmări traficul din rețeaua ta ar
  putea citi valorile și tokenul. Folosește-l în rețele de încredere, cum ar fi Wi-Fi-ul de acasă,
  nu în rețele publice.
- Tokenul de asociere este afișat o singură dată, în codul QR. Călătorește în partea adresei pe care
  browserul nu o trimite niciodată către server, iar după aceea Vitals afișează doar primele opt
  caractere.
- Cât timp rulează, PC-ul se anunță în rețea ca `_vitals._tcp`, ca telefonul să îl poată găsi.
  Anunțul se oprește când dezactivezi accesul la distanță.

## Revocă un dispozitiv

**Setări → Acces la distanță** listează fiecare dispozitiv asociat și nivelul lui de acces. Revocă-l
și tokenul lui nu mai funcționează. Dezactivarea accesului la distanță oprește serverul cu totul,
așa că niciun dispozitiv nu se poate conecta până nu îl pornești din nou.

## Fără aplicația desktop

[`vitals serve`](/ro/guides/cli/#server-lan-fără-interfață) rulează același server dintr-un terminal,
pe un calculator unde nu vrei fereastra deschisă.
