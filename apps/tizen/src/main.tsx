import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App';
import { initI18n } from './i18n';
import './styles.css';
import { registerRemoteKeys } from './ui/focus';

/**
 * Root font size as a fraction of the viewport height (see styles.css), so a
 * 1080-line TV and a 1440-line ultrawide show the same number of lines.
 */
function sizeRoot(): void {
  document.documentElement.style.fontSize = `${window.innerHeight / 54}px`;
}

async function start(): Promise<void> {
  sizeRoot();
  window.addEventListener('resize', sizeRoot);
  registerRemoteKeys();
  await initI18n();
  const root = document.getElementById('root');
  if (root === null) throw new Error('no #root element');
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  );
}

start().catch((error: unknown) => {
  console.error('Vitals failed to start', error);
});
