---
title: Primii pași
description: Instalează Vitals și orientează-te în primele cinci minute.
---

## Instalare

Descarcă [`Vitals_x64-setup.exe`](https://github.com/dragoscv/vitals/releases/latest/download/Vitals_x64-setup.exe)
și rulează-l. Nu are expert de instalare. Dacă Windows SmartScreen te avertizează, vezi
[Windows SmartScreen](/ro/download/#windows-smartscreen) pe pagina de descărcare.

După aceea, Vitals apare în meniul Start ca **Vitals**.

## Primul ecran

**Panoul principal** arată calculatorul dintr-o privire: încărcarea procesorului, a memoriei, a
discului, a rețelei și a plăcii video, plus un panou de atenționări. Când ceva nu e în regulă, acest
panou numește cauza („un singur proces ține discul ocupat”) în loc să repete cifrele de deasupra.

Alege oricând **De ce merge greu calculatorul?**. Vitals analizează ultimul minut pentru procesor,
disc, presiunea pe memorie, rețea și limitarea termică și îți dă un verdict, procesele responsabile
și un grafic. **Copiază** pune întregul raport în clipboard, pentru un raport de eroare sau o cerere
de asistență.

## Orientare

- **Performanță** — procesorul cu câte o celulă pentru fiecare nucleu logic, memoria, placa video,
  fiecare disc și adaptor de rețea și o filă Temperaturi.
- **Procese** — fiecare program care rulează, grupat pe aplicații. Oprirea, suspendarea sau reluarea
  unui proces îți spune întâi consecința reală; procesele critice pentru sistem sunt protejate.
- **Rețea** — fiecare conexiune deschisă, grupată după programul care o deține.
- **Pornire** și **Servicii** — ce rulează când te autentifici și serviciile Windows.
- **Aplicații instalate**, **Spațiu pe disc**, **Dispozitive și senzori**, **Teste de
  performanță**, **Istoric aplicații** și **Utilizatori**.

Apasă `Ctrl+K` ca să deschizi paleta de comenzi și să sari oriunde după nume. Apasă `?` ca să vezi
toate scurtăturile de tastatură.

## Ce înseamnă o linie de pauză

O valoare afișată ca **—** este una pe care Vitals nu a putut-o măsura pe acest calculator:
hardware-ul nu o raportează sau citirea ei necesită permisiuni pe care Vitals nu le are. Nu ține
niciodată locul lui zero. Ține cursorul deasupra ei sau citește notele secțiunii ca să afli motivul.

## Setări utile

- **Setări → General → Înlocuiește Managerul de activități** face ca `Ctrl+Shift+Esc`, meniul barei
  de activități și `Win+X` să deschidă Vitals. Windows cere permisiunea o singură dată. Meniul din
  zona de notificare păstrează o intrare pentru Managerul de activități încorporat.
- **Setări → General** poate păstra Vitals în zona de notificare când închizi fereastra.
- `Ctrl+Shift+H` afișează sau ascunde **HUD-ul**, un strat mereu deasupra cu procesorul, memoria și
  placa video, prin care clicurile trec la ce se află dedesubt.
- **Setări → Acces la distanță** permite unui telefon din rețeaua ta să urmărească acest PC. Este
  oprit până îl pornești; vezi [Acces la distanță](/ro/guides/remote-access/).
- **Setări → Despre** controlează verificarea automată a actualizărilor.

## Unde stau datele tale

Tot ce stochează Vitals — setări, istoric, jurnale — se află în `%LOCALAPPDATA%\Vitals`, pe acest
calculator. Nimic nu este încărcat nicăieri. Vezi [rezumatul de confidențialitate](/ro/privacy/).
