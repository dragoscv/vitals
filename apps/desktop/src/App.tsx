import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import { signalReady } from './lib/ready';
import { ThemeProvider } from './theme/ThemeProvider';

/**
 * The application shell.
 *
 * Currently a placeholder while the platform sampler is built. The theme
 * provider and i18n are wired so both are exercised from the first commit
 * rather than retrofitted — retrofitting either is far more expensive than
 * carrying them from the start.
 */
export function App() {
  return (
    <ThemeProvider>
      <Shell />
    </ThemeProvider>
  );
}

function Shell() {
  const { t } = useTranslation();

  // The window is created hidden to avoid a white flash while the webview
  // boots. Nothing else reveals it, so if this effect never runs the app has
  // no visible window at all — Rust falls back after five seconds, but that
  // fallback shows a blank window rather than a painted one.
  useEffect(() => {
    signalReady();
  }, []);

  return (
    <div className="flex h-full flex-col">
      <header
        className="surface-chrome flex h-9 shrink-0 items-center border-b px-3"
        data-tauri-drag-region
      >
        <span className="text-2xs font-medium tracking-wide text-[var(--color-fg-muted)]">
          {t('app.name')}
        </span>
      </header>

      <main className="flex flex-1 items-center justify-center overflow-hidden">
        <div className="max-w-md space-y-2 px-6 text-center">
          <h1 className="text-lg font-semibold">{t('app.name')}</h1>
          <p className="text-sm text-[var(--color-fg-muted)]">{t('app.tagline')}</p>
        </div>
      </main>
    </div>
  );
}
