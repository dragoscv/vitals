/**
 * Installed applications translations.
 *
 * Own namespace, same reasoning as the other feature bundles. English and
 * Romanian side by side so a key cannot be added to one and forgotten in the
 * other.
 */

import { i18n } from '@vitals/i18n';

export const APPS_NS = 'apps';

const en = {
  title: 'Installed apps',
  subtitle: 'Everything installed on this computer, and what it declares it takes up.',

  search: 'Filter by name, publisher, or version',
  clear: 'Clear the filter',
  refresh: 'Refresh',
  sortLabel: 'Sort by',

  sort: {
    name: 'Name',
    size: 'Size',
    date: 'Installed',
    publisher: 'Publisher',
  },

  column: {
    name: 'Name',
    publisher: 'Publisher',
    version: 'Version',
    installed: 'Installed',
    size: 'Size',
  },

  summary: {
    count_one: '{{count}} application',
    count_other: '{{count}} applications',
    declared: '{{size}} declared across {{withSize}} of them',
    noSize_one: '{{count}} did not report a size',
    noSize_other: '{{count}} did not report a size',
    sizeHint:
      'Sizes come from what each installer wrote at install time. Windows never updates them, so a program that has since downloaded more data still reports its original figure. For real disk usage, use Storage.',
    filtered:
      '{{rejected}} of {{examined}} registry entries were not applications — {{breakdown}}.',
    duplicates_one: '{{count}} duplicate entry was merged',
    duplicates_other: '{{count}} duplicate entries were merged',
  },

  reject: {
    noDisplayName: '{{count}} with no name',
    systemComponent: '{{count}} Windows components',
    updateOrHotfix: '{{count}} updates and hotfixes',
    childOfAnotherEntry: '{{count}} parts of other products',
    orphanPatch: '{{count}} orphaned patches',
  },

  source: {
    machineNative: 'All users',
    machineWow64: 'All users (32-bit)',
    userNative: 'Your account',
    userWow64: 'Your account (32-bit)',
  },

  perUser: 'Installed for you only',
  msi: 'Windows Installer',
  unknownSize: 'Not reported',
  unknownDate: 'Unknown',

  uninstall: {
    action: 'Uninstall',
    unavailable: 'This program did not publish an uninstaller.',
    title: 'Uninstall {{name}}?',
    body: "Vitals will open this program's own uninstaller. Nothing is removed until you confirm there.",
    bodyDetail:
      'Vitals never deletes files itself — guessing which files belong to a program is how data gets lost.',
    confirm: 'Open uninstaller',
    cancel: 'Cancel',
    close: 'Close this dialog',
    started: 'The uninstaller for {{name}} is opening.',
    failed: 'Could not start the uninstaller. {{message}}',
  },

  empty: {
    title: 'Nothing matches',
    body: 'Clear the search to see every application.',
  },
  noHost: {
    title: 'No readings are arriving',
    body: 'Vitals cannot reach the part of itself that reads the installed program list. Nothing is wrong with the machine — restarting Vitals usually fixes this.',
  },
  stale: 'Showing the last successful reading. {{message}}',
} as const;

const ro = {
  title: 'Aplicații instalate',
  subtitle: 'Tot ce este instalat pe acest calculator și cât declară că ocupă.',

  search: 'Filtrează după nume, editor sau versiune',
  clear: 'Șterge filtrul',
  refresh: 'Reîmprospătează',
  sortLabel: 'Sortează după',

  sort: {
    name: 'Nume',
    size: 'Dimensiune',
    date: 'Instalat',
    publisher: 'Editor',
  },

  column: {
    name: 'Nume',
    publisher: 'Editor',
    version: 'Versiune',
    installed: 'Instalat',
    size: 'Dimensiune',
  },

  summary: {
    count_one: '{{count}} aplicație',
    count_few: '{{count}} aplicații',
    count_other: '{{count}} de aplicații',
    declared: '{{size}} declarați pentru {{withSize}} dintre ele',
    noSize_one: '{{count}} nu a raportat o dimensiune',
    noSize_few: '{{count}} nu au raportat o dimensiune',
    noSize_other: '{{count}} nu au raportat o dimensiune',
    sizeHint:
      'Dimensiunile provin din ce a scris fiecare installer la instalare. Windows nu le actualizează niciodată, așa că un program care a descărcat între timp mai multe date raportează tot cifra inițială. Pentru spațiul real folosit, mergi la Stocare.',
    filtered:
      '{{rejected}} din {{examined}} intrări din registry nu erau aplicații — {{breakdown}}.',
    duplicates_one: '{{count}} intrare duplicat a fost unificată',
    duplicates_few: '{{count}} intrări duplicat au fost unificate',
    duplicates_other: '{{count}} de intrări duplicat au fost unificate',
  },

  reject: {
    noDisplayName: '{{count}} fără nume',
    systemComponent: '{{count}} componente Windows',
    updateOrHotfix: '{{count}} actualizări și remedieri',
    childOfAnotherEntry: '{{count}} părți ale altor produse',
    orphanPatch: '{{count}} patch-uri orfane',
  },

  source: {
    machineNative: 'Toți utilizatorii',
    machineWow64: 'Toți utilizatorii (32 de biți)',
    userNative: 'Contul tău',
    userWow64: 'Contul tău (32 de biți)',
  },

  perUser: 'Instalat doar pentru tine',
  msi: 'Windows Installer',
  unknownSize: 'Neraportat',
  unknownDate: 'Necunoscut',

  uninstall: {
    action: 'Dezinstalează',
    unavailable: 'Acest program nu a publicat un program de dezinstalare.',
    title: 'Dezinstalezi {{name}}?',
    body: 'Vitals va deschide programul de dezinstalare al aplicației. Nu se șterge nimic până nu confirmi acolo.',
    bodyDetail:
      'Vitals nu șterge niciodată fișiere el însuși — a ghici ce fișiere aparțin unui program este exact modul în care se pierd date.',
    confirm: 'Deschide dezinstalarea',
    cancel: 'Anulează',
    close: 'Închide această fereastră',
    started: 'Programul de dezinstalare pentru {{name}} se deschide.',
    failed: 'Nu s-a putut porni dezinstalarea. {{message}}',
  },

  empty: {
    title: 'Nimic nu se potrivește',
    body: 'Șterge căutarea ca să vezi toate aplicațiile.',
  },
  noHost: {
    title: 'Nu sosesc măsurători',
    body: 'Vitals nu poate ajunge la partea din el care citește lista de programe instalate. Nu este nimic în neregulă cu mașina — de obicei repornirea aplicației Vitals rezolvă asta.',
  },
  stale: 'Se afișează ultima citire reușită. {{message}}',
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
export function registerAppsStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerAppsStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', APPS_NS, en, true, false);
  i18n.addResourceBundle('ro', APPS_NS, ro, true, false);
}

/** Exported for the parity test. */
export const bundles = { en, ro } as const;
