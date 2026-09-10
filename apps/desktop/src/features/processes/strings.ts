/**
 * Translation keys this screen needs that `packages/i18n` does not yet ship.
 *
 * This feature may not edit `packages/**`, so every string that has no key
 * yet is funnelled through one place with an English fallback. i18next
 * renders the fallback and still records the key as missing, so adding the
 * translations later is a data change in the locale files and nothing here
 * has to move.
 *
 * Keeping them in a single exported record — rather than sprinkling
 * `t('x', 'fallback')` across twenty components — is what makes "which keys
 * are missing?" answerable by reading one file instead of grepping.
 *
 * Newer strings live in the `processes` namespace below, English and
 * Romanian side by side, following the other feature bundles. The two
 * mechanisms coexist because moving the older keys would touch every call
 * site on the screen for no user-visible change.
 */

import { i18n } from '@vitals/i18n';

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
      title: 'Handle-uri',
      loading: 'Se citesc handle-urile…',
      empty: 'Nu s-a putut citi niciun handle.',
      capped: 'Se afișează primele {{shown}} din {{total}}.',
      unnamed: '(fără nume)',
      untyped: '(tip necunoscut)',
      expand: 'Arată handle-urile',
      collapse: 'Ascunde handle-urile',
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

export const MISSING_KEYS = {
  'process.column.cpu': 'CPU',
  'process.column.memory': 'Memory',
  'process.column.disk': 'Disk',
  'process.column.network': 'Network',
  'process.column.gpu': 'GPU',
  'process.columns': 'Columns',
  'process.filter.all': 'All',
  'process.filter.apps': 'Apps',
  'process.filter.background': 'Background',
  'process.filter.system': 'Windows processes',
  'process.group.byApp': 'Group by app',
  'process.order.paused': 'Order held',
  'process.order.pausedHint':
    'Rows keep their positions while you are pointing at the table, so a row cannot move out from under the cursor. Values keep updating.',
  'process.search.placeholder': 'Filter processes',
  'process.results': '{{shown}} of {{total}} processes',
  'process.empty.title': 'No process matches',
  'process.empty.body': 'Clear the search or choose a different filter.',
  'process.noSampler.title': 'No readings are arriving',
  'process.noSampler.body':
    'Vitals cannot reach the part of itself that measures your computer. Nothing is wrong with the machine — restarting Vitals usually fixes this.',
  'process.loading': 'Waiting for the first sample…',
  'process.action.endTreeCount': 'End process tree ({{count}})',
  'process.action.searchOnline': 'Search online',
  'process.action.copyDetails': 'Copy details',
  'process.action.priority.idle': 'Low',
  'process.action.priority.belowNormal': 'Below normal',
  'process.action.priority.normal': 'Normal',
  'process.action.priority.aboveNormal': 'Above normal',
  'process.action.priority.high': 'High',
  // Named for what it does rather than what Windows calls it. "Realtime" reads
  // as a promise of speed; it is actually a way to starve input handling and
  // make the machine look frozen.
  'process.action.priority.realtime': 'Realtime (can freeze the machine)',
  'process.action.priority.failed': 'Could not change the priority of {{name}}',
  'process.action.priority.changed': '{{name}} set to {{priority}}',
  'process.confirm.title': 'End {{name}}?',
  'process.confirm.suspendTitle': 'Suspend {{name}}?',
  'process.confirm.treeTitle': 'End {{name}} and {{count}} child processes?',
  'process.confirm.proceed': 'End process',
  'process.confirm.proceedSuspend': 'Suspend',
  'process.confirm.blocked': 'This cannot be done',
  'process.confirm.elevate': 'Retry as administrator',
  'process.risk.safe': 'Safe',
  'process.risk.disruptive': 'Disruptive',
  'process.risk.critical': 'Critical',
  'process.risk.forbidden': 'Blocked by Windows',
  'process.detail.title': 'Details',
  'process.detail.none': 'Select a process to see its details.',
  'process.detail.identity': 'Identity',
  'process.detail.resources': 'Resources',
  'process.detail.flags': 'Attributes',
  'process.flag.signed': 'Signed',
  'process.flag.signatureBroken': 'Signature broken',
  'process.flag.elevated': 'Elevated',
  'process.flag.efficiencyMode': 'Efficiency mode',
  'process.flag.wow64': '32-bit',
  'process.flag.critical': 'Critical',
  'process.flag.hasWindow': 'Has a window',
  'process.flag.packaged': 'Packaged',
  'process.flag.preexisting': 'Started before Vitals',
  'process.flag.shortLived': 'Short-lived',
  'process.flag.debugged': 'Debugger attached',
  'process.flag.managed': 'Managed',
  'process.kind.app': 'App',
  'process.kind.background': 'Background',
  'process.kind.service': 'Service',
  'process.kind.system': 'Windows',
  'process.kind.containerized': 'Container',
  'process.tree.expand': 'Expand {{name}}',
  'process.tree.collapse': 'Collapse {{name}}',
  'process.table.label': 'Processes',
  'process.error.failed': 'Could not complete the action: {{message}}',
} as const;

export type MissingKey = keyof typeof MISSING_KEYS;

/** The i18next `defaultValue` for a key we know is absent. */
export function fallback(key: MissingKey): string {
  return MISSING_KEYS[key];
}
