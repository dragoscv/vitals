---
title: Depanare
description: Jurnale, valori necunoscute, funcții care au nevoie de serviciul auxiliar și avertismente SmartScreen.
---

## Jurnale

Vitals își scrie jurnalele în:

- `%LOCALAPPDATA%\Vitals\logs\vitals.log` — jurnalul de diagnostic, rotit când crește prea mult.
- `%LOCALAPPDATA%\Vitals\logs\crash.txt` — detaliile ultimei căderi, dacă a existat una.

Lipește `%LOCALAPPDATA%\Vitals\logs` în bara de adrese din Explorer ca să deschizi folderul. Atașează
ambele fișiere când [raportezi o eroare](https://github.com/dragoscv/vitals/issues/new/choose). Ele
rămân pe calculatorul tău dacă nu le trimiți tu.

## O valoare apare ca linie de pauză

**—** înseamnă că Vitals nu a putut măsura acea valoare aici. Nu este niciodată un zero deghizat.
Motive frecvente:

- Hardware-ul sau driverul lui nu expune valoarea. Multe laptopuri nu raportează turația
  ventilatoarelor, iar frecvențele memoriei plăcii video necesită un SDK de la producător pe care
  Vitals nu îl folosește încă.
- Windows oferă valoarea doar administratorilor sau proceselor aceluiași utilizator.
- Funcția nu este suportată pe această versiune de Windows.

Fila Temperaturi și secțiunea Dispozitive și senzori arată ce nu pot citi și de ce.

## „Necesită serviciul auxiliar Vitals”

Unele valori, precum activitatea pe disc a fiecărui proces și stivele firelor de execuție, au nevoie
de un serviciu Windows privilegiat. Acest serviciu auxiliar este planificat, dar nu este inclus în
beta, așa că aceste funcții sunt dezactivate cu acest motiv, în loc să dea eroare când le folosești.

## Windows SmartScreen blochează programul de instalare

Build-urile nu sunt semnate cu un certificat comercial, așa că Windows avertizează despre un editor
necunoscut. Alege **Mai multe informații**, apoi **Rulează oricum**. Ca să confirmi întâi că fișierul
este autentic, vezi [Verifică descărcarea](/ro/download/#verifică-descărcarea).

## Telefonul nu se poate conecta

- Verifică dacă accesul la distanță este pornit în **Setări → Acces la distanță**.
- Asigură-te că Windows Firewall permite Vitals pe rețele **private** și că rețeaua este setată ca
  privată în Windows.
- Telefonul și PC-ul trebuie să fie în aceeași rețea locală. Rețelele Wi-Fi pentru oaspeți izolează
  adesea dispozitivele unele de altele.

## Aplicația nu pornește

Uită-te în `crash.txt` din folderul de jurnale. Dacă WebView2 lipsește sau este deteriorat,
reinstalează-l de la [Microsoft](https://developer.microsoft.com/microsoft-edge/webview2/) și rulează
din nou programul de instalare.
