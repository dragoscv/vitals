/**
 * Devices and sensors translations.
 *
 * Own namespace, same reasoning as the other feature bundles. English and
 * Romanian side by side so a key cannot be added to one and forgotten in the
 * other.
 *
 * The `gap.*` copy is the substance of this screen rather than chrome around
 * it. On most machines nothing is measurable, and the difference between a
 * product that looks broken and one that looks considered is entirely in
 * whether the absence is explained.
 */

import { i18n } from '@vitals/i18n';

export const DEVICES_NS = 'devices';

const en = {
  title: 'Devices & sensors',
  subtitle: 'What this machine exposes, and what it does not.',
  refresh: 'Refresh',
  updated: 'Updated {{time}}',
  cadence:
    'Refreshes every {{seconds}}s — reading sensors is slow enough that doing it faster would show up in the CPU figures this app reports.',
  cost: 'Last read took {{ms}} ms.',
  stale: 'Showing the last successful reading. {{message}}',

  power: {
    title: 'Power',
    line: 'Power source',
    mode: 'Power mode',
    scheme: 'Active scheme',
    saver: 'Battery saver',
    saverOn: 'On',
    saverOff: 'Off',
    noBattery: 'No battery — this machine runs on mains only.',
  },

  line: {
    ac: 'Mains (AC)',
    battery: 'Battery',
    unknown: 'Not reported',
  },
  lineUnknownHint:
    'The firmware declined to say. This is normal in a virtual machine, and Vitals will not guess.',

  mode: {
    bestPowerEfficiency: 'Best power efficiency',
    balanced: 'Balanced',
    bestPerformance: 'Best performance',
    custom: 'Custom or vendor scheme',
  },

  battery: {
    title: 'Battery',
    chemistry: 'Chemistry',
    charge: 'Charge',
    health: 'Health',
    rate: 'Rate',
    voltage: 'Voltage',
    cycles: 'Charge cycles',
    remaining: 'Estimated remaining',
    designCapacity: 'Design capacity',
    fullCapacity: 'Full-charge capacity',
    relativeCapacity:
      'This gauge reports relative capacity, so the figures above are counts rather than energy. The percentages are still valid.',
    ups: 'Uninterruptible power supply',
    upsHint:
      'This is a UPS, not a portable pack. Its runtime estimate means minutes to an orderly shutdown, not hours of use.',
    aggregate: 'Combined across all packs',
  },

  chargeState: {
    charging: 'Charging',
    discharging: 'Discharging',
    idle: 'Not charging',
    unknown: 'Not reported',
  },

  sensors: {
    title: 'Sensor readings',
    label: 'Sensor',
    value: 'Reading',
    source: 'Source',
    quality: 'Quality',
    // Export-only columns.
    key: 'Sensor ID',
    unit: 'Unit',
    none: 'Nothing measurable',
    noneBody:
      'No sensor on this machine is readable without a kernel driver or administrator rights. Everything Vitals could show, and what each one would need, is listed below.',
  },

  source: {
    acpiThermalZone: 'ACPI thermal zone',
    batteryMiniport: 'Battery miniport',
    systemPowerStatus: 'OS power status',
    vendorLibrary: 'GPU driver',
    kernelDriver: 'Sensors service (PawnIO)',
  },

  quality: {
    measured: 'Measured',
    derived: 'Derived',
    nameplate: 'Nameplate',
  },
  qualityHint: {
    measured: 'Read from a sensor, converted but not otherwise altered.',
    derived: 'Computed from other measured values, so it inherits their error.',
    nameplate: 'A fixed characteristic reported by firmware, not sampled.',
  },

  thermal: {
    title: 'Thermal zones',
    zone: 'Zone',
    temperature: 'Temperature',
    critical: 'Critical trip point',
    cooling: 'Cooling',
    active: 'Active (fan)',
    passive: 'Passive (throttling)',
    notZone:
      'An ACPI thermal zone is not a CPU temperature. The board vendor chooses the sensor behind it — often the chipset, or a skin sensor near the palm rest — so a zone reading well below the cores is two different sensors, not a fault.',
  },

  availability: {
    available: 'Zones were read.',
    accessDenied:
      'Windows refused the query. Reading ACPI thermal zones needs administrator rights on most builds, so this is a permissions result, not an absence of sensors.',
    noZonesPresent:
      'This firmware exposes no thermal zones. Many desktop boards delegate thermal management entirely to a Super-I/O chip, which ACPI never sees. Running as administrator would not change this.',
    providerMissing: 'The ACPI WMI provider is not registered on this system.',
  },

  gaps: {
    title: 'What Vitals cannot measure here',
    body: 'Each of these is absent for a specific reason, listed with it. Vitals would rather name the obstacle than show a plausible number it did not measure.',
    actionable: 'Could be unlocked',
    permanent: 'Not possible in this build',
    requirement: 'Requires',
    empty: 'Nothing is missing on this machine.',
  },

  capability: {
    thermals: 'Temperatures',
    powerDraw: 'Power draw',
    fanControl: 'Fans',
    other: 'Other',
  },

  reason: {
    notSupportedOnPlatform: 'Not available on this platform',
    noSuchHardware: 'The hardware is not present',
    needsElevation: 'Needs administrator rights',
    needsHelper: 'Needs the Vitals helper service',
    needsPlugin: 'Needs a kernel driver or vendor SDK Vitals does not ship',
    disabledByUser: 'Turned off in settings',
  },

  service: {
    title: 'CPU temperature and power',
    body: 'Vitals can read CPU package temperature, the hottest core and package power by installing a small service that runs as SYSTEM and uses the signed PawnIO driver. It only reads; it never changes a register. One administrator prompt, removable here at any time.',
    download:
      'The PawnIO driver is not installed yet, so installing will also download PawnIO 2.2.0 from its official GitHub release. The file is checked against a pinned hash before it runs.',
    install: 'Install sensors service',
    remove: 'Remove sensors service',
    working: 'Waiting for the administrator prompt…',
    installed: 'Installed. CPU readings appear with the next refresh.',
    removed: 'Removed. PawnIO stays installed, because other tools use it too.',
    failed: 'The sensors service could not be set up: {{message}}',
    notReading: 'The service is installed but cannot read this CPU: {{message}}',
    noHelper: 'This build does not include the sensors service.',
    state: {
      running: 'Reading',
      notReading: 'Installed, not reading',
      notInstalled: 'Not installed',
    },
  },

  unavailable: 'Not available',
  unavailableHint: 'Vitals reports no value rather than a stand-in.',

  noHost: {
    title: 'No readings are arriving',
    body: 'Vitals cannot reach the part of itself that reads sensors. Nothing is wrong with the machine — restarting Vitals usually fixes this.',
  },
} as const;

const ro = {
  title: 'Dispozitive și senzori',
  subtitle: 'Ce expune această mașină și ce nu.',
  refresh: 'Reîmprospătează',
  updated: 'Actualizat {{time}}',
  cadence:
    'Se reîmprospătează la fiecare {{seconds}} s — citirea senzorilor este suficient de lentă încât să apară în cifrele de procesor pe care le raportează chiar această aplicație.',
  cost: 'Ultima citire a durat {{ms}} ms.',
  stale: 'Se afișează ultima citire reușită. {{message}}',

  power: {
    title: 'Alimentare',
    line: 'Sursă de alimentare',
    mode: 'Mod de alimentare',
    scheme: 'Schemă activă',
    saver: 'Economisire baterie',
    saverOn: 'Pornită',
    saverOff: 'Oprită',
    noBattery: 'Fără baterie — această mașină funcționează doar de la rețea.',
  },

  line: {
    ac: 'Rețea (AC)',
    battery: 'Baterie',
    unknown: 'Neraportat',
  },
  lineUnknownHint:
    'Firmware-ul a refuzat să răspundă. Este normal într-o mașină virtuală, iar Vitals nu ghicește.',

  mode: {
    bestPowerEfficiency: 'Eficiență energetică maximă',
    balanced: 'Echilibrat',
    bestPerformance: 'Performanță maximă',
    custom: 'Schemă personalizată sau a producătorului',
  },

  battery: {
    title: 'Baterie',
    chemistry: 'Chimie',
    charge: 'Încărcare',
    health: 'Sănătate',
    rate: 'Rată',
    voltage: 'Tensiune',
    cycles: 'Cicluri de încărcare',
    remaining: 'Autonomie estimată',
    designCapacity: 'Capacitate nominală',
    fullCapacity: 'Capacitate la încărcare completă',
    relativeCapacity:
      'Acest indicator raportează capacitate relativă, deci cifrele de mai sus sunt numărători, nu energie. Procentele rămân valabile.',
    ups: 'Sursă neîntreruptibilă (UPS)',
    upsHint:
      'Acesta este un UPS, nu un acumulator portabil. Autonomia estimată înseamnă minute până la o oprire ordonată, nu ore de utilizare.',
    aggregate: 'Însumat pe toți acumulatorii',
  },

  chargeState: {
    charging: 'Se încarcă',
    discharging: 'Se descarcă',
    idle: 'Nu se încarcă',
    unknown: 'Neraportat',
  },

  sensors: {
    title: 'Citiri de la senzori',
    label: 'Senzor',
    value: 'Valoare',
    source: 'Sursă',
    quality: 'Calitate',
    key: 'ID senzor',
    unit: 'Unitate',
    none: 'Nimic măsurabil',
    noneBody:
      'Niciun senzor de pe această mașină nu poate fi citit fără un driver de kernel sau drepturi de administrator. Tot ce ar putea afișa Vitals, și de ce ar avea nevoie fiecare, este listat mai jos.',
  },

  source: {
    acpiThermalZone: 'Zonă termică ACPI',
    batteryMiniport: 'Miniport de baterie',
    systemPowerStatus: 'Starea de alimentare a sistemului',
    vendorLibrary: 'Driverul plăcii video',
    kernelDriver: 'Serviciul de senzori (PawnIO)',
  },

  quality: {
    measured: 'Măsurat',
    derived: 'Derivat',
    nameplate: 'Din specificații',
  },
  qualityHint: {
    measured: 'Citit de la un senzor, convertit dar nemodificat altfel.',
    derived: 'Calculat din alte valori măsurate, deci moștenește eroarea lor.',
    nameplate: 'O caracteristică fixă raportată de firmware, nu o măsurătoare.',
  },

  thermal: {
    title: 'Zone termice',
    zone: 'Zonă',
    temperature: 'Temperatură',
    critical: 'Prag critic',
    cooling: 'Răcire',
    active: 'Activă (ventilator)',
    passive: 'Pasivă (limitare)',
    notZone:
      'O zonă termică ACPI nu este temperatura procesorului. Producătorul plăcii alege senzorul din spatele ei — adesea chipsetul sau un senzor de suprafață — deci o zonă mult sub temperatura nucleelor înseamnă doi senzori diferiți, nu o defecțiune.',
  },

  availability: {
    available: 'Zonele au fost citite.',
    accessDenied:
      'Windows a refuzat interogarea. Citirea zonelor termice ACPI necesită drepturi de administrator pe majoritatea versiunilor, deci acesta este un rezultat de permisiuni, nu o lipsă de senzori.',
    noZonesPresent:
      'Acest firmware nu expune zone termice. Multe plăci de desktop lasă gestionarea termică complet pe seama unui cip Super-I/O, pe care ACPI nu îl vede. Rularea ca administrator nu ar schimba acest lucru.',
    providerMissing: 'Furnizorul WMI pentru ACPI nu este înregistrat pe acest sistem.',
  },

  gaps: {
    title: 'Ce nu poate măsura Vitals aici',
    body: 'Fiecare dintre acestea lipsește dintr-un motiv precis, listat alături. Vitals preferă să numească obstacolul decât să afișeze o cifră plauzibilă pe care nu a măsurat-o.',
    actionable: 'S-ar putea debloca',
    permanent: 'Imposibil în această versiune',
    requirement: 'Necesită',
    empty: 'Nu lipsește nimic pe această mașină.',
  },

  capability: {
    thermals: 'Temperaturi',
    powerDraw: 'Consum',
    fanControl: 'Ventilatoare',
    other: 'Altele',
  },

  reason: {
    notSupportedOnPlatform: 'Indisponibil pe această platformă',
    noSuchHardware: 'Componenta nu este prezentă',
    needsElevation: 'Necesită drepturi de administrator',
    needsHelper: 'Necesită serviciul auxiliar Vitals',
    needsPlugin:
      'Necesită un driver de kernel sau un SDK al producătorului pe care Vitals nu îl distribuie',
    disabledByUser: 'Dezactivat în setări',
  },

  service: {
    title: 'Temperatura și consumul procesorului',
    body: 'Vitals poate citi temperatura capsulei procesorului, cel mai fierbinte nucleu și consumul capsulei instalând un serviciu mic care rulează ca SYSTEM și folosește driverul semnat PawnIO. Doar citește; nu modifică niciun registru. O singură confirmare de administrator, iar îl poți elimina de aici oricând.',
    download:
      'Driverul PawnIO nu este încă instalat, așa că instalarea va descărca și PawnIO 2.2.0 din versiunea oficială de pe GitHub. Fișierul este verificat cu un hash fixat înainte de a rula.',
    install: 'Instalează serviciul de senzori',
    remove: 'Elimină serviciul de senzori',
    working: 'Se așteaptă confirmarea de administrator…',
    installed: 'Instalat. Citirile procesorului apar la următoarea reîmprospătare.',
    removed: 'Eliminat. PawnIO rămâne instalat, pentru că îl folosesc și alte programe.',
    failed: 'Serviciul de senzori nu a putut fi configurat: {{message}}',
    notReading: 'Serviciul este instalat, dar nu poate citi acest procesor: {{message}}',
    noHelper: 'Această versiune nu include serviciul de senzori.',
    state: {
      running: 'Citește',
      notReading: 'Instalat, nu citește',
      notInstalled: 'Neinstalat',
    },
  },

  unavailable: 'Indisponibil',
  unavailableHint: 'Vitals nu raportează nicio valoare în locul unei valori inventate.',

  noHost: {
    title: 'Nu sosesc măsurători',
    body: 'Vitals nu poate ajunge la partea din el care citește senzorii. Nu este nimic în neregulă cu mașina — de obicei repornirea aplicației Vitals rezolvă asta.',
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
export function registerDevicesStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerDevicesStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', DEVICES_NS, en, true, false);
  i18n.addResourceBundle('ro', DEVICES_NS, ro, true, false);
}

/** Exported for the parity test. */
export const bundles = { en, ro } as const;
