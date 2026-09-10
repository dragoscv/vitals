/**
 * Shell-specific translations.
 *
 * These belong in `@vitals/i18n` alongside `nav.*` and `settings.*`, but that
 * package is owned elsewhere and cannot be edited from here. Registering them
 * as a separate `shell` namespace keeps the rule that no user-facing string is
 * ever written inline in a component, and keeps English and Romanian defined
 * side by side so they cannot drift.
 *
 * Migration is a move, not a rewrite: drop `en`/`ro` below into the matching
 * files in `packages/i18n/src/locales`, delete this module, and change the
 * `useTranslation('shell')` calls to plain `useTranslation()`.
 */

import { i18n } from '@vitals/i18n';

export const SHELL_NS = 'shell';

const en = {
  window: {
    minimise: 'Minimise',
    maximise: 'Maximise',
    restore: 'Restore down',
    close: 'Close',
  },
  sidebar: {
    collapse: 'Collapse sidebar',
    expand: 'Expand sidebar',
    openSettings: 'Open settings',
  },
  settings: {
    general: {
      title: 'General',
      startWithWindows: 'Start with Windows',
      startWithWindowsHint: 'Vitals opens in the background when you sign in.',
      startMinimised: 'Start minimised to the tray',
      closeToTray: 'Close to tray',
      closeToTrayHint: 'The × button hides Vitals; it keeps monitoring. Quit from the tray menu.',
      hudVisible: 'Show the overlay',
      hudVisibleHint:
        'A small always-on-top panel with CPU, memory and GPU. Ctrl+Shift+H shows or hides it.',
      confirmEndTask: 'Ask before ending a task',
      confirmEndTaskHint: 'Critical system processes always ask, whatever this is set to.',
      quit: 'Quit Vitals',
      quitHint: 'Stops monitoring and closes the app completely.',
    },
    notifications: {
      title: 'Notifications',
      enable: 'Show notifications',
      enableHint: 'Vitals only notifies you about things you asked it to watch.',
      highCpu: 'Sustained high CPU',
      highMemory: 'Memory running low',
      thermal: 'Overheating',
    },
    sampling: {
      rateFast: 'Fast — every half second',
      rateNormal: 'Normal — every second',
      rateSlow: 'Relaxed — every two seconds',
    },
    history: {
      retentionDays_one: '{{count}} day',
      retentionDays_other: '{{count}} days',
      diskUsageHint: 'Recorded on this computer only. Deleting it is immediate.',
      noData: 'Nothing recorded',
      clearHint: 'Deletes both the per-app totals and the recorded charts.',
    },
    remote: {
      title: 'Remote access',
      explainer:
        'Watch this computer from your phone, or let another program read its metrics. Everything stays on your own network — nothing is sent to the internet.',
      enable: 'Allow devices on this network',
      enableHint:
        'Off by default. Windows may ask you to allow Vitals through the firewall the first time.',
      status: 'Server',
      listening: 'Listening on port {{port}}',
      pairTitle: 'Pair a device',
      interface: 'Network adapter',
      interfaceHint:
        'This computer has more than one address. Pick the network your phone is on — virtual adapters from WSL, Hyper-V or Docker cannot be reached from it.',
      noInterfaces: 'No reachable network address',
      deviceName: 'Device name',
      deviceNamePlaceholder: 'My phone',
      unnamedDevice: 'Unnamed device',
      allowControl: 'Allow this device to end processes',
      allowControlHint:
        'Off by default. A paired phone with this on can end, suspend and reprioritise anything you could.',
      createPairing: 'Pairing code',
      showQr: 'Create',
      scanHint: 'Scan this with your phone camera.',
      qrAlt: 'Pairing QR code',
      onceOnly: 'Shown once. Create a new one if you lose it.',
      dismiss: 'Done',
      pairedTitle: 'Paired devices',
      scopeRead: 'can watch',
      scopeControl: 'can watch and control',
      revoke: 'Revoke',
      revokeAll: 'Revoke all',
    },
    flight: {
      title: 'Flight recorder',
      export: 'Save a recording',
      hint: 'The last two minutes of everything Vitals measured, plus what this computer is. Attach it to a bug report so the problem can be seen rather than described.',
      save: 'Save\u2026',
      exporting: 'Saving\u2026',
    },
  },
  // Rendered by Rust, not React: the tray must work with the window hidden,
  // so these are pushed to the backend rather than read by a component.
  tray: {
    show: 'Show Vitals',
    pause: 'Pause sampling',
    quit: 'Quit',
    cpu: 'CPU',
    memory: 'Memory',
    gpu: 'GPU',
    stillRunning: 'Vitals is still running in the tray.',
  },
  placeholder: {
    title: '{{section}} is not ready yet',
    body: 'This section is still being built. Nothing here is missing from your computer — only from Vitals.',
  },
  routeError: {
    title: 'This section stopped working',
    retry: 'Try again',
  },
  sampler: {
    errorTitle: 'A reading failed',
    errorBody: 'Vitals kept the last good numbers. {{detail}}',
  },
} as const;

/** English half of the additions for the palette and the shortcut sheet. */
const enShortcuts = {
  palette: {
    title: 'Search commands',
    placeholder: 'Type a section or an action\u2026',
    close: 'Close the command palette',
    noResults: 'Nothing matches that.',
    goTo: 'Go to this section',
    openSettings: 'Open settings',
    openSettingsHint: 'Appearance, notifications, remote access.',
    diagnose: 'Why is my PC slow?',
    diagnoseHint: 'Looks at what is using this computer right now.',
    toggleHud: 'Toggle the overlay',
    toggleHudHint: 'The small always-on-top panel.',
  },
  shortcuts: {
    title: 'Keyboard shortcuts',
    subtitle: 'These work whenever the Vitals window has focus and you are not typing in a box.',
    close: 'Close the shortcut list',
    palette: 'Search commands',
    help: 'Show this list',
    settings: 'Open settings',
    sections: 'Jump to the first nine sections',
    hud: 'Show or hide the overlay',
  },
} as const;

const ro = {
  window: {
    minimise: 'Minimizează',
    maximise: 'Maximizează',
    restore: 'Restaurează',
    close: 'Închide',
  },
  sidebar: {
    collapse: 'Restrânge bara laterală',
    expand: 'Extinde bara laterală',
    openSettings: 'Deschide setările',
  },
  settings: {
    general: {
      title: 'General',
      startWithWindows: 'Pornește odată cu Windows',
      startWithWindowsHint: 'Vitals pornește în fundal când te autentifici.',
      startMinimised: 'Pornește minimizat în bara de sistem',
      closeToTray: 'Închide în bara de sistem',
      closeToTrayHint:
        'Butonul × ascunde Vitals; monitorizarea continuă. Închide-l din meniul barei de sistem.',
      hudVisible: 'Afișează suprapunerea',
      hudVisibleHint:
        'Un panou mic, mereu deasupra, cu procesor, memorie și placă video. Ctrl+Shift+H îl afișează sau îl ascunde.',
      confirmEndTask: 'Cere confirmare înainte de a opri un proces',
      confirmEndTaskHint:
        'Procesele critice de sistem cer întotdeauna confirmare, indiferent de această setare.',
      quit: 'Închide Vitals',
      quitHint: 'Oprește monitorizarea și închide aplicația complet.',
    },
    notifications: {
      title: 'Notificări',
      enable: 'Afișează notificări',
      enableHint: 'Vitals te anunță doar despre lucrurile pe care i-ai cerut să le urmărească.',
      highCpu: 'Procesor solicitat constant',
      highMemory: 'Memorie pe terminate',
      thermal: 'Supraîncălzire',
    },
    sampling: {
      rateFast: 'Rapid — la o jumătate de secundă',
      rateNormal: 'Normal — la o secundă',
      rateSlow: 'Relaxat — la două secunde',
    },
    history: {
      retentionDays_one: '{{count}} zi',
      retentionDays_few: '{{count}} zile',
      retentionDays_other: '{{count}} de zile',
      diskUsageHint: 'Înregistrat doar pe acest calculator. Ștergerea este imediată.',
      noData: 'Nimic înregistrat',
      clearHint: 'Șterge atât totalurile per aplicație, cât și graficele înregistrate.',
    },
    remote: {
      title: 'Acces de la distanță',
      explainer:
        'Vezi acest calculator de pe telefon sau lasă alt program să‑i citească datele. Totul rămâne în rețeaua ta — nimic nu pleacă pe internet.',
      enable: 'Permite dispozitivelor din această rețea',
      enableHint:
        'Dezactivat implicit. Windows îți poate cere să permiți Vitals prin firewall prima dată.',
      status: 'Server',
      listening: 'Ascultă pe portul {{port}}',
      pairTitle: 'Asociază un dispozitiv',
      interface: 'Adaptor de rețea',
      interfaceHint:
        'Acest calculator are mai multe adrese. Alege rețeaua în care este telefonul — adaptoarele virtuale WSL, Hyper‑V sau Docker nu pot fi accesate de pe el.',
      noInterfaces: 'Nicio adresă de rețea accesibilă',
      deviceName: 'Numele dispozitivului',
      deviceNamePlaceholder: 'Telefonul meu',
      unnamedDevice: 'Dispozitiv fără nume',
      allowControl: 'Permite acestui dispozitiv să oprească procese',
      allowControlHint:
        'Dezactivat implicit. Un telefon asociat cu această opțiune poate opri, suspenda și reprioritiza orice ai putea tu.',
      createPairing: 'Cod de asociere',
      showQr: 'Creează',
      scanHint: 'Scanează‑l cu camera telefonului.',
      qrAlt: 'Cod QR de asociere',
      onceOnly: 'Afișat o singură dată. Creează altul dacă îl pierzi.',
      dismiss: 'Gata',
      pairedTitle: 'Dispozitive asociate',
      scopeRead: 'poate urmări',
      scopeControl: 'poate urmări și controla',
      revoke: 'Revocă',
      revokeAll: 'Revocă tot',
    },
    flight: {
      title: 'Înregistrare de diagnostic',
      export: 'Salvează o înregistrare',
      hint: 'Ultimele două minute din tot ce a măsurat Vitals, plus ce este acest calculator. Atașeaz-o unui raport de problemă ca să poată fi văzută, nu doar descrisă.',
      save: 'Salvează\u2026',
      exporting: 'Se salvează\u2026',
    },
  },
  tray: {
    show: 'Afișează Vitals',
    pause: 'Suspendă măsurarea',
    quit: 'Închide',
    cpu: 'CPU',
    memory: 'Memorie',
    gpu: 'GPU',
    stillRunning: 'Vitals rulează în continuare în bara de sistem.',
  },
  placeholder: {
    title: '{{section}} nu este gata încă',
    body: 'Această secțiune este încă în lucru. Nu lipsește nimic din calculatorul tău — doar din Vitals.',
  },
  sampler: {
    errorTitle: 'O măsurătoare a eșuat',
    errorBody: 'Vitals a păstrat ultimele valori bune. {{detail}}',
  },
  routeError: {
    title: 'Această secțiune a încetat să funcționeze',
    retry: 'Încearcă din nou',
  },
} as const;

/** Romanian half. Kept beside the English so the two cannot drift. */
const roShortcuts = {
  palette: {
    title: 'Caută comenzi',
    placeholder: 'Scrie o secțiune sau o acțiune\u2026',
    close: 'Închide paleta de comenzi',
    noResults: 'Nimic nu se potrivește.',
    goTo: 'Mergi la această secțiune',
    openSettings: 'Deschide setările',
    openSettingsHint: 'Aspect, notificări, acces de la distanță.',
    diagnose: 'De ce merge greu calculatorul?',
    diagnoseHint: 'Se uită la ce folosește acest calculator chiar acum.',
    toggleHud: 'Comută suprapunerea',
    toggleHudHint: 'Panoul mic, mereu deasupra.',
  },
  shortcuts: {
    title: 'Scurtături de tastatură',
    subtitle:
      'Funcționează atunci când fereastra Vitals este activă și nu scrii într-un câmp de text.',
    close: 'Închide lista de scurtături',
    palette: 'Caută comenzi',
    help: 'Afișează această listă',
    settings: 'Deschide setările',
    sections: 'Sari la primele nouă secțiuni',
    hud: 'Afișează sau ascunde suprapunerea',
  },
} as const;

/**
 * Registers the bundle.
 *
 * `deep: true, overwrite: false` so that once these keys move into
 * `@vitals/i18n` the real translations win and this module becomes inert
 * rather than silently shadowing them.
 *
 * Both flags matter and the pairing is not obvious: with `deep: false`
 * i18next shallow-merges, replacing the existing bundle wholesale, so
 * `overwrite: false` protects nothing at all. This shipped as `false, false`
 * — which would have quietly shadowed the real translations at migration —
 * until a test on the dashboard's identical registration caught it.
 *
 * # Panics
 *
 * Must be called **after** `initI18n`. i18next does not define
 * `addResourceBundle` until `init()` has run, so calling this early throws
 * `i18n.addResourceBundle is not a function` — and if that happens at module
 * scope it aborts the whole module graph before React mounts, leaving the
 * app on its splash screen forever. That shipped once; hence the guard.
 */
export function registerShellStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerShellStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', SHELL_NS, { ...en, ...enShortcuts }, true, false);
  i18n.addResourceBundle('ro', SHELL_NS, { ...ro, ...roShortcuts }, true, false);
}
