import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { initI18n } from '@vitals/i18n';

import { App } from './App';
import './styles.css';

/**
 * Boots the application.
 *
 * i18n is awaited before the first render on purpose: mounting with the
 * fallback language and then swapping causes a visible flash of English in a
 * Romanian UI, and it reflows every label width in a dense table.
 */
async function bootstrap(): Promise<void> {
  const container = document.getElementById('root');
  if (!container) throw new Error('#root is missing from index.html');

  await initI18n(undefined, { reportMissingKeys: import.meta.env.DEV });

  createRoot(container).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );

  // Dismiss the inline splash only once React has committed, so there is no
  // gap between the splash disappearing and content appearing.
  requestAnimationFrame(() => {
    document.getElementById('splash')?.setAttribute('hidden', '');
  });
}

void bootstrap();
