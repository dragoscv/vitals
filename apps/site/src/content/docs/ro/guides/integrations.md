---
title: Integrări
description: Citește Vitals din scripturi, panouri de monitorizare și automatizări pentru casă — REST, SSE, WebSocket, Prometheus, Home Assistant și un SDK TypeScript.
---

Tot ce folosește telefonul este un API HTTP documentat, pe care îl poate folosi orice altceva.
Contractul complet este
[`docs/api/openapi.yaml`](https://github.com/dragoscv/vitals/blob/main/docs/api/openapi.yaml), iar
versiunea scurtă este
[`docs/api/README.md`](https://github.com/dragoscv/vitals/blob/main/docs/api/README.md) (în engleză).

## Două servere

- **API-ul local** — `http://127.0.0.1:7330`, mereu pornit cât rulează aplicația și accesibil doar de
  pe acest calculator. De la `127.0.0.1` nu este nevoie de token. Dacă portul este ocupat, aplicația
  alege altul și îl scrie în `%LOCALAPPDATA%\Vitals\local-api.json`.
- **Serverul LAN** — portul `7331`, doar când [accesul la distanță](/ro/guides/remote-access/) este
  pornit. Fiecare cerere are nevoie de un token asociat.

```powershell
$d = Get-Content "$env:LOCALAPPDATA\Vitals\local-api.json" | ConvertFrom-Json
curl.exe -s "http://127.0.0.1:$($d.port)/api/v1/snapshot"
```

## REST, SSE și WebSocket

Toate rutele sunt sub `/api/v1/`. O captură de stare este un simplu `GET`; un flux live este
disponibil ca Server-Sent Events sau WebSocket. Fluxurile încep cu un cadru cheie complet și apoi
trimit doar diferențele, așa că un client care se conectează mai târziu pornește totuși de la
imaginea completă.

## Prometheus

`/metrics` servește formatul text Prometheus. O valoare pe care calculatorul nu o poate raporta este
o **serie absentă**, nu un zero, așa că un panou arată o întrerupere, nu o linie plată falsă.
Configurează o sarcină de colectare către serverul LAN, cu tokenul ca bearer.

## Home Assistant

Un pachet gata făcut adaugă în Home Assistant senzori pentru procesor, memorie, disc, placa video și
atenționări. Vezi
[`docs/integrations/home-assistant.md`](https://github.com/dragoscv/vitals/blob/main/docs/integrations/home-assistant.md).

## SDK TypeScript

[`@vitals/client`](https://github.com/dragoscv/vitals/tree/main/packages/client) este un client
tipizat peste REST, SSE și WebSocket, construit pe aceleași tipuri generate pe care le folosește și
aplicația. Se află în depozit, în `packages/client`; publicarea pe npm este planificată.

## Descoperire

Cât timp rulează, serverul LAN se anunță ca `_vitals._tcp` prin mDNS, ca uneltele din rețea să îl
găsească fără o adresă scrisă de mână. Anunțul se oprește odată cu serverul.
