/**
 * Strings for the Processes screen.
 *
 * The older keys (`process.*`) live in the shared locale files in
 * `packages/i18n`; newer ones in the `processes` namespace below, English and
 * Romanian side by side, following the other feature bundles.
 *
 * There is deliberately no English fallback anywhere. This screen once
 * carried sixty-four strings as `t(key, 'English default')`, which rendered
 * fine in English and left Romanian users reading English on the most used
 * screen in the app, with every parity check green because the keys were in
 * neither locale. `noDefaultStrings.test.ts` now fails on that pattern.
 */

import { i18n } from '@vitals/i18n';
import type { TFunction } from 'i18next';

export const PROCESSES_NS = 'processes';

const en = {
  detail: {
    efficiency: {
      label: 'Efficiency mode',
      description: 'Runs the process on slower cores and lower clocks to save power.',
      unavailable: 'Cannot be read for this process',
      on: 'On',
      off: 'Off',
      failed: 'Could not change efficiency mode: {{message}}',
    },
    affinity: {
      title: 'Run on',
      hint: 'Limits which processors this process may use. Takes effect immediately.',
      applied: '{{name}} limited to {{cores}} processors',
      failed: 'Could not change which processors {{name}} runs on: {{message}}',
      preset: {
        all: 'All cores ({{cores}})',
        performance: 'Performance cores only ({{cores}})',
        efficiency: 'Efficiency cores only ({{cores}})',
        firstHalf: 'First half ({{cores}})',
        secondHalf: 'Second half ({{cores}})',
      },
    },
    handles: {
      title: 'Handles',
      loading: 'Reading handles…',
      empty: 'No handles could be read.',
      capped: 'Showing the first {{shown}} of {{total}}.',
      unnamed: '(unnamed)',
      untyped: '(unknown type)',
      expand: 'Show handles',
      collapse: 'Hide handles',
    },
    modules: {
      title: 'Modules',
      loading: 'Reading modules…',
      empty: 'No modules could be read.',
      capped: 'Showing the first {{shown}} of {{total}}.',
      expand: 'Show modules',
      collapse: 'Hide modules',
    },
    file: {
      unknownPath: 'The file location is not known for this process.',
      openFailed: 'Could not open the file location: {{message}}',
      propertiesFailed: 'Could not show properties: {{message}}',
    },
  },
  disk: {
    storageStack: 'Storage I/O, as Task Manager shows it.',
    allIo: 'All I/O including pipes and sockets. Run as administrator for storage-only figures.',
    unknown: 'Which counter feeds this column is not yet known.',
  },
  elevation: {
    declined: 'Administrator approval was declined, so nothing was changed.',
  },
  panel: {
    hide: 'Hide details panel',
    show: 'Show details panel',
  },
  column: {
    kind: 'Type',
    workingSet: 'Memory in use now',
    diskRead: 'Disk read',
    diskWrite: 'Disk write',
    power: 'Power usage',
    powerTrend: 'Power usage trend',
    parentPid: 'Parent PID',
    powerHint:
      'Estimated from CPU, GPU, disk and network activity. Windows does not report energy per process.',
  },
  power: {
    veryLow: 'Very low',
    low: 'Low',
    moderate: 'Moderate',
    high: 'High',
    veryHigh: 'Very high',
  },
  kind: {
    app: 'App',
    background: 'Background',
    service: 'Service',
    system: 'Windows',
    containerized: 'Container',
  },
  front: {
    action: 'Bring to front',
    noWindow: '{{name}} has no window to show.',
    failed: 'Could not bring {{name}} to the front: {{message}}',
  },
  priority: {
    changed: '{{name}} now runs at {{level}} priority',
  },
  search: {
    clear: 'Clear search',
  },
} as const;

const ro = {
  detail: {
    efficiency: {
      label: 'Mod eficiență',
      description:
        'Rulează procesul pe nuclee mai lente și frecvențe mai mici pentru a economisi energie.',
      unavailable: 'Nu poate fi citit pentru acest proces',
      on: 'Pornit',
      off: 'Oprit',
      failed: 'Nu s-a putut schimba modul eficiență: {{message}}',
    },
    affinity: {
      title: 'Rulează pe',
      hint: 'Limitează procesoarele pe care le poate folosi acest proces. Se aplică imediat.',
      applied: '{{name}} limitat la {{cores}} procesoare',
      failed: 'Nu s-a putut schimba pe ce procesoare rulează {{name}}: {{message}}',
      preset: {
        all: 'Toate nucleele ({{cores}})',
        performance: 'Doar nucleele de performanță ({{cores}})',
        efficiency: 'Doar nucleele eficiente ({{cores}})',
        firstHalf: 'Prima jumătate ({{cores}})',
        secondHalf: 'A doua jumătate ({{cores}})',
      },
    },
    handles: {
      title: 'Descriptori (handle-uri)',
      loading: 'Se citesc descriptorii…',
      empty: 'Nu s-a putut citi niciun descriptor.',
      capped: 'Se afișează primele {{shown}} din {{total}}.',
      unnamed: '(fără nume)',
      untyped: '(tip necunoscut)',
      expand: 'Arată descriptorii',
      collapse: 'Ascunde descriptorii',
    },
    modules: {
      title: 'Module',
      loading: 'Se citesc modulele…',
      empty: 'Nu s-a putut citi niciun modul.',
      capped: 'Se afișează primele {{shown}} din {{total}}.',
      expand: 'Arată modulele',
      collapse: 'Ascunde modulele',
    },
    file: {
      unknownPath: 'Locația fișierului nu este cunoscută pentru acest proces.',
      openFailed: 'Nu s-a putut deschide locația fișierului: {{message}}',
      propertiesFailed: 'Nu s-au putut afișa proprietățile: {{message}}',
    },
  },
  disk: {
    storageStack: 'I/O de stocare, așa cum arată Task Manager.',
    allIo:
      'Tot I/O-ul, inclusiv pipe-uri și socket-uri. Rulează ca administrator pentru cifre doar de stocare.',
    unknown: 'Nu se știe încă ce contor alimentează această coloană.',
  },
  elevation: {
    declined: 'Aprobarea de administrator a fost refuzată, deci nu s-a schimbat nimic.',
  },
  panel: {
    hide: 'Ascunde panoul de detalii',
    show: 'Arată panoul de detalii',
  },
  column: {
    kind: 'Tip',
    workingSet: 'Memorie folosită acum',
    diskRead: 'Citire disc',
    diskWrite: 'Scriere disc',
    power: 'Consum energie',
    powerTrend: 'Tendință consum',
    parentPid: 'PID părinte',
    powerHint:
      'Estimat din activitatea procesorului, plăcii video, discului și rețelei. Windows nu raportează consumul pe proces.',
  },
  power: {
    veryLow: 'Foarte mic',
    low: 'Mic',
    moderate: 'Moderat',
    high: 'Mare',
    veryHigh: 'Foarte mare',
  },
  kind: {
    app: 'Aplicație',
    background: 'Fundal',
    service: 'Serviciu',
    system: 'Windows',
    containerized: 'Container',
  },
  front: {
    action: 'Adu în față',
    noWindow: '{{name}} nu are nicio fereastră de afișat.',
    failed: 'Nu s-a putut aduce {{name}} în față: {{message}}',
  },
  priority: {
    changed: '{{name}} rulează acum cu prioritate {{level}}',
  },
  search: {
    clear: 'Șterge căutarea',
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
export function registerProcessesStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerProcessesStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', PROCESSES_NS, en, true, false);
  i18n.addResourceBundle('ro', PROCESSES_NS, ro, true, false);
}

/** Exported for the parity test. */
export const bundles = { en, ro } as const;

/**
 * A column’s label. Columns added since the bundle existed carry a
 * `processes:` key; the older ones a key in the shared locale files.
 */
export function columnLabel(t: TFunction, key: string): string {
  return t(key);
}
