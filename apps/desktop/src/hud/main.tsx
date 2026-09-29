import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { i18n, initI18n, isSupportedLocale, type Locale } from '@vitals/i18n';

import { installGlobalErrorLogging, reportToLog } from '../lib/logToFile';
import { HudApp } from './HudApp';
import { applyOverlayTheme } from './lib/theme';
import './styles.css';

/**
 * Where the main window leaves the chosen language.
 *
 * The overlay cannot read the settings store — its capability grants no store
 * permission, deliberately — so the language is mirrored into `localStorage`,
 * which both windows share. A missing or unrecognised value simply falls back
 * to the default rather than failing the boot: an overlay in the wrong
 * language is a nuisance, one that never appears is a bug report.
 */
const LOCALE_KEY = 'vitals.locale.v1';

/** Reads the mirrored locale. Exported for the render test. */
export function storedLocale(storage: Storage): Locale | undefined {
  try {
    const value = storage.getItem(LOCALE_KEY);
    return value !== null && isSupportedLocale(value) ? value : undefined;
  } catch {
    // Private-mode storage throws on read. Not worth a failed boot.
    return undefined;
  }
}

async function bootstrap(): Promise<void> {
  const container = document.getElementById('root');
  if (!container) throw new Error('#root is missing from hud.html');

  applyOverlayTheme(document.documentElement);
  // Awaited before the first render: an overlay that flashes English for one
  // frame is more noticeable than a page, because it sits on top of
  // everything and the eye is drawn to the change.
  await initI18n(storedLocale(window.localStorage), { reportMissingKeys: import.meta.env.DEV });

  createRoot(container).render(
    <StrictMode>
      <HudApp locale={i18n.language} />
    </StrictMode>,
  );
}

// The overlay has no devtools at all, even in development; without this its
// errors went nowhere.
installGlobalErrorLogging('hud');
void bootstrap().catch((error: unknown) => {
  // Plain DOM, not React: using React to report React failing to start does
  // not work, and this window has no other error surface.
  console.error('Vitals overlay failed to start', error);
  reportToLog('error', error, 'hud:startup');
  const root = document.getElementById('root');
  if (root) root.textContent = error instanceof Error ? error.message : String(error);
});
