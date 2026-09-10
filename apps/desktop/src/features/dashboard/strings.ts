/**
 * Dashboard translations.
 *
 * Registered as a `dashboard` namespace for the same reason the shell has one:
 * `@vitals/i18n` is a separate package, and the rule that no user-facing
 * string is written inline in a component matters more than which file the
 * string physically lives in. English and Romanian are defined side by side so
 * a key cannot be added to one and forgotten in the other.
 *
 * Migration is a move: drop `en`/`ro` into `packages/i18n/src/locales`, delete
 * this module, change `useTranslation(DASHBOARD_NS)` to `useTranslation()`.
 */

import { i18n } from '@vitals/i18n';

export const DASHBOARD_NS = 'dashboard';

const en = {
  title: 'Dashboard',
  subtitle: 'What your computer is doing right now.',

  layout: {
    edit: 'Edit layout',
    done: 'Done',
    add: 'Add widget',
    addTitle: 'Add a widget',
    addEmpty: 'Every widget your hardware supports is already on the dashboard.',
    reset: 'Reset to defaults',
    remove: 'Remove',
    moveUp: 'Move up',
    moveDown: 'Move down',
    wide: 'Full width',
    narrow: 'Half width',
    essential: 'This widget cannot be removed.',
    empty: 'Your dashboard is empty.',
    emptyBody: 'Add a widget to start watching something.',
  },

  widget: {
    cpu: { title: 'CPU', description: 'Load over time, per core, and clock speed.' },
    memory: { title: 'Memory', description: 'In use, cached, and committed.' },
    disk: { title: 'Disk', description: 'Read and write throughput across all drives.' },
    network: { title: 'Network', description: 'Upload and download across all adapters.' },
    gpu: { title: 'GPU', description: 'Utilisation and video memory per adapter.' },
    thermals: { title: 'Thermals', description: 'Temperatures your hardware reports.' },
    battery: { title: 'Battery', description: 'Charge, wear, and time remaining.' },
    topCpu: { title: 'Top by CPU', description: 'Which applications are using the processor.' },
    topMemory: { title: 'Top by memory', description: 'Which applications are using RAM.' },
    uptime: { title: 'System', description: 'Uptime, processes, threads, and handles.' },
    storage: { title: 'Storage', description: 'Free space on every drive.' },
    alerts: { title: 'Attention', description: 'Anything that looks wrong, and why.' },
  },

  cpu: {
    total: 'Total',
    kernel: 'Kernel',
    clock: 'Clock',
    cores: 'Cores',
    perCore: 'Per core',
    processes: 'Processes',
    threads: 'Threads',
    handles: 'Handles',
  },
  memory: {
    used: 'In use',
    available: 'Available',
    cached: 'Cached',
    committed: 'Committed',
    ofTotal: '{{used}} of {{total}}',
    speed: 'Speed',
    slots: '{{used}} of {{total}} slots',
  },
  disk: { read: 'Read', write: 'Write', active: 'Active', response: 'Response' },
  network: { down: 'Download', up: 'Upload', total: 'Session total', offline: 'Not connected' },
  gpu: { memory: 'Video memory', clock: 'Clock', engines: 'Engines' },
  battery: {
    remaining: '{{time}} remaining',
    charging: 'Charging',
    charged: 'Fully charged',
    health: 'Health',
    cycles: 'Cycles',
  },
  top: {
    processCount_one: '{{count}} process',
    processCount_other: '{{count}} processes',
    idle: 'Nothing is using this right now.',
    viewAll: 'View all processes',
  },
  storage: { free: '{{free}} free of {{total}}', used: '{{percent}}% used' },
  system: { uptime: 'Uptime', since: 'Running since {{date}}' },

  alert: {
    none: 'Nothing needs your attention.',
    noneBody: 'Vitals is watching CPU, memory, disks, temperature, and network health.',
    investigate: 'Investigate',
    // Appended to a toast title when a condition has gone away. Rendered in
    // Rust, so it is pushed across with the titles rather than read here.
    cleared: 'resolved',
    cpuSustained: {
      title: 'The processor has been busy for a while',
      cause:
        'CPU has averaged {{percent}}% for over {{seconds}} seconds. Something is running that you may not have started.',
    },
    cpuThrottled: {
      title: 'The processor is running slower than it can',
      thermal: 'It is too hot, so Windows is slowing it down to protect the hardware.',
      powerLimit: 'It has hit its power limit. On a laptop this is usually the charger.',
      currentLimit: 'It has hit a current limit set by the motherboard.',
      voltageDrop: 'The supply voltage dropped below what the current clock needs.',
      powerPolicy: 'A Windows power plan is capping it. You can change this in Performance.',
      unknown: 'The hardware reports throttling but not the reason.',
    },
    memoryPressure: {
      title: 'The machine is running out of memory',
      cause:
        '{{faults}} page faults a second with almost no memory free — Windows is moving pages to disk to keep going, and that is what slows everything down.',
    },
    memoryCommit: {
      title: 'Committed memory is close to the limit',
      cause:
        '{{percent}}% of the commit limit is in use. New allocations may start failing, which applications usually report as an out-of-memory crash.',
    },
    diskSaturated: {
      title: '{{disk}} is busy constantly',
      cause: 'The drive has had work queued the whole time for over a minute.',
    },
    diskLatency: {
      title: '{{disk}} is responding slowly',
      cause:
        'Requests are taking {{ms}} ms on average while the drive is fully busy — long enough that anything reading from it will feel unresponsive.',
    },
    diskSpace: {
      title: '{{disk}} is nearly full',
      cause:
        'Only {{percent}}% free. Below this Windows cannot reliably page, update, or write temporary files.',
    },
    diskHealth: {
      title: '{{disk}} reports that it is failing',
      cause:
        'This is the drive telling you, not a guess by Vitals. Back it up now and plan to replace it.',
    },
    gpuThrottled: {
      title: '{{gpu}} is running slower than it can',
      thermal: 'It is too hot and is clocking itself down.',
      powerLimit: 'It has hit its power limit.',
      currentLimit: 'It has hit a current limit.',
      voltageDrop: 'The supply voltage dropped below what the current clock needs.',
      powerPolicy: 'A power policy is capping it.',
      unknown: 'The hardware reports throttling but not the reason.',
    },
    thermalCpu: {
      title: 'The processor is overheating',
      cause:
        '{{celsius}}°C. At this temperature the chip protects itself by slowing down, and sustained heat shortens its life. Check that the fans and vents are clear.',
    },
    networkErrors: {
      title: '{{adapter}} is dropping packets',
      cause:
        '{{errors}} a second. On a cable this usually means a bad cable or port; on Wi-Fi, a weak signal or a congested channel.',
    },
    batteryLow: { title: 'Battery is low', cause: '{{percent}}% remaining and not charging.' },
    batteryHealth: {
      title: 'The battery has worn down',
      cause:
        'It holds {{percent}}% of its original capacity. This is normal with age, but runtime will keep shrinking.',
    },
  },

  diagnosis: {
    ask: 'Why is my PC slow?',
    title: 'Why is my PC slow?',
    description: 'What Vitals sees right now, and which applications are behind it.',
    healthy: {
      title: 'Nothing is holding your computer back.',
      body: 'CPU, memory, disks, temperature and network all look normal. If it still feels slow, the cause is probably a specific application waiting on something outside this machine — a server, a download, or a login.',
    },
    responsible: 'Responsible',
    share: '{{percent}} of the load',
    diffuse: 'No single application stands out — the load is spread across many small ones.',
    noProcessData: 'This kind of problem is not caused by an application.',
    lastMinute: 'Last minute',
    also: 'Also noticed',
    copy: 'Copy report',
    copied: 'Copied',
    close: 'Close',
    working: 'Looking…',
    failed: 'Vitals could not reach its own measurement engine to answer this.',
    report: {
      heading: 'Vitals — why is my PC slow?',
      generated: 'Generated {{date}}',
      responsible: 'Responsible processes:',
      also: 'Also noticed:',
      snapshot: 'Snapshot:',
      memory: 'Memory',
      processes: 'Processes',
      threads: 'Threads',
    },
  },

  waiting: 'Collecting data…',
  waitingBody: 'Charts fill in as samples arrive.',
  unavailable: 'Not reported by your hardware',
  noData: {
    title: 'No readings are arriving',
    noSampler:
      'Vitals cannot reach the part of itself that measures your computer. Nothing is wrong with the machine — restarting Vitals usually fixes this.',
  },
} as const;

const ro = {
  title: 'Panou',
  subtitle: 'Ce face calculatorul tău chiar acum.',

  layout: {
    edit: 'Editează aranjarea',
    done: 'Gata',
    add: 'Adaugă widget',
    addTitle: 'Adaugă un widget',
    addEmpty: 'Toate widgeturile compatibile cu hardware-ul tău sunt deja pe panou.',
    reset: 'Revino la valorile implicite',
    remove: 'Elimină',
    moveUp: 'Mută mai sus',
    moveDown: 'Mută mai jos',
    wide: 'Lățime completă',
    narrow: 'Jumătate de lățime',
    essential: 'Acest widget nu poate fi eliminat.',
    empty: 'Panoul tău este gol.',
    emptyBody: 'Adaugă un widget ca să începi să urmărești ceva.',
  },

  widget: {
    cpu: { title: 'Procesor', description: 'Încărcarea în timp, pe nucleu și frecvența.' },
    memory: { title: 'Memorie', description: 'Utilizată, în cache și angajată.' },
    disk: { title: 'Disc', description: 'Citire și scriere pe toate unitățile.' },
    network: { title: 'Rețea', description: 'Trafic trimis și primit pe toate adaptoarele.' },
    gpu: { title: 'Placă video', description: 'Utilizare și memorie video pentru fiecare placă.' },
    thermals: { title: 'Temperaturi', description: 'Temperaturile raportate de hardware.' },
    battery: { title: 'Baterie', description: 'Nivel, uzură și timp rămas.' },
    topCpu: { title: 'Top procesor', description: 'Ce aplicații folosesc procesorul.' },
    topMemory: { title: 'Top memorie', description: 'Ce aplicații folosesc memoria.' },
    uptime: { title: 'Sistem', description: 'Timp de funcționare, procese, fire și descriptori.' },
    storage: { title: 'Stocare', description: 'Spațiul liber pe fiecare unitate.' },
    alerts: { title: 'Atenție', description: 'Ce pare în neregulă și de ce.' },
  },

  cpu: {
    total: 'Total',
    kernel: 'Nucleu',
    clock: 'Frecvență',
    cores: 'Nuclee',
    perCore: 'Pe nucleu',
    processes: 'Procese',
    threads: 'Fire',
    handles: 'Descriptori',
  },
  memory: {
    used: 'Utilizată',
    available: 'Disponibilă',
    cached: 'În cache',
    committed: 'Angajată',
    ofTotal: '{{used}} din {{total}}',
    speed: 'Frecvență',
    slots: '{{used}} din {{total}} sloturi',
  },
  disk: { read: 'Citire', write: 'Scriere', active: 'Activ', response: 'Răspuns' },
  network: { down: 'Descărcare', up: 'Încărcare', total: 'Total sesiune', offline: 'Neconectat' },
  gpu: { memory: 'Memorie video', clock: 'Frecvență', engines: 'Motoare' },
  battery: {
    remaining: '{{time}} rămase',
    charging: 'Se încarcă',
    charged: 'Încărcată complet',
    health: 'Stare',
    cycles: 'Cicluri',
  },
  top: {
    processCount_one: '{{count}} proces',
    processCount_few: '{{count}} procese',
    processCount_other: '{{count}} de procese',
    idle: 'Nimic nu folosește asta acum.',
    viewAll: 'Vezi toate procesele',
  },
  storage: { free: '{{free}} liberi din {{total}}', used: '{{percent}}% utilizat' },
  system: { uptime: 'Timp de funcționare', since: 'Pornit din {{date}}' },

  alert: {
    none: 'Nimic nu necesită atenția ta.',
    noneBody: 'Vitals urmărește procesorul, memoria, discurile, temperatura și starea rețelei.',
    investigate: 'Investighează',
    cleared: 'rezolvat',
    cpuSustained: {
      title: 'Procesorul este solicitat de ceva timp',
      cause:
        'Procesorul a fost în medie la {{percent}}% timp de peste {{seconds}} secunde. Rulează ceva ce poate nu ai pornit tu.',
    },
    cpuThrottled: {
      title: 'Procesorul rulează mai încet decât poate',
      thermal: 'Este prea cald, așa că Windows îl încetinește ca să protejeze hardware-ul.',
      powerLimit: 'A atins limita de putere. Pe un laptop, de obicei este de la încărcător.',
      currentLimit: 'A atins o limită de curent impusă de placa de bază.',
      voltageDrop: 'Tensiunea de alimentare a scăzut sub necesarul frecvenței curente.',
      powerPolicy: 'Un plan de alimentare Windows îl limitează. Poți schimba asta în Performanță.',
      unknown: 'Hardware-ul raportează limitare, dar nu și motivul.',
    },
    memoryPressure: {
      title: 'Calculatorul rămâne fără memorie',
      cause:
        '{{faults}} erori de pagină pe secundă și aproape deloc memorie liberă — Windows mută pagini pe disc ca să facă față, iar asta încetinește totul.',
    },
    memoryCommit: {
      title: 'Memoria angajată este aproape de limită',
      cause:
        '{{percent}}% din limita de angajare este folosită. Alocările noi pot începe să eșueze, ceea ce aplicațiile raportează de obicei ca lipsă de memorie.',
    },
    diskSaturated: {
      title: '{{disk}} este ocupat continuu',
      cause: 'Unitatea a avut lucru în așteptare tot timpul, de peste un minut.',
    },
    diskLatency: {
      title: '{{disk}} răspunde încet',
      cause:
        'Cererile durează în medie {{ms}} ms cât timp unitatea este complet ocupată — suficient cât orice citește de acolo să pară blocat.',
    },
    diskSpace: {
      title: '{{disk}} este aproape plin',
      cause:
        'Doar {{percent}}% liber. Sub acest prag Windows nu mai poate pagina, actualiza sau scrie fișiere temporare în mod fiabil.',
    },
    diskHealth: {
      title: '{{disk}} raportează că se defectează',
      cause:
        'Îți spune unitatea însăși, nu este o presupunere a Vitals. Fă o copie de siguranță acum și pregătește-te să o înlocuiești.',
    },
    gpuThrottled: {
      title: '{{gpu}} rulează mai încet decât poate',
      thermal: 'Este prea caldă și își reduce singură frecvența.',
      powerLimit: 'A atins limita de putere.',
      currentLimit: 'A atins o limită de curent.',
      voltageDrop: 'Tensiunea de alimentare a scăzut sub necesarul frecvenței curente.',
      powerPolicy: 'O politică de alimentare o limitează.',
      unknown: 'Hardware-ul raportează limitare, dar nu și motivul.',
    },
    thermalCpu: {
      title: 'Procesorul se supraîncălzește',
      cause:
        '{{celsius}}°C. La această temperatură cipul se protejează încetinind, iar căldura susținută îi scurtează viața. Verifică dacă ventilatoarele și fantele sunt libere.',
    },
    networkErrors: {
      title: '{{adapter}} pierde pachete',
      cause:
        '{{errors}} pe secundă. Pe cablu asta înseamnă de obicei un cablu sau un port defect; pe Wi-Fi, semnal slab sau un canal aglomerat.',
    },
    batteryLow: { title: 'Bateria este descărcată', cause: '{{percent}}% rămas și nu se încarcă.' },
    batteryHealth: {
      title: 'Bateria s-a uzat',
      cause:
        'Mai păstrează {{percent}}% din capacitatea inițială. Este normal odată cu vârsta, dar autonomia va scădea în continuare.',
    },
  },

  diagnosis: {
    ask: 'De ce merge încet calculatorul?',
    title: 'De ce merge încet calculatorul?',
    description: 'Ce vede Vitals acum și ce aplicații sunt responsabile.',
    healthy: {
      title: 'Nimic nu îți încetinește calculatorul.',
      body: 'Procesorul, memoria, discurile, temperatura și rețeaua arată normal. Dacă tot pare lent, cauza e probabil o aplicație care așteaptă ceva din afara acestui calculator — un server, o descărcare sau o autentificare.',
    },
    responsible: 'Responsabile',
    share: '{{percent}} din încărcare',
    diffuse:
      'Nicio aplicație nu iese în evidență — încărcarea e împărțită între multe aplicații mici.',
    noProcessData: 'Acest tip de problemă nu e cauzat de o aplicație.',
    lastMinute: 'Ultimul minut',
    also: 'Observate și',
    copy: 'Copiază raportul',
    copied: 'Copiat',
    close: 'Închide',
    working: 'Se analizează…',
    failed: 'Vitals nu a putut ajunge la propriul motor de măsurare pentru a răspunde.',
    report: {
      heading: 'Vitals — de ce merge încet calculatorul?',
      generated: 'Generat {{date}}',
      responsible: 'Procese responsabile:',
      also: 'Observate și:',
      snapshot: 'Instantaneu:',
      memory: 'Memorie',
      processes: 'Procese',
      threads: 'Fire de execuție',
    },
  },

  waiting: 'Se colectează date…',
  waitingBody: 'Graficele se completează pe măsură ce sosesc măsurătorile.',
  unavailable: 'Neraportat de hardware-ul tău',
  noData: {
    title: 'Nu sosesc măsurători',
    noSampler:
      'Vitals nu poate ajunge la partea din el care măsoară calculatorul. Nu este nimic în neregulă cu mașina — de obicei repornirea aplicației Vitals rezolvă asta.',
  },
} as const;

/**
 * Registers the bundle.
 *
 * Must be called after `initI18n`, for the reason documented at length on
 * `registerShellStrings` — i18next has no `addResourceBundle` before `init()`,
 * and a throw here at module scope leaves the app on its splash forever.
 *
 * `deep: true, overwrite: false` so that once these keys move into
 * `@vitals/i18n` the real translations win and this module becomes inert.
 *
 * Both flags matter, and the pairing is not obvious: with `deep: false`
 * i18next shallow-merges and the incoming bundle replaces the existing one
 * wholesale, so `overwrite: false` protects nothing. A test asserting that an
 * existing translation survives is what surfaced this; without it the
 * migration would have silently shadowed the real strings.
 */
export function registerDashboardStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerDashboardStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', DASHBOARD_NS, en, true, false);
  i18n.addResourceBundle('ro', DASHBOARD_NS, ro, true, false);
}

/** Exported for the parity test, which asserts en and ro define the same keys. */
export const bundles = { en, ro } as const;
