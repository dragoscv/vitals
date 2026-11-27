import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { App } from './App';
import { initI18n } from './i18n';
import './styles.css';
import { ErrorBoundary, RootError } from './ui/ErrorBoundary';
import { registerRemoteKeys } from './ui/focus';

/**
 * Root font size as a fraction of the viewport height (see styles.css), so a
 * 1080-line TV and a 1440-line ultrawide show the same number of lines.
 */
function sizeRoot(): void {
  document.documentElement.style.fontSize = `${window.innerHeight / 45}px`;
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
      {/* At the root: a throw anywhere below unmounts the tree and the TV is
          left on a black screen a viewer cannot recover from with the remote. */}
      <ErrorBoundary fallback={(error, retry) => <RootError error={error} onRetry={retry} />}>
        <App />
      </ErrorBoundary>
    </StrictMode>,
  );
}

start().catch((error: unknown) => {
  console.error('Vitals failed to start', error);
});
