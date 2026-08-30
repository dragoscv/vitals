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
      confirmEndTask: 'Ask before ending a task',
      confirmEndTaskHint: 'Critical system processes always ask, whatever this is set to.',
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
    },
  },
  placeholder: {
    title: '{{section}} is not ready yet',
    body: 'This section is still being built. Nothing here is missing from your computer — only from Vitals.',
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
      confirmEndTask: 'Cere confirmare înainte de a opri un proces',
      confirmEndTaskHint:
        'Procesele critice de sistem cer întotdeauna confirmare, indiferent de această setare.',
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
    },
  },
  placeholder: {
    title: '{{section}} nu este gata încă',
    body: 'Această secțiune este încă în lucru. Nu lipsește nimic din calculatorul tău — doar din Vitals.',
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

  i18n.addResourceBundle('en', SHELL_NS, en, true, false);
  i18n.addResourceBundle('ro', SHELL_NS, ro, true, false);
}
