---
title: Întrebări frecvente
description: Este Vitals sigur? De ce avertizează SmartScreen? Trimite date undeva? Are nevoie de drepturi de administrator? Răspunsuri la întrebările de dinaintea instalării.
---

## Este sigur să instalez Vitals?

Vitals este open source, sub licența MIT, deci oricine poate citi exact ce face. Fiecare versiune
este construită de un flux public GitHub Actions, iar fiecare program de instalare are o atestare de
construire semnată, care leagă fișierul de commit-ul și fluxul care l-au produs.
[Pagina de descărcare](/ro/download/) arată comanda care o verifică.

[Versiunea din Microsoft Store](https://apps.microsoft.com/detail/9P95RK5VMGPJ) este în plus
verificată și semnată de Microsoft înainte de publicare.

## De ce avertizează Windows SmartScreen la programul de instalare?

Programul de instalare de pe GitHub nu este semnat cu un certificat comercial, așa că Windows nu
recunoaște editorul și afișează **„Windows a protejat computerul”**. Alege **Mai multe informații**,
apoi **Rulează oricum**. Dacă nu vrei niciun avertisment, instalează din
[Microsoft Store](https://apps.microsoft.com/detail/9P95RK5VMGPJ): Microsoft semnează pachetul, deci
SmartScreen nu avertizează.

## Trimite Vitals date undeva?

Fără telemetrie, fără analiză, fără rapoarte de erori și fără cont Vitals. Singura conexiune pe care
o face Vitals de la sine este verificarea actualizărilor pe GitHub în versiunea cu program de
instalare, pe care o poți opri din **Setări → Despre**. Versiunea din Store nici măcar nu o face,
pentru că o actualizează Store-ul.

Alte două conexiuni sunt oprite până le pornești: conectarea la Kodwen din pagina Agenți AI și
pagina Sănătate, care citește de pe un server vmui pe care îl rulezi tu.

Accesul de la distanță (vederea calculatorului de pe telefon sau televizor) rămâne în rețeaua ta și
este oprit până îl pornești. [Politica de confidențialitate](/ro/privacy/) enumeră fiecare conexiune
și fiecare fișier pe care îl scrie Vitals.

## Are nevoie de drepturi de administrator?

Nu. Vitals rulează ca utilizator obișnuit. Câteva acțiuni cer drepturi de administrator — închiderea
unui proces al altui cont, rularea Disk Cleanup sau DISM din Windows, modificarea unui serviciu — iar
pentru ele Windows afișează fereastra obișnuită, o singură dată, doar pentru acea acțiune. Tot ce nu
îți permit drepturile este dezactivat și își spune motivul, nu un buton care dă eroare.

Unele valori sunt disponibile doar administratorilor, de exemplu memoria folosită de procesele altor
utilizatori. Acestea apar ca o liniuță când Vitals nu le poate citi.

## Poate o alertă să ajungă în altă parte decât în notificările Windows?

Da, dacă vrei asta. **Setări → Notificări** primește o adresă de webhook, un subiect
[ntfy](https://ntfy.sh), un webhook de Discord sau o instanță Home Assistant, iar fiecare alertă
poate fi trimisă către destinațiile pe care le alegi — o regulă către Discord, o alertă de disc
defect către toate. Există un buton **Testează** pentru fiecare destinație, ca să afli dacă
funcționează înainte să te bazezi pe ea.

Nu se trimite nimic până nu adaugi o adresă: fără nicio destinație configurată, funcția nu face
absolut nimic. Un webhook primește în plus o semnătură HMAC într-un header, ca destinația ta să poată
dovedi că cererea a venit de pe calculatorul tău. Pleacă titlul alertei și explicația ei, gravitatea,
regula sau discul la care se referă și numele acestui calculator — către exact adresa pe care ai
scris-o, și nicăieri altundeva. [Politica de confidențialitate](/privacy/) listează fiecare câmp.

Tot aici îți poți scrie propriile condiții: o măsurătoare (procesor, temperatură, memorie, activitate
a discului, baterie, pachete pierdute), peste sau sub un număr ales de tine, cât timp decizi tu, cu o
perioadă de liniște ca o condiție care oscilează să nu-ți umple telefonul.

## De ce arată „—” în loc de 0?

Pentru că înseamnă lucruri diferite. Zero este o măsurătoare: programul chiar nu a folosit nimic. O
liniuță înseamnă că Windows sau hardware-ul nu raportează valoarea ori că drepturile tale nu permit
citirea ei. Un zero acolo ar transforma „placa video nu își raportează memoria” în „placa video nu
folosește memorie”, ceea ce e fals. Liniuța e onestă.

## Care e diferența dintre versiunea din Store și programul de instalare?

Este același program. Versiunea din Store:

- este actualizată de Microsoft Store, nu de Vitals;
- nu poate înlocui Task Manager pentru Ctrl+Shift+Esc, pentru că Windows nu permite asta aplicațiilor
  împachetate;
- nu include serviciul opțional de senzori, așa că temperatura procesorului vine doar din ce
  raportează Windows;
- nu afișează avertisment SmartScreen la instalare.

Restul — procese, performanță, stocare, aplicații la pornire, diagnostic, acces de la distanță — este
identic. Poți trece de la una la alta oricând; dezinstalează-o pe una înainte să o instalezi pe
cealaltă.

## Încetinește calculatorul?

E construit să nu o facă. Eșantionarea pe Windows are un buget de 30 de milisecunde pe eșantion,
verificat la fiecare modificare, iar costul de repaus al întregii aplicații este verificat separat
față de `idle-budget.json`: o mediană de 10,7 % dintr-un nucleu cât stă ascunsă în zona de
notificare, inclusiv procesul WebView2. Eșantionarea este o dată pe secundă cât fereastra e
vizibilă și o dată la 20 de secunde când e ascunsă.

## Merge pe Windows 10, ARM sau macOS?

Windows 10 (versiunea 1809 sau mai nouă) și Windows 11, pe x64. O versiune ARM64 este în
previzualizare. macOS 13 sau mai nou este în previzualizare, ca imagine de disc notarizată pentru
Apple silicon și Intel. Nu există planuri datate pentru Linux. Aplicațiile pentru telefon Android,
Wear OS și televizor, precum și aplicația pentru iPhone, pot urmări un PC sau un Mac din rețeaua ta.

## Cum îl dezinstalez?

Deschide **Setări → Aplicații → Aplicații instalate** în Windows, găsește **Vitals** și alege
**Dezinstalare**. Merge pentru ambele versiuni. Versiunea cu program de instalare păstrează setările
și istoricul în `%LOCALAPPDATA%\Vitals`, ca o reinstalare să continue de unde ai rămas; șterge
folderul ca să le elimini și pe ele. Dacă ai lăsat versiunea cu program de instalare să înlocuiască
Task Manager sau ai instalat serviciul de senzori, programul de dezinstalare le elimină pe amândouă.
Serviciul de senzori rulează ca SYSTEM, deci eliminarea lui cere drepturi de administrator: pentru
o copie instalată doar pentru tine, programul de dezinstalare le cere printr-o singură solicitare
Windows. Dacă o refuzi, serviciul rămâne, iar programul de dezinstalare îți arată cum îl elimini mai
târziu (vezi [Depanare](/ro/guides/troubleshooting/#serviciul-de-senzori-a-rămas-după-dezinstalare)).
Îl poți elimina și din pagina **Dispozitive și senzori** înainte de dezinstalare.

## Unde raportez o problemă?

Deschide un issue pe [GitHub](https://github.com/dragoscv/vitals/issues).
[Ghidul de depanare](/ro/guides/troubleshooting/) arată unde se află fișierul jurnal, care ajută.
