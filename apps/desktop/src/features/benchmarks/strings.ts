/**
 * Benchmark translations.
 *
 * Own namespace, same reasoning as the other feature bundles. English and
 * Romanian side by side so a key cannot be added to one and forgotten in the
 * other.
 *
 * Romanian counts in three forms — 1, 2..19 (`few`), 20+ (`other`, which also
 * takes the "de" particle: "20 de secunde"). Getting that wrong is the single
 * most visible tell that a translation was machine-produced, so every counted
 * string here carries all three.
 */

import { i18n } from '@vitals/i18n';

export const BENCHMARKS_NS = 'benchmarks';

const en = {
  title: 'Benchmarks',
  subtitle:
    'Measures what this machine can actually do, and records the conditions it was measured under.',
  refresh: 'Refresh the list',

  name: {
    cpuSingleThread: 'CPU — single thread',
    cpuMultiThread: 'CPU — all threads',
    memoryBandwidth: 'Memory bandwidth',
    memoryLatency: 'Memory latency',
    diskSequentialRead: 'Disk — sequential read',
    diskSequentialWrite: 'Disk — sequential write',
    diskRandomRead: 'Disk — random read',
    diskRandomWrite: 'Disk — random write',
    gpuCompute: 'GPU — compute',
    gpuRender: 'GPU — render',
  },

  describe: {
    cpuSingleThread: 'How fast one core is. This is what most applications feel like.',
    cpuMultiThread: 'How much work every core can do at once, for compiling and rendering.',
    memoryBandwidth: 'How much data can move between the processor and RAM each second.',
    memoryLatency: 'How long the processor waits for a value that is not in cache.',
    diskSequentialRead: 'Reading one large file end to end.',
    diskSequentialWrite: 'Writing one large file end to end.',
    diskRandomRead: 'Reading many small pieces scattered across the drive.',
    diskRandomWrite: 'Writing many small pieces scattered across the drive.',
    gpuCompute: 'Raw arithmetic throughput on the graphics processor.',
    gpuRender: 'How many frames the graphics processor can draw.',
  },

  group: {
    cpu: 'Processor',
    memory: 'Memory',
    disk: 'Storage',
    gpu: 'Graphics',
  },

  select: {
    legend: 'Choose what to measure',
    all: 'Select all available',
    none: 'Clear the selection',
  },

  menu: {
    select: 'Include in the run',
    deselect: 'Leave out of the run',
    runOnly: 'Run only this',
    copyResult: 'Copy result',
  },

  unavailable: {
    badge: 'Not available',
    // Keyed by the reason string the backend sends. `unknown` catches anything
    // a newer backend introduces, so an unrecognised reason still reads as a
    // sentence rather than as a raw identifier.
    needsConsent: 'This writes test data to your drive, so it needs your permission first.',
    needsGraphicsContext:
      'This needs a graphics context that Vitals does not create while running as a window.',
    notImplemented: 'Not built yet. It is listed so you can see what is planned.',
    unknown: 'This benchmark cannot run on this system.',
  },

  warning: {
    title: 'This will make your computer unresponsive while it runs.',
    body: 'A benchmark deliberately uses everything the machine has. Close what you are working on first, leave it plugged in, and do not use the computer until it finishes.',
    estimate_one: 'The selected benchmark takes about {{count}} second.',
    estimate_other: 'The selected benchmarks take about {{count}} seconds in total.',
    nothingSelected: 'Select at least one benchmark to begin.',
  },

  action: {
    start: 'Run benchmarks',
    rerun: 'Run again',
    busy: 'Running…',
  },

  running: {
    region: 'Benchmark progress',
    title: 'Measuring {{name}}.',
    body: 'Any numbers still on screen are from the previous run and are not final. Leave the machine alone until this finishes.',
    remaining_one: '{{count}} benchmark remaining.',
    remaining_other: '{{count}} benchmarks remaining.',
  },

  results: {
    title: 'Results',
    total: 'The whole suite took {{seconds}} seconds.',
    headline_one: 'A single run',
    headline_other: 'Median of {{count}} runs',
    runs: 'Individual runs',
    spread: 'Best {{best}}, worst {{worst}}',
    variability: 'Spread between runs: {{percent}}',
    variabilitySingle: 'Only one run, so there is no spread to report.',
    medianWhy:
      'The median is shown rather than the average because interference can only make a run slower, never faster — one stalled pass would drag an average down for good.',
    duration: 'Took {{seconds}} seconds',
  },

  trust: {
    ok: 'Conditions were clean',
    okBody: 'Nothing was detected that would distort this figure.',
    bad: 'Do not trust this figure',
    badIntro: 'This was measured under conditions that change the result:',
    reason: {
      throttled:
        'The processor was thermally throttled, so it was not running at the speed it is capable of.',
      onBattery:
        'The machine was on battery. Windows caps performance to save power, often by half.',
      backgroundLoad:
        'Something else was using {{percent}} of the machine, so this measured the leftovers.',
      variability:
        'The runs disagreed with each other by {{percent}}, which is more than the measurement can absorb.',
      tainted: 'The backend flagged the conditions as unsuitable.',
    },
    conditions: 'Conditions',
    powerPlan: 'Power plan: {{plan}}',
    powerPlanUnknown: 'Power plan: not reported',
    background: 'Background load: {{percent}}',
    temperature: 'Temperature: {{start}} at the start, {{end}} at the end',
    temperatureUnknown: 'Temperature: not reported',
  },

  error: {
    failed: 'The benchmark run failed. {{message}}',
    stale: 'Showing the last completed run. {{message}}',
    list: 'Could not read the list of benchmarks. {{message}}',
  },

  idle: {
    title: 'Nothing measured yet',
    body: 'Choose what to measure and start a run. Numbers appear here with the conditions they were taken under.',
  },
  noHost: {
    title: 'No readings are arriving',
    body: 'Vitals cannot reach the part of itself that runs benchmarks. Nothing is wrong with the machine — restarting Vitals usually fixes this.',
  },
} as const;

const ro = {
  title: 'Teste de performanță',
  subtitle:
    'Măsoară ce poate face efectiv acest calculator și înregistrează condițiile în care a fost măsurat.',
  refresh: 'Reîmprospătează lista',

  name: {
    cpuSingleThread: 'Procesor — un singur fir',
    cpuMultiThread: 'Procesor — toate firele',
    memoryBandwidth: 'Debit de memorie',
    memoryLatency: 'Latență de memorie',
    diskSequentialRead: 'Disc — citire secvențială',
    diskSequentialWrite: 'Disc — scriere secvențială',
    diskRandomRead: 'Disc — citire aleatorie',
    diskRandomWrite: 'Disc — scriere aleatorie',
    gpuCompute: 'Placă video — calcul',
    gpuRender: 'Placă video — randare',
  },

  describe: {
    cpuSingleThread: 'Cât de rapid este un singur nucleu. Așa se simt majoritatea aplicațiilor.',
    cpuMultiThread: 'Cât de mult pot lucra toate nucleele deodată, pentru compilare și randare.',
    memoryBandwidth: 'Câte date se pot muta între procesor și memorie în fiecare secundă.',
    memoryLatency: 'Cât așteaptă procesorul o valoare care nu se află în cache.',
    diskSequentialRead: 'Citirea unui fișier mare de la un capăt la altul.',
    diskSequentialWrite: 'Scrierea unui fișier mare de la un capăt la altul.',
    diskRandomRead: 'Citirea multor bucăți mici împrăștiate pe disc.',
    diskRandomWrite: 'Scrierea multor bucăți mici împrăștiate pe disc.',
    gpuCompute: 'Debitul aritmetic brut al procesorului grafic.',
    gpuRender: 'Câte cadre poate desena procesorul grafic.',
  },

  group: {
    cpu: 'Procesor',
    memory: 'Memorie',
    disk: 'Stocare',
    gpu: 'Grafică',
  },

  select: {
    legend: 'Alege ce să se măsoare',
    all: 'Selectează tot ce este disponibil',
    none: 'Golește selecția',
  },

  menu: {
    select: 'Include în rulare',
    deselect: 'Exclude din rulare',
    runOnly: 'Rulează doar acesta',
    copyResult: 'Copiază rezultatul',
  },

  unavailable: {
    badge: 'Indisponibil',
    needsConsent: 'Acest test scrie date pe disc, așa că are nevoie mai întâi de acordul tău.',
    needsGraphicsContext:
      'Acest test are nevoie de un context grafic pe care Vitals nu îl creează cât rulează ca fereastră.',
    notImplemented: 'Încă nu este implementat. Este afișat ca să vezi ce urmează.',
    unknown: 'Acest test nu poate rula pe acest sistem.',
  },

  warning: {
    title: 'Calculatorul va deveni greu de folosit cât timp rulează.',
    body: 'Un test de performanță folosește intenționat tot ce are mașina. Închide ce lucrezi, lasă calculatorul în priză și nu îl folosi până se termină.',
    estimate_one: 'Testul selectat durează aproximativ {{count}} secundă.',
    estimate_few: 'Testele selectate durează în total aproximativ {{count}} secunde.',
    estimate_other: 'Testele selectate durează în total aproximativ {{count}} de secunde.',
    nothingSelected: 'Selectează cel puțin un test ca să începi.',
  },

  action: {
    start: 'Rulează testele',
    rerun: 'Rulează din nou',
    busy: 'Rulează…',
  },

  running: {
    region: 'Progresul testelor',
    title: 'Se măsoară {{name}}.',
    body: 'Cifrele rămase pe ecran sunt din rularea anterioară și nu sunt finale. Nu folosi calculatorul până se termină.',
    remaining_one: 'A mai rămas {{count}} test.',
    remaining_few: 'Au mai rămas {{count}} teste.',
    remaining_other: 'Au mai rămas {{count}} de teste.',
  },

  results: {
    title: 'Rezultate',
    total: 'Întreaga suită a durat {{seconds}} secunde.',
    headline_one: 'O singură rulare',
    headline_few: 'Mediana a {{count}} rulări',
    headline_other: 'Mediana a {{count}} de rulări',
    runs: 'Rulări individuale',
    spread: 'Cea mai bună {{best}}, cea mai slabă {{worst}}',
    variability: 'Diferența dintre rulări: {{percent}}',
    variabilitySingle: 'A existat o singură rulare, deci nu există o diferență de raportat.',
    medianWhy:
      'Se afișează mediana și nu media, pentru că o perturbare poate doar să încetinească o rulare, niciodată să o accelereze — o singură rulare blocată ar trage media în jos definitiv.',
    duration: 'A durat {{seconds}} secunde',
  },

  trust: {
    ok: 'Condițiile au fost curate',
    okBody: 'Nu s-a detectat nimic care să distorsioneze această cifră.',
    bad: 'Nu te baza pe această cifră',
    badIntro: 'A fost măsurată în condiții care schimbă rezultatul:',
    reason: {
      throttled:
        'Procesorul a fost limitat termic, deci nu a rulat la viteza de care este capabil.',
      onBattery:
        'Calculatorul era pe baterie. Windows limitează performanța ca să economisească energie, adesea la jumătate.',
      backgroundLoad:
        'Altceva folosea {{percent}} din mașină, deci s-a măsurat doar ce a rămas liber.',
      variability:
        'Rulările au diferit între ele cu {{percent}}, mai mult decât poate absorbi măsurătoarea.',
      tainted: 'Componenta de măsurare a semnalat condițiile ca fiind nepotrivite.',
    },
    conditions: 'Condiții',
    powerPlan: 'Plan de alimentare: {{plan}}',
    powerPlanUnknown: 'Plan de alimentare: neraportat',
    background: 'Încărcare de fundal: {{percent}}',
    temperature: 'Temperatură: {{start}} la început, {{end}} la final',
    temperatureUnknown: 'Temperatură: neraportată',
  },

  error: {
    failed: 'Rularea testelor a eșuat. {{message}}',
    stale: 'Se afișează ultima rulare finalizată. {{message}}',
    list: 'Nu s-a putut citi lista de teste. {{message}}',
  },

  idle: {
    title: 'Nu s-a măsurat încă nimic',
    body: 'Alege ce să se măsoare și pornește o rulare. Cifrele apar aici împreună cu condițiile în care au fost obținute.',
  },
  noHost: {
    title: 'Nu sosesc măsurători',
    body: 'Vitals nu poate ajunge la partea din el care rulează testele de performanță. Nu este nimic în neregulă cu mașina — de obicei repornirea aplicației Vitals rezolvă asta.',
  },
} as const;

/**
 * Registers the bundle.
 *
 * `deep: true, overwrite: false` — with `deep: false` the merge is shallow and
 * the incoming bundle wins, so `overwrite: false` would protect nothing.
 *
 * Must run after `initI18n`: `addResourceBundle` does not exist before
 * `init()`, and that throw at module scope once left the app on its splash
 * screen forever.
 */
export function registerBenchmarksStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerBenchmarksStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', BENCHMARKS_NS, en, true, false);
  i18n.addResourceBundle('ro', BENCHMARKS_NS, ro, true, false);
}

/** Exported for the parity test. */
export const bundles = { en, ro } as const;
