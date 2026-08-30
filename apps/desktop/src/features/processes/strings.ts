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
 */
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
  'process.action.notImplemented': 'Not wired up yet — the backend command does not exist.',
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
