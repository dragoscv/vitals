---
title: Linie de comandă
description: Folosește Vitals din terminal — o vizualizare live, liste de procese punctuale, ieșire JSON și un server fără interfață.
---

Comanda `vitals` citește aceleași cifre pe care le arată fereastra.

## Instalare

Linia de comandă **nu** este inclusă în programul de instalare, în versiunea din Microsoft Store sau
în managerii de pachete, dar fiecare [versiune de pe GitHub](https://github.com/dragoscv/vitals/releases/latest)
o livrează ca un zip separat, lângă programul de instalare — `vitals-cli_<versiune>_x64.zip` și
`_arm64.zip`, fiecare cu `vitals.exe`, licența și notele despre componentele terțe. Descarcă acel
zip sau construiește-o din sursă, cu [Rust](https://rustup.rs) instalat:

```powershell
cargo install --git https://github.com/dragoscv/vitals vitals-cli
```

Comanda pune `vitals.exe` în `%USERPROFILE%\.cargo\bin`.

## Vizualizare live

```powershell
vitals top
```

O listă de procese live, ca în aplicație, direct în terminal, redesenată pe loc; apasă `q`, `Esc` sau
`Ctrl-C` ca să ieși. Când ieșirea nu este un terminal — un pipe sau un fișier redirectat — afișează o
singură captură de stare și iese cu codul 0, în loc să deseneze un ecran, așa că `vitals top | head`
și jurnalele CI funcționează. `--json` afișează un obiect JSON pe tick până la `Ctrl-C`.

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

## Senzori, istoric și alerte

```powershell
vitals sensors
vitals history --seconds 600
vitals alerts
```

`sensors` afișează fiecare citire de temperatură, ventilator, tensiune și baterie pe care o poate face
calculatorul, din aceeași sursă pe care o servește ruta `/api/v1/sensors` a aplicației; un senzor care
nu a putut fi citit lipsește din listă, nu apare ca zero. `history` citește eșantioane la nivel de
calculator din depozitul de istoric al aplicației desktop, iar `-s`, `--seconds <N>` stabilește cât de
departe în urmă să privească (implicit 300) — istoricul aparține aplicației desktop, așa că un
eșantionator direct nu are ce să dea și iese cu codul 5. `alerts` listează condițiile pe care le-a
ridicat motorul de alerte, fiecare cu severitatea, condiția, subiectul și valorile alertei.

## Modul de eficiență

```powershell
vitals eco 1234        # pornește modul de eficiență Windows 11 pentru PID-ul 1234
vitals eco 1234 --off  # revine la planificarea normală
```

PID-ul este verificat întâi într-un eșantion proaspăt și refuzat dacă lipsește, așa că un PID
refolosit nu este atins niciodată. La final se afișează starea citită înapoi.

## Închiderea, suspendarea și prioritatea unui proces

```powershell
vitals kill 1234                    # închide PID-ul 1234
vitals suspend 1234                 # îl îngheață
vitals resume 1234                  # îl lasă să ruleze din nou
vitals priority 1234 below-normal   # idle, below-normal, normal, above-normal, high, realtime
```

Se aplică aceeași verificare a PID-ului. Procesele fără de care Windows nu poate funcționa sunt
refuzate, nu închise sau înghețate: aplicația desktop întreabă înainte să le atingă, iar un script
nu poate fi întrebat. Un proces al altui cont are nevoie de un prompt ridicat. Când o comandă este
refuzată, iese cu un cod diferit de zero și motivul pe ieșirea de erori, așa că unui script îi ajunge
codul de ieșire.

## Servire către un agent AI

```powershell
vitals mcp
```

Vorbește Model Context Protocol pe intrarea și ieșirea standard — un mesaj JSON pe linie — ca un agent
AI să poată citi acest calculator. Este doar-citire: nimic accesibil aici nu poate închide, îngheța
sau schimba prioritatea unui proces. Vezi
[`docs/integrations/mcp.md`](https://github.com/dragoscv/vitals/blob/main/docs/integrations/mcp.md) (în engleză).

## Server LAN fără interfață

```powershell
vitals serve
vitals serve --json
```

Pornește același server de [acces la distanță](/ro/guides/remote-access/) fără aplicația desktop, pe
portul **7331**. Generează un token și îl afișează **o singură dată**; copiază-l atunci, pentru că nu
mai este arătat. Se aplică aceleași reguli ca în aplicație: doar rețeaua locală, HTTP simplu, acces
de citire implicit. Apasă `Ctrl+C` ca să îl oprești.

Cu `--json`, serverul afișează pe prima linie de pe ieșirea standard un obiect care se descrie —
portul, nivelul de acces, tokenul, dacă l-a generat, adresa de asociere și codul de asociere — și apoi
continuă să servească, așa că un script care a lăsat serverul să genereze tokenul îl poate afla
totuși:

```json
{
  "port": 7331,
  "scope": "read",
  "token": "…",
  "generated": true,
  "url": "http://192.168.1.20:7331/mobile.html#token=…",
  "pairingCode": "ABCDEF"
}
```

`url` este `null` când nu s-a găsit nicio interfață LAN. `--mqtt <url>` publică în plus către un
broker MQTT cu descoperire Home Assistant, iar `--mqtt-interval <n>` stabilește secundele dintre
publicări (limitat la 1..=3600). Nivelul tokenului este mereu doar-citire: `--control` este refuzat cu
codul de ieșire 4, pentru că un server fără interfață nu are nicio asociere de acordat.

## Completări de shell

```powershell
vitals completions powershell > vitals-completions.ps1
```

Afișează un script de completare pentru `bash`, `elvish`, `fish`, `powershell` sau `zsh`;
redirecționează-l într-un fișier și pune shell-ul să îl încarce.

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

Opțiunile globale merg cu orice comandă. `--json` este acceptat de orice comandă: afișează aceleași
date ca tabelul, iar o valoare care nu a fost măsurată niciodată este `null` sau lipsește, niciodată
`0`.

| Opțiune                       | Ce face                                                                           |
| ----------------------------- | --------------------------------------------------------------------------------- |
| `--json`                      | Ieșire JSON în loc de tabel.                                                      |
| `--source <auto\|app\|local>` | De unde citește. Implicit `auto`.                                                 |
| `--attach <URL>`              | Se atașează la un API local de la această adresă, de ex. `http://127.0.0.1:7330`. |
| `--no-attach`                 | Nu se atașează niciodată; eșantionează direct. La fel ca `--source local`.        |
| `-h`, `--help`                | Ajutor pentru comandă.                                                            |
| `-V`, `--version`             | Afișează versiunea.                                                               |

`--source`, `--attach` și `--no-attach` nu pot fi folosite împreună.

| Comandă                         | Opțiuni                                                                                                                                                                                                                                                            |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `vitals ps`                     | `-f`, `--filter <TEXT>`: doar procesele al căror nume îl conține. `-n`, `--top <N>`: cei mai mari N consumatori (0, implicit, înseamnă toți).                                                                                                                      |
| `vitals top`                    | `-i`, `--interval <MS>`: intervalul de reîmprospătare în milisecunde, implicit 1000.                                                                                                                                                                               |
| `vitals info`                   | Niciuna.                                                                                                                                                                                                                                                           |
| `vitals eco <PID>`              | `--off`: revine la planificarea normală în loc să pornească modul de eficiență.                                                                                                                                                                                    |
| `vitals kill <PID>`             | Niciuna. Închide procesul.                                                                                                                                                                                                                                         |
| `vitals suspend <PID>`          | Niciuna. Îngheață procesul până la `vitals resume`.                                                                                                                                                                                                                |
| `vitals resume <PID>`           | Niciuna.                                                                                                                                                                                                                                                           |
| `vitals priority <PID> <NIVEL>` | `idle`, `below-normal`, `normal`, `above-normal`, `high` sau `realtime`.                                                                                                                                                                                           |
| `vitals report`                 | `-d`, `--duration <SECUNDE>`: cât înregistrează, implicit 60. `-o`, `--output <CALE>`: unde scrie fișierul JSON.                                                                                                                                                   |
| `vitals sensors`                | Niciuna. Fiecare citire de senzor pe care o poate face calculatorul.                                                                                                                                                                                               |
| `vitals history`                | `-s`, `--seconds <N>`: cât de departe în urmă, implicit 300.                                                                                                                                                                                                       |
| `vitals alerts`                 | Niciuna. Ce a ridicat motorul de alerte.                                                                                                                                                                                                                           |
| `vitals mcp`                    | Niciuna. Servește calculatorul prin MCP pe intrarea și ieșirea standard (doar-citire).                                                                                                                                                                             |
| `vitals completions <SHELL>`    | `bash`, `elvish`, `fish`, `powershell` sau `zsh`.                                                                                                                                                                                                                  |
| `vitals serve`                  | `-p`, `--port <PORT>`: implicit 7331. `--token <TOKEN>`: folosește acest token în loc să genereze unul. `--json`: afișează obiectul serverului și continuă să servească. `--mqtt <URL>`, `--mqtt-interval <N>`: publică pe MQTT. `--control` este refuzat (cod 4). |

`serve` nu se atașează niciodată la aplicația desktop, ca un server fără interfață să nu se oprească
odată cu fereastra.

## Coduri de ieșire

| Cod | Semnificație                                                                                                          |
| --- | --------------------------------------------------------------------------------------------------------------------- |
| 0   | Succes.                                                                                                               |
| 1   | O eroare care nu este niciuna dintre cele de mai jos.                                                                 |
| 2   | O eroare de utilizare (argumente greșite), raportată de parserul de argumente.                                        |
| 3   | Aplicația nu a putut fi atinsă (niciun canal de atașare, nimic la `--attach`) sau calculatorul nu are eșantionator.   |
| 4   | O acțiune de control a fost refuzată — acces doar-citire, un proces protejat sau un `serve --control` fără interfață. |
| 5   | Elementul numit nu a fost găsit: un PID absent din eșantion sau date pe care sursa nu le păstrează.                   |

Un script poate alege pe baza acestor coduri fără să citească text englez de pe ieșirea de erori, de
exemplu `if ($LASTEXITCODE -eq 3) { 'rulează Vitals?' }`.

## Culoare

Linia de comandă nu scrie niciun byte ANSI de culoare când ieșirea standard nu este un terminal și
niciunul deloc când variabila de mediu `NO_COLOR` este setată (la orice valoare), urmând
[no-color.org](https://no-color.org), așa că o rulare redirectată nu adună coduri de escape într-un
fișier jurnal.
