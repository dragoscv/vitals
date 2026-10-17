---
title: Linie de comandă
description: Folosește Vitals din terminal — o vizualizare live, liste de procese punctuale, ieșire JSON și un server fără interfață.
---

Comanda `vitals` citește aceleași cifre pe care le arată fereastra.

## Instalare

Linia de comandă **nu** este inclusă încă în programul de instalare, în versiunea din Microsoft Store
sau în managerii de pachete. Până când o versiune o va include, o construiești din sursă, cu
[Rust](https://rustup.rs) instalat:

```powershell
cargo install --git https://github.com/dragoscv/vitals vitals-cli
```

Comanda pune `vitals.exe` în `%USERPROFILE%\.cargo\bin`.

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
secunde și scrie ce s-a întâmplat într-un fișier JSON (`vitals-report-<moment>.json` în folderul
curent, dacă nu dai `--output`).

## Modul de eficiență

```powershell
vitals eco 1234        # pornește modul de eficiență Windows 11 pentru PID-ul 1234
vitals eco 1234 --off  # revine la planificarea normală
```

PID-ul este verificat întâi într-un eșantion proaspăt și refuzat dacă lipsește, așa că un PID
refolosit nu este atins niciodată. La final se afișează starea citită înapoi.

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
pipe) sau, dacă acesta nu răspunde, prin API-ul local al aplicației, și arată exact ce vede
aplicația, așa că un calculator rulează un singur eșantionator. Când aplicația nu rulează, linia de
comandă eșantionează singură calculatorul. Ce s-a întâmplat apare pe ieșirea de erori, ca ieșirea
standard să rămână curată pentru `--json`.

- `--source app` cere aplicația pornită și eșuează dacă nu este.
- `--source local` eșantionează mereu direct.
- Valoarea implicită, `auto`, încearcă întâi aplicația.

## Referință

Opțiunile globale merg cu orice comandă.

| Opțiune                       | Ce face                                                                           |
| ----------------------------- | --------------------------------------------------------------------------------- |
| `--json`                      | Ieșire JSON în loc de tabel.                                                      |
| `--source <auto\|app\|local>` | De unde citește. Implicit `auto`.                                                 |
| `--attach <URL>`              | Se atașează la un API local de la această adresă, de ex. `http://127.0.0.1:7330`. |
| `--no-attach`                 | Nu se atașează niciodată; eșantionează direct. La fel ca `--source local`.        |
| `-h`, `--help`                | Ajutor pentru comandă.                                                            |
| `-V`, `--version`             | Afișează versiunea.                                                               |

`--source`, `--attach` și `--no-attach` nu pot fi folosite împreună.

| Comandă            | Opțiuni                                                                                                                                       |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `vitals ps`        | `-f`, `--filter <TEXT>`: doar procesele al căror nume îl conține. `-n`, `--top <N>`: cei mai mari N consumatori (0, implicit, înseamnă toți). |
| `vitals top`       | `-i`, `--interval <MS>`: intervalul de reîmprospătare în milisecunde, implicit 1000.                                                          |
| `vitals info`      | Niciuna.                                                                                                                                      |
| `vitals eco <PID>` | `--off`: revine la planificarea normală în loc să pornească modul de eficiență.                                                               |
| `vitals report`    | `-d`, `--duration <SECUNDE>`: cât înregistrează, implicit 60. `-o`, `--output <CALE>`: unde scrie fișierul JSON.                              |
| `vitals serve`     | `-p`, `--port <PORT>`: implicit 7331. `--token <TOKEN>`: folosește acest token în loc să genereze unul. `--control`: îi dă drept de control.  |

`serve` nu se atașează niciodată la aplicația desktop, ca un server fără interfață să nu se oprească
odată cu fereastra.
