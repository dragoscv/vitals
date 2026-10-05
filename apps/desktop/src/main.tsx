import { StrictMode } from 'react';
import { createRoot, type Root } from 'react-dom/client';

import { initI18n } from '@vitals/i18n';

import { App } from './App';
import { registerDashboardStrings } from './features/dashboard';
import { installGlobalErrorLogging, reportToLog } from './lib/logToFile';
import { suppressNativeContextMenu } from './lib/nativeMenu';
import { registerShellStrings } from './shell/strings';
import './styles.css';

/**
 * The mounted root, for the bootstrap test only. A root left mounted when
 * the test environment tears `window` down keeps React's scheduler running
 * against a document that no longer exists — "window is not defined", an
 * unhandled error that failed CI while every assertion passed.
 */
export let mountedRoot: Root | null = null;

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

  // Strictly after the await: i18next only defines `addResourceBundle` once
  // initialised, and this used to run at module scope in `App.tsx` — which
  // threw before React ever mounted and left the app stuck on its splash.
  //
  // Only the two eager surfaces register here. Every other feature registers
  // its own strings from its lazy chunk, because importing a bundle from this
  // file pulls the whole feature in through its barrel and undoes the route
  // splitting entirely.
  registerShellStrings();
  registerDashboardStrings();
  suppressNativeContextMenu();

  mountedRoot = createRoot(container);
  mountedRoot.render(
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

/**
 * Replaces the splash with a readable error.
 *
 * Without this, ANY throw in `bootstrap` leaves the pulsing splash on screen
 * forever with no indication of what went wrong — the user sees a hung app
 * and the cause is only visible in a devtools console they cannot open in a
 * release build. That is exactly how a one-line ordering mistake became an
 * app that never started.
 *
 * Rendered as plain DOM rather than React: if React is what failed, using it
 * to report the failure fails too.
 */
function reportStartupFailure(error: unknown): void {
  console.error('Vitals failed to start', error);
  reportToLog('error', error, 'main:startup');

  const splash = document.getElementById('splash');
  if (!splash) return;

  const message = error instanceof Error ? error.message : String(error);

  splash.replaceChildren();
  splash.style.padding = '2rem';
  splash.style.textAlign = 'center';
  splash.style.font = '13px/1.5 system-ui, sans-serif';

  const heading = document.createElement('p');
  heading.textContent = 'Vitals could not start.';
  heading.style.fontWeight = '600';
  heading.style.margin = '0 0 0.5rem';

  const detail = document.createElement('p');
  // textContent, never innerHTML: the message can contain arbitrary text.
  detail.textContent = message;
  detail.style.opacity = '0.7';
  detail.style.margin = '0';
  detail.style.wordBreak = 'break-word';

  splash.append(heading, detail);
}

// First, before anything can throw: an error during bootstrap that escapes
// the catch below (a throw inside the rAF, say) is otherwise lost in a
// release build.
installGlobalErrorLogging('main');
void bootstrap().catch(reportStartupFailure);
