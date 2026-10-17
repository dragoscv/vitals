# Listare Microsoft Store — română (ro-ro)

Se lipește fiecare bloc în Partner Center > Store listings > Romanian (Romania).
Limitele sunt cele din Partner Center, verificate în octombrie 2026 (surse și
numărul de caractere în `docs/store-research-2026-10.md`). Textul descrie doar
ce există în versiunea din Store: actualizarea automată proprie, înlocuirea
Task Manager, serviciul de senzori și CLI-ul nu sunt în pachetul MSIX.

## Nume produs

Vitals: System Monitor

(Store ID 9P95RK5VMGPJ — https://apps.microsoft.com/detail/9P95RK5VMGPJ. Numele „Vitals” singur este deja luat în Store.)

## Descriere scurtă (≤ 1000 caractere; în unele vederi apar doar primele 270)

Vezi ce face de fapt calculatorul tău și de ce merge greu. Procese cu nume și pictograme reale, temperaturi, starea unităților SSD, curățarea spațiului și un watchdog pentru blocaje. Gratuit, open source și privat.

## Descriere (≤ 10 000 caractere)

Îți merge greu PC-ul? Vitals îți spune de ce, pe înțelesul tuturor.

Apeși un buton și Vitals se uită la ce îți folosește acum procesorul, memoria, discul și rețeaua, și numește programul vinovat — „Chrome folosește cea mai mare parte a memoriei”, nu un zid de cifre.

PROCESE, MAI CLAR
• Numele reale ale aplicațiilor și pictograma fiecărui program, grupate și sortate după ce contează
• Totaluri deasupra fiecărei coloane, ca să vezi dintr-o privire cât de ocupat e tot calculatorul
• Consumul de energie și tendința lui, estimate din activitate — Windows nu raportează energia pe program, iar Vitals spune asta
• Cu clic dreapta închizi un program, îi schimbi prioritatea, îi aduci fereastra în față sau îi deschizi folderul
• Când închizi sau suspenzi un program, Vitals îți spune întâi ce urmează să se întâmple, iar ce e necesar pentru Windows cere confirmare înainte

PERFORMANȚĂ ȘI COMPONENTE
• Grafice în timp real pentru procesor, memorie, placa video, fiecare unitate de stocare și fiecare adaptor de rețea, fiecare tip în culoarea lui
• Frecvența procesorului și temperaturile fiecărei unități de stocare, ale plăcilor video NVIDIA și ale procesorului, acolo unde Windows le raportează
• Starea unităților SSD: cât a mai rămas din durata de viață estimată, orele de funcționare și datele scrise, acolo unde unitatea le raportează
• Opțional, încărcarea procesorului pe butonul din bara de activități cât fereastra e deschisă
• Inventarul componentelor, o vedere de tip Manager dispozitive, bateria și alimentarea

STOCARE
• Scanezi o unitate și o explorezi pe o hartă; găsești cele mai mari foldere și fișiere
• Pui deoparte ce nu-ți mai trebuie, verifici lista și trimiți totul în Coșul de reciclare dintr-un pas, de unde se poate recupera
• Eliberezi spațiul Windows cu instrumentele Windows (Curățare disc, DISM) și vezi cât s-a eliberat de fapt
• Curățare pentru programatori: rezultatele compilărilor, worktree-uri git nefolosite, cache-uri de pachete și resturi Docker, șterse doar după ce confirmi
• Când un fișier nu se poate șterge, Vitals îți spune ce program îl ține ocupat

ȘI ÎNCĂ
• Aplicații de pornire și servicii pe care le poți opri, porni sau dezactiva, cu intrările Microsoft ascunse până le ceri
• Un watchdog care observă când Windows nu mai răspunde, numește programul vinovat și îți propune să-l închizi, să-i scazi prioritatea sau să repornești un Explorer blocat. Nu închide nimic de unul singur
• Alerte, un panou configurabil, teme luminoasă și întunecată și zece culori de accent
• O vedere opțională a PC-ului de pe telefon sau televizor, în rețeaua de acasă, plus puncte de acces pentru Prometheus și Home Assistant — toate oprite implicit

Cifre oneste: când ceva nu se poate măsura, Vitals arată o liniuță, niciodată un zero inventat.

Privat din principiu: fără conturi, fără reclame, fără telemetrie, fără analiză. Nimic nu pleacă de pe calculatorul tău decât dacă pornești singur accesul la distanță.

Gratuit și open source (MIT). Disponibil în română și engleză.

## Ce e nou în această versiune (≤ 1500 caractere)

Ce e nou în 1.0.2

• Securitate: biblioteca TLS inclusă în Vitals (rustls) este actualizată la 0.23.46, care repară patru vulnerabilități publicate. Versiunea din Store nu face singură cereri pe internet, deci actualizarea ține la zi o dependență, nu închide o breșă la care erai expus
• Reparații în procesul de lansare; aplicația Windows funcționează la fel ca în 1.0.0

Nou în 1.0, prima versiune stabilă

• Pictogramele programelor în lista de procese, inclusiv pentru servicii și procesele altor conturi
• Totaluri deasupra coloanelor pentru procesor, memorie, disc, rețea și placa video
• Coloane pentru consumul de energie și tendința lui, plus Tip, Set de lucru, Citire disc, Scriere disc și PID părinte
• Adu în față: cu clic dreapta pe o aplicație îi restaurezi și îi ridici fereastra
• Temperaturile procesorului, ale plăcilor video NVIDIA și ale fiecărei unități de stocare, plus starea unităților SSD
• Frecvența procesorului și, opțional, încărcarea lui pe butonul din bara de activități

Reparat: graficele folosesc culoarea ta de accent; cardurile de stocare și dispozitive nu își mai taie conținutul; coloanele se redimensionează în loc să se sorteze când le tragi marginea.

## Funcții (până la 20, ≤ 200 caractere fiecare)

- Un răspuns simplu, din citiri în timp real, la „de ce îmi merge greu PC-ul?”
- Procese cu numele reale ale aplicațiilor, pictogramele programelor și acțiuni la clic dreapta
- Totaluri pentru tot calculatorul deasupra fiecărei coloane de procese
- Consumul de energie și tendința lui pentru fiecare program, estimate din activitate
- Aduci în față fereastra oricărei aplicații direct din lista de procese
- Grafice în timp real pentru procesor, memorie, placa video, unități de stocare și rețea
- Temperaturile procesorului, ale plăcilor video NVIDIA și ale fiecărei unități de stocare
- Starea unităților SSD: durata de viață rămasă, orele de funcționare și datele scrise
- Frecvența procesorului și, opțional, încărcarea lui pe butonul din bara de activități
- Harta spațiului de stocare, cu cele mai mari foldere și fișiere
- Curățare pe care o verifici întâi, cu ștergere în Coșul de reciclare, de unde se poate recupera
- Curățare pentru programatori: compilări, cache-uri, worktree-uri și resturi Docker
- Aplicații de pornire și servicii pe care le poți opri sau porni
- Watchdog care numește programul blocat și poate reporni Explorer
- Inventarul componentelor și o vedere de tip Manager dispozitive
- Alerte și un panou configurabil, cu teme luminoasă și întunecată
- Vedere opțională a PC-ului de pe telefon sau televizor, în rețeaua de acasă
- Puncte de acces pentru Prometheus și Home Assistant, oprite implicit
- Fără conturi, fără reclame, fără telemetrie
- Gratuit și open source (MIT), în română și engleză

## Cuvinte cheie (până la 7, ≤ 40 caractere fiecare, ≤ 21 de cuvinte în total)

task manager, monitor sistem, pc lent, temperatura procesor, curățare disc, sănătate ssd, aplicații pornire

## Politica de confidențialitate

https://vitals.dragoscatalin.ro/ro/privacy/

## Contact pentru asistență

dragoscv12@gmail.com
