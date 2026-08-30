/**
 * App history translations.
 *
 * Own namespace, matching the pattern from apps and devices. English and
 * Romanian bundles together so a key added to one cannot be forgotten in the
 * other.
 */

import { i18n } from '@vitals/i18n';

export const HISTORY_NS = 'history';

const en = {
  title: 'App history',
  subtitle:
    "Resource usage per application, accumulated from the moment Vitals first ran. This is not Windows' SRUM data — history starts from installation.",

  search: 'Filter by name or executable',
  clear: 'Clear the filter',
  refresh: 'Refresh',
  clearHistory: 'Clear history',
  sortLabel: 'Sort by',

  sort: {
    cpu: 'CPU time',
    disk: 'Disk I/O',
    memory: 'Peak memory',
    lastSeen: 'Last seen',
  },

  column: {
    name: 'Application',
    cpuTime: 'CPU time',
    diskRead: 'Disk read',
    diskWrite: 'Disk write',
    peakMemory: 'Peak memory',
    firstSeen: 'First seen',
    lastSeen: 'Last seen',
    sessions: 'Sessions',
  },

  summary: {
    count_one: '{{count}} application',
    count_other: '{{count}} applications',
    totalCpu: '{{time}} total CPU time',
    totalDisk: '{{size}} read, {{written}} written',
  },

  clearDialog: {
    title: 'Clear app history?',
    body: 'All accumulated usage data will be permanently deleted. History will restart from zero.',
    confirm: 'Clear history',
    cancel: 'Cancel',
  },

  empty: {
    title: 'No history yet',
    body: 'Vitals accumulates resource usage as applications run. This list will populate over time.',
  },

  filtered: {
    title: 'Nothing matches',
    body: 'Clear the search to see the full history.',
  },

  noHost: {
    title: 'No readings are arriving',
    body: 'Vitals cannot reach the part of itself that tracks app history. Restarting Vitals usually fixes this.',
  },

  stale: 'Showing the last successful reading. {{message}}',

  about: {
    title: 'About app history',
    ours: "This is Vitals' own accumulated history, starting from installation. It is <strong>not</strong> Windows' SRUM database.",
    taskManager:
      'Task Manager\'s "App history" tab reads SRUM, which requires elevation and an ESE database reader. Vitals accumulates from its own sampler instead, which starts empty on a fresh install.',
    starts:
      'History begins from the first time Vitals ran. Older usage is not retroactively included.',
  },
} as const;

const ro = {
  title: 'Istoric aplicații',
  subtitle:
    'Utilizarea de resurse pe aplicație, acumulată de când Vitals a rulat prima dată. Nu este baza SRUM a Windows — istoricul pornește de la instalare.',

  search: 'Filtrează după nume sau executabil',
  clear: 'Șterge filtrul',
  refresh: 'Reîncarcă',
  clearHistory: 'Șterge istoricul',
  sortLabel: 'Sortează după',

  sort: {
    cpu: 'Timp CPU',
    disk: 'I/O disc',
    memory: 'Memorie maximă',
    lastSeen: 'Văzut ultima dată',
  },

  column: {
    name: 'Aplicație',
    cpuTime: 'Timp CPU',
    diskRead: 'Citiri disc',
    diskWrite: 'Scrieri disc',
    peakMemory: 'Memorie maximă',
    firstSeen: 'Văzut prima dată',
    lastSeen: 'Văzut ultima dată',
    sessions: 'Sesiuni',
  },

  summary: {
    count_one: '{{count}} aplicație',
    count_few: '{{count}} aplicații',
    count_other: '{{count}} de aplicații',
    totalCpu: '{{time}} timp CPU total',
    totalDisk: '{{size}} citit, {{written}} scris',
  },

  clearDialog: {
    title: 'Ștergi istoricul aplicațiilor?',
    body: 'Toate datele de utilizare acumulate vor fi șterse permanent. Istoricul va reporni de la zero.',
    confirm: 'Șterge istoricul',
    cancel: 'Anulează',
  },

  empty: {
    title: 'Încă fără istoric',
    body: 'Vitals acumulează utilizarea de resurse pe măsură ce aplicațiile rulează. Lista se va popula în timp.',
  },

  filtered: {
    title: 'Nimic nu se potrivește',
    body: 'Șterge căutarea pentru a vedea istoricul complet.',
  },

  noHost: {
    title: 'Nu sosesc citiri',
    body: 'Vitals nu poate ajunge la componenta care urmărește istoricul aplicațiilor. Repornirea Vitals rezolvă de obicei problema.',
  },

  stale: 'Se arată ultima citire reușită. {{message}}',

  about: {
    title: 'Despre istoricul aplicațiilor',
    ours: 'Acesta este istoricul acumulat de Vitals, pornind de la instalare. <strong>Nu este</strong> baza de date SRUM a Windows.',
    taskManager:
      'Tab-ul "App history" din Task Manager citește SRUM, care necesită elevare și un cititor de baze ESE. Vitals acumulează din propriul sampler, care pornește gol la o instalare proaspătă.',
    starts:
      'Istoricul începe de când Vitals a rulat prima dată. Utilizarea mai veche nu este inclusă retroactiv.',
  },
} as const;

export const bundles = { en, ro } as const;

export function registerHistoryStrings(): void {
  Object.entries(bundles).forEach(([locale, bundle]) => {
    i18n.addResourceBundle(locale, HISTORY_NS, bundle, true, false);
  });
}
