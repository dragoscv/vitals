/**
 * Startup and services translations.
 *
 * Own namespace, same reasoning as the other feature bundles. English and
 * Romanian side by side so a key cannot be added to one and forgotten in the
 * other.
 */

import { i18n } from '@vitals/i18n';

export const STARTUP_NS = 'startup';

const en = {
  startup: {
    title: 'Startup apps',
    subtitle: 'Everything that runs when you sign in.',
    search: 'Filter startup items',
    clear: 'Clear the filter',
    filterLabel: 'Show startup items',
  },
  services: {
    title: 'Services',
    subtitle: 'Every Windows service, whatever its start type.',
    search: 'Filter services',
    clear: 'Clear the filter',
    filterLabel: 'Show services',
  },

  refresh: 'Refresh',
  refreshing: 'Refreshing…',

  filter: {
    all: 'All',
    enabled: 'Enabled',
    disabled: 'Disabled',
    running: 'Running',
    stopped: 'Stopped',
    automatic: 'Starts at boot',
  },

  counts: {
    startup: '{{enabled}} of {{total}} will run at sign-in',
    services: '{{running}} of {{total}} running',
    automatic: '{{count}} start at boot',
    unknownState_one: '{{count}} whose state could not be read',
    unknownState_other: '{{count}} whose state could not be read',
    unknownStartType_one: '{{count}} whose start type could not be read',
    unknownStartType_other: '{{count}} whose start type could not be read',
    unreadableTasks_one: '{{count}} scheduled task could not be read',
    unreadableTasks_other: '{{count}} scheduled tasks could not be read',
    undercountHint:
      'Reading these needs administrator rights. The figures above are a floor, not a total — Vitals would rather undercount than guess.',
  },

  source: {
    machineRun: 'Registry (all users)',
    machineRun32: 'Registry (all users, 32-bit)',
    machineRunOnce: 'Registry, run once (all users)',
    machineRunOnce32: 'Registry, run once (all users, 32-bit)',
    userRun: 'Registry (your account)',
    userRunOnce: 'Registry, run once (your account)',
    commonStartupFolder: 'Startup folder (all users)',
    userStartupFolder: 'Startup folder (your account)',
    scheduledTask: 'Scheduled task',
    service: 'Service',
  },

  state: {
    enabled: 'Enabled',
    disabled: 'Disabled',
    unknown: 'Could not be read',
  },

  serviceState: {
    running: 'Running',
    stopped: 'Stopped',
    startPending: 'Starting',
    stopPending: 'Stopping',
    continuePending: 'Resuming',
    pausePending: 'Pausing',
    paused: 'Paused',
    unknown: 'Unrecognised state',
  },

  startType: {
    boot: 'Boot',
    system: 'System',
    automatic: 'Automatic',
    manual: 'Manual',
    disabled: 'Disabled',
    unknown: 'Could not be read',
  },

  column: {
    name: 'Name',
    publisher: 'Publisher',
    source: 'Source',
    state: 'Status',
    startType: 'Start type',
    command: 'Command',
    impact: 'Startup cost',
    impactCpu: 'CPU at startup',
    impactDisk: 'Disk at startup',
    // Export-only columns; the table does not draw them.
    pid: 'PID',
    path: 'Path',
    serviceName: 'Service name',
    sharedGroup: 'Shared process group',
  },

  impact: {
    hint: 'Processor time and disk traffic this item used in the first two minutes after Windows started, measured by Vitals.',
    measured: 'Startup cost measured over the first 2 minutes after boot on {{date}}.',
    unmeasured:
      'Startup cost has not been measured yet. Vitals needs to be running during the first two minutes after Windows starts; the next boot with Vitals in your startup items will fill this in.',
    notSeen: 'Not seen running during the measured window',
  },

  allUsers: 'All users',
  allUsersHint: 'Changing this affects everyone who signs in, and needs administrator rights.',
  shared: 'Shares a process',
  sharedHint:
    'This service runs inside a shared host with others, so its CPU and memory cannot be measured separately.',

  empty: {
    title: 'Nothing matches',
    body: 'Clear the search or choose a different filter.',
  },
  noHost: {
    title: 'No readings are arriving',
    body: 'Vitals cannot reach the part of itself that reads startup entries. Nothing is wrong with the machine — restarting Vitals usually fixes this.',
  },
  stale: 'Showing the last successful reading. {{message}}',

  microsoft: {
    hideStartup: 'Hide Microsoft entries',
    hideServices: 'Hide Microsoft services',
    hidden_one: '{{count}} Microsoft item hidden',
    hidden_other: '{{count}} Microsoft items hidden',
  },

  action: {
    actions: 'Actions',
    rowActions: 'Actions for {{name}}',
    enable: 'Enable',
    disable: 'Disable',
    start: 'Start',
    stop: 'Stop',
    restart: 'Restart',
    startType: 'Start type',
    automatic: 'Automatic',
    manual: 'Manual',
    disabled: 'Disabled',
    openLocation: 'Open file location',
    properties: 'Properties',
    searchOnline: 'Search online',
    copyDetails: 'Copy details',
  },

  reason: {
    noPath: 'Vitals could not find the file this runs',
    runOnce: 'Run-once entries delete themselves after running',
    forbidden: 'Windows does not allow this to be changed',
    elevation: 'Windows will ask for administrator permission',
  },

  confirm: {
    title: {
      disable: 'Disable {{name}}?',
      stop: 'Stop {{name}}?',
      restart: 'Restart {{name}}?',
      manual: 'Set {{name}} to start manually?',
      disabled: 'Set {{name}} to Disabled?',
    },
    body: {
      degrades: 'Something that relies on it may stop working until you turn it back on.',
      systemCritical:
        'Windows depends on this. Turning it off can break sign-in, networking or the desktop.',
    },
    confirm: 'Continue',
    cancel: 'Cancel',
    close: 'Close this dialog',
  },

  done: {
    enable: 'Enabled {{name}}',
    disable: 'Disabled {{name}}',
    start: 'Started {{name}}',
    stop: 'Stopped {{name}}',
    restart: 'Restarted {{name}}',
    automatic: '{{name}} now starts automatically',
    manual: '{{name}} now starts only when needed',
    disabled: '{{name}} is now disabled',
  },

  failed: 'That did not work. {{message}}',
} as const;

const ro = {
  startup: {
    title: 'Aplicații la pornire',
    subtitle: 'Tot ce rulează când te autentifici.',
    search: 'Filtrează elementele de pornire',
    clear: 'Șterge filtrul',
    filterLabel: 'Arată elementele de pornire',
  },
  services: {
    title: 'Servicii',
    subtitle: 'Fiecare serviciu Windows, indiferent de tipul de pornire.',
    search: 'Filtrează serviciile',
    clear: 'Șterge filtrul',
    filterLabel: 'Arată serviciile',
  },

  refresh: 'Reîmprospătează',
  refreshing: 'Se reîmprospătează…',

  filter: {
    all: 'Toate',
    enabled: 'Activate',
    disabled: 'Dezactivate',
    running: 'Pornite',
    stopped: 'Oprite',
    automatic: 'Pornesc la boot',
  },

  counts: {
    startup: '{{enabled}} din {{total}} vor rula la autentificare',
    services: '{{running}} din {{total}} pornite',
    automatic: '{{count}} pornesc la boot',
    unknownState_one: '{{count}} a cărui stare nu a putut fi citită',
    unknownState_few: '{{count}} a căror stare nu a putut fi citită',
    unknownState_other: '{{count}} a căror stare nu a putut fi citită',
    unknownStartType_one: '{{count}} al cărui tip de pornire nu a putut fi citit',
    unknownStartType_few: '{{count}} ale căror tipuri de pornire nu au putut fi citite',
    unknownStartType_other: '{{count}} ale căror tipuri de pornire nu au putut fi citite',
    unreadableTasks_one: '{{count}} activitate programată nu a putut fi citită',
    unreadableTasks_few: '{{count}} activități programate nu au putut fi citite',
    unreadableTasks_other: '{{count}} de activități programate nu au putut fi citite',
    undercountHint:
      'Citirea acestora necesită drepturi de administrator. Cifrele de mai sus sunt un minim, nu un total — Vitals preferă să numere în minus decât să ghicească.',
  },

  source: {
    machineRun: 'Registry (toți utilizatorii)',
    machineRun32: 'Registry (toți utilizatorii, 32 de biți)',
    machineRunOnce: 'Registry, o singură dată (toți utilizatorii)',
    machineRunOnce32: 'Registry, o singură dată (toți utilizatorii, 32 de biți)',
    userRun: 'Registry (contul tău)',
    userRunOnce: 'Registry, o singură dată (contul tău)',
    commonStartupFolder: 'Folder Startup (toți utilizatorii)',
    userStartupFolder: 'Folder Startup (contul tău)',
    scheduledTask: 'Activitate programată',
    service: 'Serviciu',
  },

  state: {
    enabled: 'Activat',
    disabled: 'Dezactivat',
    unknown: 'Nu a putut fi citit',
  },

  serviceState: {
    running: 'Pornit',
    stopped: 'Oprit',
    startPending: 'Pornește',
    stopPending: 'Se oprește',
    continuePending: 'Se reia',
    pausePending: 'Se suspendă',
    paused: 'Suspendat',
    unknown: 'Stare nerecunoscută',
  },

  startType: {
    boot: 'La boot',
    system: 'Sistem',
    automatic: 'Automat',
    manual: 'Manual',
    disabled: 'Dezactivat',
    unknown: 'Nu a putut fi citit',
  },

  column: {
    name: 'Nume',
    publisher: 'Editor',
    source: 'Sursă',
    state: 'Stare',
    startType: 'Tip de pornire',
    command: 'Comandă',
    impact: 'Cost la pornire',
    impactCpu: 'Procesor la pornire',
    impactDisk: 'Disc la pornire',
    pid: 'PID',
    path: 'Cale',
    serviceName: 'Numele serviciului',
    sharedGroup: 'Grup de proces partajat',
  },

  impact: {
    hint: 'Timpul de procesor și traficul pe disc folosite de acest element în primele două minute după pornirea Windows, măsurate de Vitals.',
    measured: 'Costul la pornire a fost măsurat în primele 2 minute după boot, pe {{date}}.',
    unmeasured:
      'Costul la pornire nu a fost măsurat încă. Vitals trebuie să ruleze în primele două minute după pornirea Windows; următorul boot cu Vitals printre elementele de pornire va completa această coloană.',
    notSeen: 'Nu a fost văzut rulând în fereastra măsurată',
  },

  allUsers: 'Toți utilizatorii',
  allUsersHint:
    'Modificarea afectează pe oricine se autentifică și necesită drepturi de administrator.',
  shared: 'Partajează un proces',
  sharedHint:
    'Acest serviciu rulează într-o gazdă partajată cu altele, deci procesorul și memoria lui nu pot fi măsurate separat.',

  empty: {
    title: 'Nimic nu se potrivește',
    body: 'Șterge căutarea sau alege un alt filtru.',
  },
  noHost: {
    title: 'Nu sosesc măsurători',
    body: 'Vitals nu poate ajunge la partea din el care citește elementele de pornire. Nu este nimic în neregulă cu mașina — de obicei repornirea aplicației Vitals rezolvă asta.',
  },
  stale: 'Se afișează ultima citire reușită. {{message}}',

  microsoft: {
    hideStartup: 'Ascunde elementele Microsoft',
    hideServices: 'Ascunde serviciile Microsoft',
    hidden_one: '{{count}} element Microsoft ascuns',
    hidden_few: '{{count}} elemente Microsoft ascunse',
    hidden_other: '{{count}} de elemente Microsoft ascunse',
  },

  action: {
    actions: 'Acțiuni',
    rowActions: 'Acțiuni pentru {{name}}',
    enable: 'Activează',
    disable: 'Dezactivează',
    start: 'Pornește',
    stop: 'Oprește',
    restart: 'Repornește',
    startType: 'Tip de pornire',
    automatic: 'Automat',
    manual: 'Manual',
    disabled: 'Dezactivat',
    openLocation: 'Deschide locația fișierului',
    properties: 'Proprietăți',
    searchOnline: 'Caută online',
    copyDetails: 'Copiază detaliile',
  },

  reason: {
    noPath: 'Vitals nu a găsit fișierul pe care îl rulează',
    runOnce: 'Elementele care rulează o singură dată se șterg singure după rulare',
    forbidden: 'Windows nu permite modificarea acestui element',
    elevation: 'Windows va cere permisiunea de administrator',
  },

  confirm: {
    title: {
      disable: 'Dezactivezi {{name}}?',
      stop: 'Oprești {{name}}?',
      restart: 'Repornești {{name}}?',
      manual: 'Setezi {{name}} să pornească manual?',
      disabled: 'Setezi {{name}} ca dezactivat?',
    },
    body: {
      degrades: 'Ceva ce depinde de el poate să nu mai funcționeze până când îl activezi din nou.',
      systemCritical:
        'Windows depinde de acesta. Oprirea lui poate strica autentificarea, rețeaua sau desktopul.',
    },
    confirm: 'Continuă',
    cancel: 'Anulează',
    close: 'Închide acest dialog',
  },

  done: {
    enable: 'S-a activat {{name}}',
    disable: 'S-a dezactivat {{name}}',
    start: 'S-a pornit {{name}}',
    stop: 'S-a oprit {{name}}',
    restart: 'S-a repornit {{name}}',
    automatic: '{{name}} pornește acum automat',
    manual: '{{name}} pornește acum doar la nevoie',
    disabled: '{{name}} este acum dezactivat',
  },

  failed: 'Nu a funcționat. {{message}}',
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
export function registerStartupStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerStartupStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', STARTUP_NS, en, true, false);
  i18n.addResourceBundle('ro', STARTUP_NS, ro, true, false);
}

/** Exported for the parity test. */
export const bundles = { en, ro } as const;
