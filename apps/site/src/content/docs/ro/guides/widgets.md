---
title: Widgeturi pe desktop
description: Plasează un panou mic, mereu deasupra, care arată o singură valoare, oriunde vrei pe ecran.
---

Un widget este o fereastră de sine stătătoare: un panou mic care rămâne deasupra celorlalte ferestre
și arată **o singură** valoare a acestui calculator — încărcarea procesorului, memoria folosită,
activitatea discului, traficul de rețea, temperaturile sau o prezentare generală compactă. Poți
plasa mai multe deodată, fiecare cu conținutul, mărimea, stilul și poziția lui.

Widgeturile se plasează din ecranul **Widgeturi** al aplicației, în bara laterală între **Urmărire**
și **Sănătate**. Fiecare rând de acolo arată o previzualizare live, exact ca ceea ce va afișa
widgetul.

## Plasează un widget

1. Deschide **Widgeturi** în bara laterală.
2. Alege o valoare din butoanele de sus: **Procesor**, **Memorie**, **Disc**, **Rețea**,
   **Temperaturi** sau **Prezentare generală**.
3. Widgetul se deschide imediat ca un panou mic, care rămâne deasupra. Trage-l unde vrei.
4. Ajustează-l în rândul lui din ecranul Widgeturi; previzualizarea se schimbă pe măsură ce modifici
   ceva.

Poți plasa cel mult **12** widgeturi deodată. Fiecare este o fereastră separată, așa că douăsprezece
sunt deja douăsprezece randări așezate peste ce rulează dedesubt — limita este un buget real, nu o
cifră rotundă.

Fiecare rând spune și dacă widgetul este de fapt **pe ecran** și unde se află, pentru că un widget
poate să nu se deschidă (un manager de ferestre care refuză) și lista nu are voie să pretindă altceva.

## Ce poate arăta un widget

| Valoare                 | Afișează                                                                                     |
| ----------------------- | -------------------------------------------------------------------------------------------- |
| **Procesor**            | Încărcarea totală, cu frecvența efectivă și o temperatură.                                   |
| **Memorie**             | Memoria folosită, ca procent și ca valoare din total.                                        |
| **Disc**                | Activitatea discului cel mai solicitat, plus citire și scriere.                              |
| **Rețea**               | Descărcare și încărcare totală, cu numele adaptorului cel mai activ.                         |
| **Temperaturi**         | Senzorul cel mai fierbinte întâi, apoi temperaturile procesorului, plăcii video și discului. |
| **Prezentare generală** | Procesor, memorie, placă video și rețea împreună, dintr-o privire.                           |

O valoare pe care calculatorul nu o poate măsura — o temperatură pe un PC fără senzor, de pildă —
este afișată ca **linie de pauză** (—), niciodată ca zero. Este aceeași regulă pe care o respectă
tot restul aplicației Vitals: o valoare nemăsurată și un zero adevărat sunt fapte diferite.

## Mărime, stil și aranjare

Fiecare widget are una dintre trei mărimi, în pixeli logici:

| Mărime    | Panou     |
| --------- | --------- |
| **Mic**   | 190 × 120 |
| **Mediu** | 280 × 170 |
| **Mare**  | 400 × 260 |

Sunt oferite două stiluri. **Simplu** este aproape opac și se citește peste orice. **Sticlă** este
mai ușor, cu o margine subțire, pentru când vrei să se vadă desktopul prin el. Niciunul nu folosește
o estompare de fundal: este costisitoare pe o fereastră Windows transparentă și se randează diferit
peste un joc pe tot ecranul, exact acolo unde un strat suprapus își merită locul.

Valorile din interiorul panoului sunt aranjate în unul dintre trei moduri:

- **Vertical** — valorile ca rânduri, una sub alta.
- **Împărțit** — valorile așezate unul lângă altul.
- **Inel** — un cadran inelar pentru o singură valoare între 0 și 100.

Doar **Procesor** și **Memorie** sunt un singur procent între 0 și 100, așa că doar ele oferă inelul;
celelalte valori oferă cele două aranjări pe rânduri. Un widget nou de procesor se deschide ca inel,
iar restul ca o coloană de rânduri. Poți alege orice aranjare oferă un widget, oricând.

## Opțiuni pentru fiecare widget

Fiecare widget are propriile setări, în rândul lui din ecranul Widgeturi:

- **Opacitate** — cât de solid este panoul, de la complet opac până la 25 %. Sub această valoare
  cifrele nu mai pot fi citite peste un desktop luminos, așa că acesta este pragul minim.
- **Ține deasupra** — dacă widgetul rămâne peste celelalte ferestre, inclusiv peste aplicațiile pe
  tot ecranul. Pornit implicit.
- **Clicuri prin fereastră** — widgetul nu mai primește input de la mouse, așa că un clic ajunge la
  ce se află în spatele lui. Oprit implicit.

Un widget cu clicurile prin fereastră nu poate primi clic-ul care ar opri din nou această opțiune,
așa că ecranul Widgeturi — nu widgetul — este calea de a o opri. Aceleași opțiuni sunt și pe widgetul
însuși, la trecerea cu cursorul: fixează-l sau eliberează-l, comută clicurile prin fereastră,
deschide-i setările sau închide-l. Închiderea panoului din widget îl elimină și din listă.

## Poziție și mișcare

Widgeturile rețin locul unde le-ai tras. Un widget nou se deschide în cascadă — fiecare cu un pas mai
jos și mai la dreapta decât precedentul — ca mai multe să nu ajungă în același loc și să pară un
singur panou.

Mișcarea respectă setarea Windows **reduce motion** (mișcare redusă): dacă este pornită, widgeturile
își schimbă starea instantaneu, fără animație.

## De unde vin cifrele

Widgeturile citesc din același eșantionor ca tot restul aplicației, la **1 Hz** — o citire pe
secundă. Trasarea mică de sub o valoare păstrează cam patruzeci de secunde de istoric, scurtă în mod
deliberat: un widget este privit în trecere, iar o fereastră mai lungă ar aplatiza un vârf real până
când acesta nu ar mai părea un vârf. Când eșantionorul nu are nicio citire de dat, trasarea se rupe
în loc să deseneze o cădere spre zero care nu s-a întâmplat.

Fiecare widget este o fereastră separată, cu propriul webview, așa că widgeturile costă doar cât
plasezi de fapt — niciunul nu rulează cât timp ecranul este gol.

## Elimină un widget

**Elimină** din rândul widgetului, în ecranul Widgeturi, îi închide fereastra și îl uită. Închiderea
panoului din widgetul însuși face același lucru.
