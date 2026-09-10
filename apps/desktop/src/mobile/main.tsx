import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { initI18n } from '@vitals/i18n';

import { followSystemTheme } from './lib/theme';
import { MobileApp, storedLocale } from './MobileApp';
import './styles.css';

/**
 * Boots the phone app.
 *
 * Nothing from the desktop shell is imported here — a phone on a LAN must not
 * download the desktop bundle to show three tabs — and i18n is awaited before
 * the first render so a Romanian phone never flashes English.
 */
async function bootstrap(): Promise<void> {
  const container = document.getElementById('root');
  if (!container) throw new Error('#root is missing from mobile.html');

  followSystemTheme(document.documentElement, window.matchMedia.bind(window));
  await initI18n(storedLocale(window.localStorage), { reportMissingKeys: import.meta.env.DEV });

  createRoot(container).render(
    <StrictMode>
      <MobileApp />
    </StrictMode>,
  );
}

void bootstrap().catch((error: unknown) => {
  console.error('Vitals mobile failed to start', error);
  const root = document.getElementById('root');
  if (root) root.textContent = error instanceof Error ? error.message : String(error);
});
