/**
 * Tracking page translations. English and Romanian side by side so a key
 * cannot be added to one and forgotten in the other — the same shape every
 * feature bundle here uses, registered from the route's `lazy` factory.
 */

import { i18n } from '@vitals/i18n';

export const TRACKING_NS = 'tracking';

const en = {
  title: 'Tracking',
  subtitle:
    'The cross-repo tracker index agentq builds from every repository it scans — plans, slices and their status, with the drift it found.',
  reload: 'Reload',
  /** Both are prefixed by a relative-time string from the UI formatter. */
  generated: 'Scan from {{ago}} ago',
  generatedNever: 'No scan recorded yet',

  locality: {
    label: 'Where this runs',
    localOnly: 'This machine only',
    connected: 'Codwen account connected',
    localHint:
      'The index is read from this machine and never leaves it. Sign in to Codwen to reach it from your other devices.',
    connectedHint:
      'The index is read from this machine; your Codwen account can reach this host from your other devices.',
  },

  loading: 'Reading the tracker index…',

  empty: {
    title: 'No tracker index yet',
    body: 'Nothing has scanned this machine (or none of the scanned repos declared a tracker). Run the scanner once and this page fills in:',
  },

  noHost: {
    title: 'No data',
    body: 'Vitals could not reach the component that reads the tracker index. Restarting Vitals usually fixes it.',
  },

  error: {
    title: 'The tracker index could not be read',
    body: 'The file is present but could not be parsed. Re-run the scanner to rebuild it.',
  },

  warnings: {
    title: 'Needs attention',
    drift_one: '{{count}} drift finding',
    drift_other: '{{count}} drift findings',
    blocked_one: '{{count}} blocked row',
    blocked_other: '{{count}} blocked rows',
    repos_one: 'in {{count}} repository',
    repos_other: 'in {{count}} repositories',
    none: 'No drift and no blocked rows.',
    driftHint: 'A drift finding means a tracker no longer matches the conventions it is read with.',
  },

  kpi: {
    repos: 'Repositories',
    trackers: 'Trackers',
    items: 'Rows',
    open: 'Open',
    drift: 'Drift',
  },

  status: {
    todo: 'To do',
    claimed: 'Claimed',
    'in-progress': 'In progress',
    blocked: 'Blocked',
    review: 'Review',
    done: 'Done',
    dropped: 'Dropped',
    deferred: 'Deferred',
  },

  repo: {
    branch: 'Branch',
    head: 'Commit',
    noGit: 'Not a clone',
    rootIssue: {
      'mapping-missing-at-root':
        'Its tracker mapping is not in the clone this index points at — the root moved, or it was never committed.',
      'mapping-on-origin-main-only':
        'The mapping exists on origin/main but not in the clone at this root — the clone is behind.',
      unknown: 'Its tracker mapping could not be found at this root.',
    },
  },

  tracker: {
    file: 'File',
    empty: 'No rows in this tracker',
    items_one: '{{count}} row',
    items_other: '{{count}} rows',
  },

  group: {
    ungrouped: 'Ungrouped',
  },

  item: {
    evidence: 'Evidence',
    notes: 'Notes',
  },

  resume: {
    title: 'Resume pack',
    session: 'Session {{id}}',
    goal: 'Goal',
    next: 'Next steps',
    nextEmpty: 'No open steps in the plan.',
    claims: 'Claims',
    claimsEmpty: 'Nothing claimed by this session or its subagents yet.',
    available: 'Open tasks',
    availableEmpty: 'Every open task is already claimed.',
    roster: 'Subagents',
    claim: 'Claim',
    complete: 'Complete',
    working: 'Working…',
    error: 'The resume pack could not be read: {{message}}',
    claimError: 'The claim was refused: {{message}}',
    noPlan: {
      title: 'No session plan recorded yet',
      body: 'A session writes its goal and next steps with track.mjs; until then there is nothing to resume.',
    },
    disabled: {
      noSession: 'No session to claim as',
    },
  },
};

// Untyped on purpose, as every feature bundle here is: Romanian needs a
// `_few` plural form that English has no equivalent for, so the object cannot
// be structurally identical to `en`.
const ro = {
  title: 'Urmărire',
  subtitle:
    'Indexul de urmărire cross-repo pe care agentq îl construiește din fiecare depozit scanat — planuri, felii și starea lor, împreună cu derapajele găsite.',
  reload: 'Reîncarcă',
  generated: 'Scanare de acum {{ago}}',
  generatedNever: 'Nicio scanare înregistrată încă',

  locality: {
    label: 'Unde rulează',
    localOnly: 'Doar acest calculator',
    connected: 'Cont Codwen conectat',
    localHint:
      'Indexul este citit de pe acest calculator și nu îl părăsește. Autentifică-te în Codwen pentru a-l atinge de pe celelalte dispozitive.',
    connectedHint:
      'Indexul este citit de pe acest calculator; contul tău Codwen poate atinge acest host de pe celelalte dispozitive.',
  },

  loading: 'Se citește indexul de urmărire…',

  empty: {
    title: 'Încă nu există un index de urmărire',
    body: 'Nimic nu a scanat acest calculator (sau niciunul dintre depozitele scanate nu a declarat un tracker). Rulează scanerul o dată și pagina se umple:',
  },

  noHost: {
    title: 'Fără date',
    body: 'Vitals nu poate ajunge la componenta care citește indexul de urmărire. Repornirea Vitals rezolvă de obicei problema.',
  },

  error: {
    title: 'Indexul de urmărire nu a putut fi citit',
    body: 'Fișierul există, dar nu a putut fi parsat. Re-rulează scanerul pentru a-l reconstrui.',
  },

  warnings: {
    title: 'Necesită atenție',
    drift_one: '{{count}} derapaj',
    drift_few: '{{count}} derapaje',
    drift_other: '{{count}} derapaje',
    blocked_one: '{{count}} rând blocat',
    blocked_few: '{{count}} rânduri blocate',
    blocked_other: '{{count}} rânduri blocate',
    repos_one: 'în {{count}} depozit',
    repos_few: 'în {{count}} depozite',
    repos_other: 'în {{count}} depozite',
    none: 'Niciun derapaj și niciun rând blocat.',
    driftHint: 'Un derapaj înseamnă că un tracker nu mai respectă convențiile cu care este citit.',
  },

  kpi: {
    repos: 'Depozite',
    trackers: 'Trackere',
    items: 'Rânduri',
    open: 'Deschise',
    drift: 'Derapaje',
  },

  status: {
    todo: 'De făcut',
    claimed: 'Revendicat',
    'in-progress': 'În lucru',
    blocked: 'Blocat',
    review: 'Verificare',
    done: 'Finalizat',
    dropped: 'Renunțat',
    deferred: 'Amânat',
  },

  repo: {
    branch: 'Ramură',
    head: 'Commit',
    noGit: 'Nu este un clone',
    rootIssue: {
      'mapping-missing-at-root':
        'Maparea trackerului nu este în clona către care pointează acest index — rădăcina s-a mutat sau nu a fost comisă niciodată.',
      'mapping-on-origin-main-only':
        'Maparea există pe origin/main, dar nu în clona de la această rădăcină — clona este în urmă.',
      unknown: 'Maparea trackerului nu a putut fi găsită la această rădăcină.',
    },
  },

  tracker: {
    file: 'Fișier',
    empty: 'Niciun rând în acest tracker',
    items_one: '{{count}} rând',
    items_few: '{{count}} rânduri',
    items_other: '{{count}} rânduri',
  },

  group: {
    ungrouped: 'Fără grup',
  },

  item: {
    evidence: 'Dovadă',
    notes: 'Note',
  },

  resume: {
    title: 'Pachet de reluare',
    session: 'Sesiunea {{id}}',
    goal: 'Obiectiv',
    next: 'Pași următori',
    nextEmpty: 'Niciun pas deschis în plan.',
    claims: 'Revendicări',
    claimsEmpty: 'Încă nimic revendicat de această sesiune sau de subagenții ei.',
    available: 'Sarcini deschise',
    availableEmpty: 'Toate sarcinile deschise sunt deja revendicate.',
    roster: 'Subagenți',
    claim: 'Revendică',
    complete: 'Finalizează',
    working: 'Se lucrează…',
    error: 'Pachetul de reluare nu a putut fi citit: {{message}}',
    claimError: 'Revendicarea a fost refuzată: {{message}}',
    noPlan: {
      title: 'Încă nu există un plan de sesiune',
      body: 'O sesiune își scrie obiectivul și pașii următori cu track.mjs; până atunci nu este nimic de reluat.',
    },
    disabled: {
      noSession: 'Nicio sesiune în numele căreia să revendici',
    },
  },
};

export const bundles = { en, ro } as const;

export function registerTrackingStrings(): void {
  for (const [locale, bundle] of Object.entries(bundles)) {
    i18n.addResourceBundle(locale, TRACKING_NS, bundle, true, false);
  }
}
