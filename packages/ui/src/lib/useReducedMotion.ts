import { useSyncExternalStore } from 'react';

const QUERY = '(prefers-reduced-motion: reduce)';

function subscribe(onChange: () => void): () => void {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') {
    return () => {};
  }
  const list = window.matchMedia(QUERY);
  list.addEventListener('change', onChange);
  return () => list.removeEventListener('change', onChange);
}

function getSnapshot(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false;
  return window.matchMedia(QUERY).matches;
}

/**
 * Whether the user has asked the system to reduce motion.
 *
 * The stylesheet already collapses CSS animation durations globally, but that
 * cannot reach motion expressed in JavaScript — a spinner that keeps rotating
 * because its animation is driven by a timer, or an indeterminate bar that
 * sweeps. Components with such motion must consult this and render a static
 * state instead.
 *
 * Implemented with `useSyncExternalStore` rather than `useEffect` + state so
 * the very first render is already correct: a user who has requested reduced
 * motion should never see one frame of animation before we catch up.
 */
export function useReducedMotion(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, () => false);
}
