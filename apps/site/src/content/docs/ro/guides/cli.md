---
title: Linie de comandă
description: Folosește Vitals din terminal — o vizualizare live, liste de procese punctuale, ieșire JSON și un server fără interfață.
---

Comanda `vitals` vine împreună cu aplicația. Citește aceleași cifre pe care le arată fereastra.

## Vizualizare live

```powershell
vitals top
```

O listă de procese live, ca în aplicație, direct în terminal. Apasă `q` ca să ieși.

## Listă de procese punctuală

```powershell
vitals ps --top 10
```

Cele mai ocupate zece procese, afișate o dată. Adaugă `--json` pentru o ieșire pe care o poate citi
alt program:

```powershell
vitals ps --json
vitals ps --top 5 --json | ConvertFrom-Json
```

## Despre calculator

```powershell
vitals info
vitals report --duration 30
```

`info` descrie hardware-ul și versiunea de Windows. `report` eșantionează timp de numărul dat de
secunde și rezumă ce s-a întâmplat.

## Server LAN fără interfață

```powershell
vitals serve
```

Pornește același server de [acces la distanță](/ro/guides/remote-access/) fără aplicația desktop, pe
portul **7331**. Generează un token și îl afișează **o singură dată**; copiază-l atunci, pentru că nu
mai este arătat. Se aplică aceleași reguli ca în aplicație: doar rețeaua locală, HTTP simplu, acces
de citire implicit. Apasă `Ctrl+C` ca să îl oprești.

## De unde vin cifrele

Când aplicația desktop rulează, linia de comandă se atașează la ea printr-un canal privat (named
pipe) și arată exact ce vede aplicația, așa că un calculator rulează un singur eșantionator. Când
aplicația nu rulează, linia de comandă eșantionează singură calculatorul și spune asta pe ieșirea de
erori.

- `--source app` cere aplicația pornită și eșuează dacă nu este.
- `--source local` eșantionează mereu direct.
- Valoarea implicită, `auto`, încearcă întâi aplicația.
