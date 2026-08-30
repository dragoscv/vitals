/**
 * Performance page translations.
 *
 * Registered as its own namespace for the same reason the shell and dashboard
 * have one — `@vitals/i18n` is a separate package and the rule that no
 * user-facing string is written inline in a component matters more than which
 * file it physically lives in. English and Romanian sit side by side so a key
 * cannot be added to one and forgotten in the other.
 */

import { i18n } from '@vitals/i18n';

export const PERFORMANCE_NS = 'performance';

const en = {
  title: 'Performance',
  subtitle: 'Every resource, in detail.',

  rail: {
    label: 'Resources',
    showVirtual: 'Show virtual adapters',
    showVirtualHint:
      'Hyper-V switches, WSL, VPNs and loopback. Hidden by default because they are rarely the answer to "why is this slow".',
  },

  cpu: {
    title: 'CPU',
    utilisation: 'Utilisation',
    speed: 'Speed',
    baseSpeed: 'Base speed',
    maxSpeed: 'Maximum speed',
    processes: 'Processes',
    threads: 'Threads',
    handles: 'Handles',
    uptime: 'Up time',
    kernelTime: 'Kernel time',
    contextSwitches: 'Context switches',
    interrupts: 'Interrupts',
    perCore: 'Logical processors',
    coreLabel: 'Core {{index}}',
    throttled: 'Running below capability',
  },

  memory: {
    title: 'Memory',
    inUse: 'In use',
    available: 'Available',
    cached: 'Cached',
    cachedHint:
      'File data Windows is keeping in RAM because it is free to do so. Counted as available, and given back the moment a program needs it.',
    committed: 'Committed',
    commitHint:
      'Memory programs have reserved, which can exceed physical RAM. When this reaches its limit, allocations start failing.',
    pagedPool: 'Paged pool',
    nonPagedPool: 'Non-paged pool',
    hardwareReserved: 'Hardware reserved',
    swap: 'Page file',
    faults: 'Page faults',
    faultsHint:
      'The number that actually indicates memory pressure. A machine at 90% with no faulting is fine; one at 60% that is faulting heavily is not.',
    speed: 'Speed',
    slots: 'Slots used',
    formFactor: 'Form factor',
    composition: 'Composition',
  },

  gpu: {
    title: 'GPU',
    engines: 'Engines',
    enginesHint:
      'A single "GPU %" hides the case that matters: a machine can sit at 5% overall while the video decode engine is pinned.',
    dedicatedMemory: 'Dedicated memory',
    sharedMemory: 'Shared memory',
    coreClock: 'Core clock',
    memoryClock: 'Memory clock',
    driver: 'Driver version',
    power: 'Power draw',
    fan: 'Fan',
    vendor: 'Vendor',
  },

  disk: {
    title: 'Disk',
    activeTime: 'Active time',
    activeTimeHint: 'Share of the last second the drive had at least one request outstanding.',
    responseTime: 'Average response time',
    responseHint:
      'The figure that correlates with a machine feeling slow — far more than throughput does.',
    readSpeed: 'Read',
    writeSpeed: 'Write',
    queueDepth: 'Queue depth',
    capacity: 'Capacity',
    used: 'Used',
    free: 'Free',
    model: 'Model',
    type: 'Type',
    health: 'Health',
    lifeRemaining: 'Life remaining',
    powerOnHours: 'Powered on for',
    totalWritten: 'Total written',
    reallocated: 'Reallocated sectors',
    failing: 'This drive reports that it is failing',
    failingHint: 'The drive is telling you, not Vitals. Back it up now and plan to replace it.',
  },

  network: {
    title: 'Network',
    send: 'Send',
    receive: 'Receive',
    sessionTotal: 'Since Vitals started',
    linkSpeed: 'Link speed',
    adapter: 'Adapter',
    connection: 'Connection type',
    ipv4: 'IPv4 address',
    ipv6: 'IPv6 address',
    mac: 'Physical address',
    ssid: 'Network name',
    signal: 'Signal strength',
    errors: 'Dropped packets',
    errorsHint:
      'Invisible in Task Manager, and the first sign of a failing cable or a saturated link.',
    connected: 'Connected',
    disconnected: 'Not connected',
  },

  thermals: {
    title: 'Thermals',
    subtitle: 'Every temperature your hardware is willing to report.',
    cpuPackage: 'CPU package',
    gpuCore: '{{name}} core',
    gpuHotspot: '{{name}} hotspot',
    diskTemp: '{{name}}',
    batteryTemp: 'Battery',
    fanSpeed: 'Fan speed',
    limits: 'Limits',
    gaps: 'What cannot be measured',
    gapsHint:
      'These need a kernel-mode driver Vitals does not install. Nothing is broken — the readings are simply not reachable from a normal program.',
  },

  kind: {
    hdd: 'Hard disk',
    ssd: 'Solid state',
    nvme: 'NVMe',
    removable: 'Removable',
    network: 'Network drive',
    optical: 'Optical',
    ethernet: 'Ethernet',
    wiFi: 'Wi-Fi',
    cellular: 'Cellular',
    bluetooth: 'Bluetooth',
    loopback: 'Loopback',
    virtual: 'Virtual',
    vpn: 'VPN',
    unknown: 'Unknown',
  },

  throttle: {
    thermal: 'Too hot',
    powerLimit: 'Power limit reached',
    currentLimit: 'Current limit reached',
    voltageDrop: 'Voltage dropped',
    powerPolicy: 'Limited by a power plan',
    unknown: 'Throttled, reason not reported',
  },

  unavailable: 'Not reported by your hardware',
  noData: {
    title: 'No readings are arriving',
    body: 'Vitals cannot reach the part of itself that measures your computer. Nothing is wrong with the machine — restarting Vitals usually fixes this.',
  },
} as const;

const ro = {
  title: 'Performanță',
  subtitle: 'Fiecare resursă, în detaliu.',

  rail: {
    label: 'Resurse',
    showVirtual: 'Arată adaptoarele virtuale',
    showVirtualHint:
      'Switch-uri Hyper-V, WSL, VPN-uri și loopback. Ascunse implicit pentru că rareori explică de ce ceva este lent.',
  },

  cpu: {
    title: 'Procesor',
    utilisation: 'Utilizare',
    speed: 'Frecvență',
    baseSpeed: 'Frecvență de bază',
    maxSpeed: 'Frecvență maximă',
    processes: 'Procese',
    threads: 'Fire',
    handles: 'Descriptori',
    uptime: 'Timp de funcționare',
    kernelTime: 'Timp în nucleu',
    contextSwitches: 'Comutări de context',
    interrupts: 'Întreruperi',
    perCore: 'Procesoare logice',
    coreLabel: 'Nucleu {{index}}',
    throttled: 'Rulează sub capacitate',
  },

  memory: {
    title: 'Memorie',
    inUse: 'Utilizată',
    available: 'Disponibilă',
    cached: 'În cache',
    cachedHint:
      'Date din fișiere pe care Windows le păstrează în RAM pentru că poate. Sunt considerate disponibile și sunt eliberate imediat ce un program are nevoie.',
    committed: 'Angajată',
    commitHint:
      'Memorie rezervată de programe, care poate depăși memoria fizică. Când atinge limita, alocările încep să eșueze.',
    pagedPool: 'Pool paginabil',
    nonPagedPool: 'Pool nepaginabil',
    hardwareReserved: 'Rezervată de hardware',
    swap: 'Fișier de paginare',
    faults: 'Erori de pagină',
    faultsHint:
      'Numărul care indică într-adevăr presiunea pe memorie. O mașină la 90% fără erori este în regulă; una la 60% cu erori multe nu este.',
    speed: 'Frecvență',
    slots: 'Sloturi folosite',
    formFactor: 'Format',
    composition: 'Compoziție',
  },

  gpu: {
    title: 'Placă video',
    engines: 'Motoare',
    enginesHint:
      'Un singur procent „GPU" ascunde exact cazul care contează: sistemul poate sta la 5% în total în timp ce motorul de decodare video este la maxim.',
    dedicatedMemory: 'Memorie dedicată',
    sharedMemory: 'Memorie partajată',
    coreClock: 'Frecvență nucleu',
    memoryClock: 'Frecvență memorie',
    driver: 'Versiune driver',
    power: 'Consum',
    fan: 'Ventilator',
    vendor: 'Producător',
  },

  disk: {
    title: 'Disc',
    activeTime: 'Timp activ',
    activeTimeHint:
      'Proporția din ultima secundă în care unitatea a avut cel puțin o cerere în așteptare.',
    responseTime: 'Timp mediu de răspuns',
    responseHint:
      'Valoarea care se corelează cu senzația de lentoare — mult mai mult decât viteza de transfer.',
    readSpeed: 'Citire',
    writeSpeed: 'Scriere',
    queueDepth: 'Cereri în coadă',
    capacity: 'Capacitate',
    used: 'Utilizat',
    free: 'Liber',
    model: 'Model',
    type: 'Tip',
    health: 'Stare',
    lifeRemaining: 'Durată rămasă',
    powerOnHours: 'Pornit de',
    totalWritten: 'Total scris',
    reallocated: 'Sectoare realocate',
    failing: 'Această unitate raportează că se defectează',
    failingHint:
      'Îți spune unitatea însăși, nu Vitals. Fă o copie de siguranță acum și pregătește-te să o înlocuiești.',
  },

  network: {
    title: 'Rețea',
    send: 'Trimis',
    receive: 'Primit',
    sessionTotal: 'De la pornirea Vitals',
    linkSpeed: 'Viteza legăturii',
    adapter: 'Adaptor',
    connection: 'Tip de conexiune',
    ipv4: 'Adresă IPv4',
    ipv6: 'Adresă IPv6',
    mac: 'Adresă fizică',
    ssid: 'Nume rețea',
    signal: 'Putere semnal',
    errors: 'Pachete pierdute',
    errorsHint:
      'Invizibile în Task Manager și primul semn al unui cablu defect sau al unei legături saturate.',
    connected: 'Conectat',
    disconnected: 'Neconectat',
  },

  thermals: {
    title: 'Temperaturi',
    subtitle: 'Fiecare temperatură pe care hardware-ul tău acceptă să o raporteze.',
    cpuPackage: 'Pachet procesor',
    gpuCore: '{{name}} nucleu',
    gpuHotspot: '{{name}} punct fierbinte',
    diskTemp: '{{name}}',
    batteryTemp: 'Baterie',
    fanSpeed: 'Turație ventilator',
    limits: 'Limite',
    gaps: 'Ce nu poate fi măsurat',
    gapsHint:
      'Acestea necesită un driver în mod nucleu pe care Vitals nu îl instalează. Nu este nimic stricat — valorile pur și simplu nu sunt accesibile dintr-un program obișnuit.',
  },

  kind: {
    hdd: 'Disc mecanic',
    ssd: 'SSD',
    nvme: 'NVMe',
    removable: 'Detașabil',
    network: 'Unitate de rețea',
    optical: 'Optic',
    ethernet: 'Ethernet',
    wiFi: 'Wi-Fi',
    cellular: 'Celular',
    bluetooth: 'Bluetooth',
    loopback: 'Loopback',
    virtual: 'Virtual',
    vpn: 'VPN',
    unknown: 'Necunoscut',
  },

  throttle: {
    thermal: 'Prea cald',
    powerLimit: 'Limită de putere atinsă',
    currentLimit: 'Limită de curent atinsă',
    voltageDrop: 'Tensiune scăzută',
    powerPolicy: 'Limitat de un plan de alimentare',
    unknown: 'Limitat, motivul nu este raportat',
  },

  unavailable: 'Neraportat de hardware-ul tău',
  noData: {
    title: 'Nu sosesc măsurători',
    body: 'Vitals nu poate ajunge la partea din el care măsoară calculatorul. Nu este nimic în neregulă cu mașina — de obicei repornirea aplicației Vitals rezolvă asta.',
  },
} as const;

/**
 * Registers the bundle.
 *
 * `deep: true, overwrite: false` — both flags matter. With `deep: false`
 * i18next replaces the bundle wholesale and `overwrite: false` protects
 * nothing, which is how the shell's registration would have shadowed the real
 * translations at migration time.
 *
 * Must be called after `initI18n`: i18next has no `addResourceBundle` before
 * `init()`, and that throw at module scope is what once left the app on its
 * splash screen forever.
 */
export function registerPerformanceStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerPerformanceStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', PERFORMANCE_NS, en, true, false);
  i18n.addResourceBundle('ro', PERFORMANCE_NS, ro, true, false);
}

/** Exported for the parity test, which asserts en and ro define the same keys. */
export const bundles = { en, ro } as const;
