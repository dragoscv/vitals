/**
 * The Motion providers, in their own chunk.
 *
 * Only `MotionProvider` imports this, lazily. Everything from `motion/react`
 * that the shell needs at the root lives here so the bundler has one place to
 * put it — see `MotionProvider` for the measurement that forced the split.
 *
 * - `LazyMotion` with `domAnimation` loads only the DOM animation feature set,
 *   roughly a third of the full library. `strict` is the guard that keeps it
 *   that way: it throws the moment anyone renders a `motion.*` component,
 *   which would pull the complete feature bundle into whichever chunk did it.
 *   Use `m.*` everywhere.
 * - `MotionConfig reducedMotion` maps the app's own three-state preference on
 *   to Motion's, so JavaScript-driven animation obeys the same setting as the
 *   CSS in `theme.css`.
 */

import { LazyMotion, MotionConfig, domAnimation } from 'motion/react';
import type { ReactNode } from 'react';

export function MotionRuntime({
  children,
  reducedMotion,
}: {
  readonly children: ReactNode;
  readonly reducedMotion: 'user' | 'always' | 'never';
}) {
  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion={reducedMotion}>{children}</MotionConfig>
    </LazyMotion>
  );
}
