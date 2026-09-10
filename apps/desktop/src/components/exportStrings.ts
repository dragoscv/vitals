/**
 * Export button translations.
 *
 * Own namespace, same shape as every feature bundle: English and Romanian side
 * by side so a key cannot be added to one and forgotten in the other. Lives
 * beside the component rather than in a feature folder because every table
 * screen shares it.
 */

import { i18n } from '@vitals/i18n';

export const EXPORT_NS = 'export';

const en = {
  button: 'Export',
  menuLabel: 'Export {{count}} rows',
  csv: 'Spreadsheet (CSV)',
  json: 'JSON',
  csvHint: 'Opens in Excel. Numbers are exported unformatted.',
  jsonHint: 'For scripts and bug reports.',
  nothing: 'Nothing to export',
  nothingHint: 'The table is empty, so there is nothing to save.',
} as const;

const ro = {
  button: 'Exportă',
  menuLabel: 'Exportă {{count}} rânduri',
  csv: 'Foaie de calcul (CSV)',
  json: 'JSON',
  csvHint: 'Se deschide în Excel. Numerele sunt exportate neformatate.',
  jsonHint: 'Pentru scripturi și rapoarte de erori.',
  nothing: 'Nimic de exportat',
  nothingHint: 'Tabelul este gol, așa că nu există nimic de salvat.',
} as const;

/**
 * Registers the bundle. Idempotent: `overwrite: false` means a second call
 * from a second screen is a no-op rather than a warning.
 *
 * Must run after `initI18n` — see any feature `strings.ts` for the reason.
 */
export function registerExportStrings(): void {
  if (!i18n.isInitialized) {
    throw new Error(
      'registerExportStrings() was called before initI18n(). i18next only ' +
        'defines addResourceBundle after init, so this must run after the ' +
        'await in bootstrap().',
    );
  }

  i18n.addResourceBundle('en', EXPORT_NS, en, true, false);
  i18n.addResourceBundle('ro', EXPORT_NS, ro, true, false);
}

/** Exported for the parity test. */
export const bundles = { en, ro } as const;
