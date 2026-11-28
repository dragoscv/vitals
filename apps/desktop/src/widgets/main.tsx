import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { i18n, initI18n, isSupportedLocale, type Locale } from '@vitals/i18n';

import { installGlobalErrorLogging, reportToLog } from '../lib/logToFile';
import { ErrorBoundary } from './ErrorBoundary';
import { RootError } from './RootError';
import { WidgetApp } from './WidgetApp';
import { applyWidgetTheme } from './lib/theme';
import './styles.css';

/**
 * Where the main window leaves the chosen language.
 *
 * The same mirror the HUD reads: a widget window cannot reach the settings
 * store (its capability grants no store permission, deliberately), so the
 * language arrives through `localStorage`, which both windows share. A missing
 * or unrecognised value falls back to the default rather than failing the
 * boot — a widget in the wrong language is a nuisance, one that never appears
 * is a bug report.
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
  if (!container) throw new Error('#root is missing from widget.html');

  applyWidgetTheme(document.documentElement);
  // Awaited before the first render: a widget sits on top of the user's work
  // and a one-frame flash of the wrong language is the most noticeable thing a
  // translucent panel can do.
  await initI18n(storedLocale(window.localStorage), { reportMissingKeys: import.meta.env.DEV });

  createRoot(container).render(
    <StrictMode>
      {/* At the root: one throw anywhere in the widget unmounts the tree and
          leaves an empty always-on-top window on the user's screen that has no
          chrome to close. */}
      <ErrorBoundary fallback={(error, retry) => <RootError error={error} onRetry={retry} />}>
        <WidgetApp locale={i18n.language} />
      </ErrorBoundary>
    </StrictMode>,
  );
}

// There are no devtools here, not even in development; without this the
// window's errors went nowhere at all.
installGlobalErrorLogging('widgets');
void bootstrap().catch((error: unknown) => {
  // Plain DOM, not React: a widget cannot report React failing to start by
  // rendering React, and this window has no other error surface.
  console.error('Vitals widget failed to start', error);
  reportToLog('error', error, 'widgets:startup');
  const root = document.getElementById('root');
  if (root) root.textContent = error instanceof Error ? error.message : String(error);
});
