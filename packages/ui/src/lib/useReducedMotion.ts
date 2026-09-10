import { useSyncExternalStore } from 'react';

const QUERY = '(prefers-reduced-motion: reduce)';

/**
 * The application's own override, mirrored on to the root element.
 *
 * The desktop app lets a user reduce motion inside Vitals without changing
 * the setting for their whole system, and writes that choice to
 * `data-reduce-motion` on `<html>` (see the desktop app's `theme/apply.ts`).
 * The stylesheet and Motion's `MotionConfig` both honour it, so this hook has
 * to as well — otherwise a component consulting it would keep animating while
 * everything around it had stopped. Absent means "follow the system".
 *
 * Read from the DOM rather than from a React context on purpose: this package
 * is shared with the overlay window, which cannot reach the settings store and
 * does not mount the theme provider.
 */
function override(): boolean | null {
  if (typeof document === 'undefined') return null;
  const value = document.documentElement.dataset['reduceMotion'];
  if (value === undefined) return null;
  return value === 'true';
}

function prefersReduced(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false;
  return window.matchMedia(QUERY).matches;
}

function subscribe(onChange: () => void): () => void {
  const unsubscribers: (() => void)[] = [];

  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    const list = window.matchMedia(QUERY);
    list.addEventListener('change', onChange);
    unsubscribers.push(() => list.removeEventListener('change', onChange));
  }

  if (typeof document !== 'undefined' && typeof MutationObserver === 'function') {
    const observer = new MutationObserver(onChange);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-reduce-motion'],
    });
    unsubscribers.push(() => observer.disconnect());
  }

  return () => {
    for (const off of unsubscribers) off();
  };
}

function getSnapshot(): boolean {
  return override() ?? prefersReduced();
}

/**
 * Whether motion should be reduced — the app's own override when it has one,
 * otherwise the operating system's preference.
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
