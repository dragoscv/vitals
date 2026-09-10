/**
 * The single place motion is configured for the whole shell.
 *
 * The providers themselves are in `MotionRuntime`, behind a lazy boundary,
 * and that boundary is the only thing keeping Motion off the critical path.
 * Measured on this build, gzip, against a 181.6 KB initial-load budget with
 * 4.3 KB of headroom:
 *
 * - `LazyMotion` + `MotionConfig` imported eagerly, features deferred: entry
 *   grew 11.6 KB. Splitting only the features was not enough, because the
 *   providers and the feature bundle share internals and the bundler hoisted
 *   those into the entry.
 * - Providers behind `lazy()` (this file): entry unchanged at 57.5 KB. The
 *   whole of Motion is one chunk fetched after first paint.
 *
 * Until that chunk arrives the tree simply has no motion context, which is
 * correct: the only `m.*` surfaces are themselves in lazy chunks that load
 * later, and the route fade uses the platform's own animation API precisely so
 * it never has to wait for this.
 *
 * The cost, stated plainly: when the chunk lands, React swaps the fallback
 * for the real tree and the shell **remounts once**, a few milliseconds after
 * first paint and before the window is even revealed (`signalReady` fires
 * two frames after the commit above this). The shell already remounts once
 * at that moment for settings hydration (`ThemeProvider key={hydrated}` in
 * `App.tsx`), so this adds nothing a user can perceive. It does mean a test
 * that interacts with the shell must wait for the runtime first — see
 * `AppShell.test.tsx`.
 */

import { Suspense, lazy, type ReactNode } from 'react';

import type { ThemeSettings } from './types';

const MotionRuntime = lazy(async () => ({
  default: (await import('./MotionRuntime')).MotionRuntime,
}));

/**
 * Translates the stored preference into Motion's vocabulary.
 *
 * `null` means "follow the operating system", which is what Motion calls
 * `user` — it reads `prefers-reduced-motion` itself. The explicit values are
 * an override in both directions, because a user who has reduced motion
 * system-wide may still want it here, and vice versa.
 */
export function reducedMotionFor(
  reduceMotion: ThemeSettings['reduceMotion'],
): 'user' | 'always' | 'never' {
  if (reduceMotion === null) return 'user';
  return reduceMotion ? 'always' : 'never';
}

/**
 * `reduceMotion` is a prop rather than read from the theme context, so this
 * can wrap a subtree in a test without dragging the whole provider stack in.
 * The shell passes the real setting.
 */
export function MotionProvider({
  children,
  reduceMotion = null,
}: {
  readonly children: ReactNode;
  readonly reduceMotion?: ThemeSettings['reduceMotion'];
}) {
  return (
    // `children` as the fallback, not `null`: the app must paint on the first
    // frame whether or not Motion has arrived.
    <Suspense fallback={children}>
      <MotionRuntime reducedMotion={reducedMotionFor(reduceMotion)}>{children}</MotionRuntime>
    </Suspense>
  );
}
