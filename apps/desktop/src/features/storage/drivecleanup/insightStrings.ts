/**
 * Translations for the drive history and the weekly clean-up.
 *
 * Merged into the storage namespace under `insight`, beside `drive`. The
 * notification text lives in schedule.ts: the shell pushes it at start.
 */

const en = {
  growth: {
    title: 'Why is this drive growing?',
    how: 'Vitals reads how full each drive is once an hour.',
    day: 'Last 24 hours',
    week: 'Last 7 days',
    month: 'Last 30 days',
    perDay: '+{{size}} a day',
    perDayLess: '−{{size}} a day',
    steady: 'No change',
    notEnough: 'Not enough readings yet',
    chartLabel: 'Space used on {{mount}} over the last 30 days',
    changesTitle: 'Changed since the analysis of {{date}}',
    grew: '+{{size}}',
    shrank: '−{{size}}',
    firstAnalysis:
      'This is the first analysis of this drive, so there is nothing to compare it with yet.',
    noChanges: 'Nothing measured has changed since {{date}}.',
    unavailable: 'The history of this drive could not be read. {{message}}',
    item: {
      dockerDisk: 'Docker disk file',
      wslDisk: 'Linux disk file',
    },
  },
  weekly: {
    title: 'Weekly clean-up',
    body: 'Runs the safe part of your choices once a week, without asking. Never anything that needs administrator approval, stops WSL or Docker, or cannot be undone.',
    toggle: 'Clean up {{mount}} every week',
    day: 'Day',
    time: 'Time',
    when: 'Waits until the PC is idle and plugged in, or runs within a day if it never is. If Vitals was closed at that time, it runs at the next start.',
    includes: 'It will clean: {{list}}',
    includesNone:
      'None of your saved choices for this drive is safe to run unattended, so the weekly clean-up would do nothing.',
    fromDefaults: 'No choices saved for this drive yet, so it uses the safe defaults.',
    recent: 'Recent clean-ups',
    none: 'No clean-up of this drive has been recorded yet.',
    byUser: 'By you',
    scheduled: 'Weekly',
    freed: 'freed {{size}}',
    freedUnknown: 'gain not measured',
    nothing: 'nothing to do',
    failures: 'failures: {{n}}',
  },
};

const ro: typeof en = {
  growth: {
    title: 'De ce se umple această unitate?',
    how: 'Vitals citește o dată pe oră cât de plină este fiecare unitate.',
    day: 'Ultimele 24 de ore',
    week: 'Ultimele 7 zile',
    month: 'Ultimele 30 de zile',
    perDay: '+{{size}} pe zi',
    perDayLess: '−{{size}} pe zi',
    steady: 'Nicio schimbare',
    notEnough: 'Încă nu sunt destule citiri',
    chartLabel: 'Spațiul ocupat pe {{mount}} în ultimele 30 de zile',
    changesTitle: 'Ce s-a schimbat de la analiza din {{date}}',
    grew: '+{{size}}',
    shrank: '−{{size}}',
    firstAnalysis:
      'Aceasta este prima analiză a unității, așa că încă nu există cu ce să fie comparată.',
    noChanges: 'Nimic din ce s-a măsurat nu s-a schimbat din {{date}}.',
    unavailable: 'Istoricul acestei unități nu a putut fi citit. {{message}}',
    item: {
      dockerDisk: 'Fișierul discului Docker',
      wslDisk: 'Fișierul discului Linux',
    },
  },
  weekly: {
    title: 'Curățenie săptămânală',
    body: 'Rulează o dată pe săptămână partea sigură a alegerilor tale, fără să întrebe. Niciodată ceva care cere aprobare de administrator, oprește WSL sau Docker ori nu poate fi anulat.',
    toggle: 'Curăță {{mount}} în fiecare săptămână',
    day: 'Ziua',
    time: 'Ora',
    when: 'Așteaptă ca PC-ul să fie neatins și în priză, sau rulează în cel mult o zi dacă nu se întâmplă. Dacă Vitals era închis la ora aceea, rulează la următoarea pornire.',
    includes: 'Va curăța: {{list}}',
    includesNone:
      'Niciuna dintre alegerile salvate pentru această unitate nu poate rula sigur nesupravegheată, așa că curățenia săptămânală nu ar face nimic.',
    fromDefaults:
      'Nu ai salvat încă alegeri pentru această unitate, așa că folosește variantele sigure implicite.',
    recent: 'Curățenii recente',
    none: 'Încă nu s-a înregistrat nicio curățenie a acestei unități.',
    byUser: 'De tine',
    scheduled: 'Săptămânală',
    freed: 'a eliberat {{size}}',
    freedUnknown: 'câștig nemăsurat',
    nothing: 'nimic de făcut',
    failures: 'eșecuri: {{n}}',
  },
};

/** Merged into the storage bundles under `insight`. */
export const insightBundles = { en, ro } as const;
