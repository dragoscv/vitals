import { flushSync } from 'react-dom';
import { useCallback } from 'react';

import { useReducedMotion } from './useReducedMotion';

/**
 * Where a named shared element lives, as `selector -> view-transition-name`.
 *
 * A record rather than an array so a caller cannot accidentally give two
 * elements the same name: the browser aborts the whole transition when it
 * finds a duplicate, which turns the screen into a cut with no explanation.
 */
export type ViewTransitionNames = Readonly<Record<string, string>>;

/** Runs the update, through a view transition when the platform has one. */
export type ViewTransitionRunner = (update: () => void, names?: ViewTransitionNames) => void;

interface NamedElement {
  readonly element: HTMLElement;
  /** The value to put back afterwards, which may be empty or a caller's own. */
  readonly previous: string;
}

/**
 * Morphs a change into place with the View Transitions API, when that is both
 * available and wanted.
 *
 * The hook exists because the naive call is wrong in three separate ways that
 * all look like "the animation just didn't happen":
 *
 * - The update must be committed **synchronously** inside the callback. A
 *   normal React update lands a frame later, after the browser has already
 *   snapshotted, so it animates the old screen into itself. `flushSync` is what
 *   makes the new DOM exist before the callback returns, and putting it here
 *   rather than leaving it to each caller means it cannot be forgotten.
 * - Under reduced motion the transition must be skipped, not shortened. The
 *   stylesheet cannot decide this one: the API captures and animates whether or
 *   not any CSS animation is running.
 * - Engines without the API must run the update anyway. A missing
 *   `startViewTransition` that silently swallows the update is a button that
 *   does nothing.
 *
 * The returned runner is stable across renders, so it can be a dependency of a
 * `useCallback` without invalidating it.
 */
export function useViewTransition(): ViewTransitionRunner {
  const reduced = useReducedMotion();

  return useCallback(
    (update: () => void, names?: ViewTransitionNames) => {
      // `document.startViewTransition` comes from the ambient DOM library, not
      // from a local declaration: an earlier version of this file re-declared
      // it to narrow the return type to `{ finished }`, which stopped being
      // assignable to `Document` (TS2430) as soon as the library gained the
      // real signature — the declaration predated any engine shipping the API.
      // The runtime check below is what supports an engine without it, and no
      // type can express "this browser has not implemented it yet" anyway.
      if (reduced || typeof document.startViewTransition !== 'function') {
        update();
        return;
      }

      // Applied before the call, because that is when the browser captures the
      // *old* snapshot: a name set later is not on the outgoing element. A
      // caller morphing one element into a different one sets the name on the
      // incoming element inside `update`, the way `dismissSplash` does.
      const named = applyNames(names);
      const transition = document.startViewTransition(() => {
        flushSync(update);
      });
      // `finally`, not `then`: a transition that never starts resolves
      // `finished` by rejecting, and leaving the names behind would give the
      // element a `view-transition-name` that nothing ever clears — the next
      // transition then aborts on the duplicate.
      void transition.finished.finally(() => {
        for (const { element, previous } of named) element.style.viewTransitionName = previous;
      });
    },
    [reduced],
  );
}

function applyNames(names: ViewTransitionNames | undefined): readonly NamedElement[] {
  if (names === undefined) return [];
  const applied: NamedElement[] = [];
  for (const [selector, name] of Object.entries(names)) {
    const element = document.querySelector<HTMLElement>(selector);
    if (element === null) continue;
    applied.push({ element, previous: element.style.viewTransitionName });
    element.style.viewTransitionName = name;
  }
  return applied;
}
