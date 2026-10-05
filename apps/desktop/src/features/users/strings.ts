/**
 * Users translations.
 *
 * Own namespace, same reasoning as the other feature bundles. English and
 * Romanian side by side so a key cannot be added to one and forgotten in the
 * other.
 */

import { i18n } from '@vitals/i18n';

export const USERS_NS = 'users';

const en = {
  title: 'Users',
  subtitle: 'Every logon session on this machine and what it is consuming.',
  refresh: 'Refresh',
  search: 'Filter sessions',
  clear: 'Clear the filter',

  counts: {
    sessions: '{{count}} session',
    sessions_other: '{{count}} sessions',
    interactive: '{{count}} interactive',
    services: 'Session 0 (Services)',
  },

  state: {
    active: 'Active',
    connected: 'Connected',
    disconnected: 'Disconnected',
    idle: 'Idle',
    listen: 'Listening',
    shadow: 'Shadow',
    unknown: 'Unknown state',
  },

  column: {
    user: 'User',
    sessionId: 'Session ID',
    state: 'State',
    client: 'Client',
    logonTime: 'Logon time',
    processes: 'Processes',
    cpu: 'CPU',
    memory: 'Memory',
  },

  empty: {
    title: 'No sessions',
    body: 'No logon sessions were enumerated. This should never happen — at minimum, session 0 should exist.',
  },

  error: {
    title: 'Failed to load sessions',
  },

  noHost: {
    title: 'No sessions are arriving',
    body: 'Vitals cannot reach the part of itself that reads logon sessions. Restarting Vitals usually fixes this.',
  },

  remote: 'Remote desktop session',
  unavailable: 'Not available',
  logonTimeUnavailable: 'Logon time could not be determined',
  menu: {
    copyName: 'Copy user name',
    copyDetails: 'Copy session details',
  },
  requiresElevation:
    'Some session details require administrator rights. Run Vitals elevated to see them.',
};

const ro = {
  title: 'Utilizatori',
  subtitle: 'Toate sesiunile de logon pe această mașină și ce consumă.',
  refresh: 'Reîmprospătare',
  search: 'Filtru sesiuni',
  clear: 'Șterge filtrul',

  counts: {
    sessions: '{{count}} sesiune',
    sessions_other: '{{count}} sesiuni',
    interactive: '{{count}} interactive',
    services: 'Sesiunea 0 (Services)',
  },

  state: {
    active: 'Activă',
    connected: 'Conectată',
    disconnected: 'Deconectată',
    idle: 'Inactivă',
    listen: 'Ascultare',
    shadow: 'Shadow',
    unknown: 'Stare necunoscută',
  },

  column: {
    user: 'Utilizator',
    sessionId: 'ID sesiune',
    state: 'Stare',
    client: 'Client',
    logonTime: 'Ora logon',
    processes: 'Procese',
    cpu: 'CPU',
    memory: 'Memorie',
  },

  empty: {
    title: 'Nicio sesiune',
    body: 'Nu au fost enumerate sesiuni de logon. Aceasta nu ar trebui să se întâmple niciodată — cel puțin sesiunea 0 ar trebui să existe.',
  },

  error: {
    title: 'Eșec la încărcarea sesiunilor',
  },

  noHost: {
    title: 'Nu sosesc sesiuni',
    body: 'Vitals nu poate ajunge la componenta care citește sesiunile de autentificare. Repornirea Vitals rezolvă de obicei problema.',
  },

  remote: 'Sesiune desktop la distanță',
  unavailable: 'Indisponibil',
  logonTimeUnavailable: 'Ora de logon nu a putut fi determinată',
  menu: {
    copyName: 'Copiază numele utilizatorului',
    copyDetails: 'Copiază detaliile sesiunii',
  },
  requiresElevation:
    'Unele detalii ale sesiunilor necesită drepturi de administrator. Rulați Vitals cu privilegii elevate pentru a le vedea.',
};

export const bundles = { en, ro } as const;

export function registerUsersStrings(): void {
  for (const [locale, bundle] of Object.entries(bundles)) {
    i18n.addResourceBundle(locale, USERS_NS, bundle, true, false);
  }
}
