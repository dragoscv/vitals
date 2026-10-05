/**
 * Network connections translations.
 *
 * Own namespace, same reasoning as the shell, dashboard and performance
 * bundles: `@vitals/i18n` is a separate package, and the rule that no
 * user-facing string is inline in a component matters more than which file it
 * sits in. English and Romanian side by side so a key cannot be added to one
 * and forgotten in the other.
 */

import { i18n } from '@vitals/i18n';

export const CONNECTIONS_NS = 'connections';

const en = {
  title: 'Network connections',
  subtitle: 'Every socket on this machine, grouped by the program that owns it.',

  search: 'Filter by app, address, or port',
  clear: 'Clear the filter',
  filterLabel: 'Show connections',
  refresh: 'Refresh',
  updated: 'Updated {{seconds}}s ago',
  updatedJustNow: 'Updated just now',

  filter: {
    all: 'All',
    established: 'Active',
    listening: 'Listening',
    external: 'External',
  },

  column: {
    app: 'Application',
    protocol: 'Protocol',
    local: 'Local address',
    remote: 'Remote address',
    state: 'State',
    pid: 'PID',
    // Export-only: the table joins address and port in one cell, a
    // spreadsheet wants them apart so the port column can be filtered.
    localPort: 'Local port',
    remotePort: 'Remote port',
  },

  summary: {
    connections_one: '{{count}} connection',
    connections_other: '{{count}} connections',
    established: '{{count}} active',
    listening: '{{count}} listening',
    hosts_one: '{{count}} host',
    hosts_other: '{{count}} hosts',
    external: '{{count}} external',
    publicListener: 'Reachable from the network',
    publicListenerHint:
      'This program accepts connections on every network interface, not just from this computer.',
  },

  state: {
    established: 'Active',
    listen: 'Listening',
    synSent: 'Connecting',
    synReceived: 'Connecting',
    finWait1: 'Closing',
    finWait2: 'Closing',
    timeWait: 'Closing',
    closed: 'Closed',
    closeWait: 'Closing',
    lastAck: 'Closing',
    closing: 'Closing',
    deleteTcb: 'Closing',
    stateless: '—',
  },

  unknownApp: 'Unknown program',
  unknownAppHint:
    'The process closed between reading the socket table and reading the process list.',

  menu: {
    expand: 'Show connections',
    collapse: 'Hide connections',
    copyName: 'Copy program name',
    copyPids: 'Copy process IDs',
    copyRemotes: 'Copy remote addresses',
    copyRemote: 'Copy remote address',
    copyLocal: 'Copy local address',
    copyProcess: 'Copy program name and PID',
  },

  empty: {
    title: 'No connection matches',
    body: 'Clear the search or choose a different filter.',
  },
  noHost: {
    title: 'No readings are arriving',
    body: 'Vitals cannot reach the part of itself that reads the network tables. Nothing is wrong with the machine — restarting Vitals usually fixes this.',
  },
  stale: 'Showing the last successful reading. {{message}}',
} as const;

const ro = {
  title: 'Conexiuni de rețea',
  subtitle: 'Fiecare socket de pe acest calculator, grupat după programul care îl deține.',

  search: 'Filtrează după aplicație, adresă sau port',
  clear: 'Șterge filtrul',
  filterLabel: 'Arată conexiunile',
  refresh: 'Reîmprospătează',
  updated: 'Actualizat acum {{seconds}}s',
  updatedJustNow: 'Actualizat chiar acum',

  filter: {
    all: 'Toate',
    established: 'Active',
    listening: 'În ascultare',
    external: 'Externe',
  },

  column: {
    app: 'Aplicație',
    protocol: 'Protocol',
    local: 'Adresă locală',
    remote: 'Adresă la distanță',
    state: 'Stare',
    pid: 'PID',
    localPort: 'Port local',
    remotePort: 'Port la distanță',
  },

  summary: {
    connections_one: '{{count}} conexiune',
    connections_few: '{{count}} conexiuni',
    connections_other: '{{count}} de conexiuni',
    established: '{{count}} active',
    listening: '{{count}} în ascultare',
    hosts_one: '{{count}} gazdă',
    hosts_few: '{{count}} gazde',
    hosts_other: '{{count}} de gazde',
    external: '{{count}} externe',
    publicListener: 'Accesibil din rețea',
    publicListenerHint:
      'Acest program acceptă conexiuni pe toate interfețele de rețea, nu doar de pe acest calculator.',
  },

  state: {
    established: 'Activă',
    listen: 'În ascultare',
    synSent: 'Se conectează',
    synReceived: 'Se conectează',
    finWait1: 'Se închide',
    finWait2: 'Se închide',
    timeWait: 'Se închide',
    closed: 'Închisă',
    closeWait: 'Se închide',
    lastAck: 'Se închide',
    closing: 'Se închide',
    deleteTcb: 'Se închide',
    stateless: '—',
  },

  unknownApp: 'Program necunoscut',
  unknownAppHint:
    'Procesul s-a închis între citirea tabelei de socketuri și citirea listei de procese.',

  menu: {
    expand: 'Arată conexiunile',
    collapse: 'Ascunde conexiunile',
    copyName: 'Copiază numele programului',
    copyPids: 'Copiază ID-urile proceselor',
    copyRemotes: 'Copiază adresele la distanță',
    copyRemote: 'Copiază adresa la distanță',
    copyLocal: 'Copiază adresa locală',
    copyProcess: 'Copiază numele programului și PID-ul',
  },

  empty: {
    title: 'Nicio conexiune nu se potrivește',
    body: 'Șterge căutarea sau alege un alt filtru.',
  },
  noHost: {
    title: 'Nu sosesc măsurători',
    body: 'Vitals nu poate ajunge la partea din el care citește tabelele de rețea. Nu este nimic în neregulă cu mașina — de obicei repornirea aplicației Vitals rezolvă asta.',
  },
  stale: 'Se afișează ultima citire reușită. {{message}}',
} as const;

/**
 * Registers the bundle.
 *
 * `deep: true, overwrite: false` — both flags matter. With `deep: false`
 * i18next replaces the bundle wholesale and `overwrite: false` protects
 * nothing, so these placeholders would shadow the real translations when the
 * keys eventually move into `@vitals/i18n`.
 *
 * Must run after `initI18n`: `addResourceBundle` does not exist before
 * `init()`, and that throw at module scope is what once left the app on its
 * splash screen forever.
 */
export function registerConnectionStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerConnectionStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', CONNECTIONS_NS, en, true, false);
  i18n.addResourceBundle('ro', CONNECTIONS_NS, ro, true, false);
}

/** Exported for the parity test. */
export const bundles = { en, ro } as const;
