/**
 * Binds the shortcut registry to the window, once.
 *
 * Mounted in `AppShell` and nowhere else. Two listeners for the same chord
 * would run both handlers, which for a toggle means it flips twice and appears
 * to do nothing at all.
 */

import { useEffect, useRef } from 'react';

import { isTypingTarget, runShortcut, type ShortcutActions } from './shortcuts';

export function useShortcuts(actions: ShortcutActions): void {
  // The actions object is rebuilt on every render of the shell. Keeping it in
  // a ref means the listener is attached once for the lifetime of the window
  // rather than being torn down and re-added on each render, which would drop
  // a keystroke arriving in between.
  const latest = useRef(actions);

  useEffect(() => {
    latest.current = actions;
  }, [actions]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      // A repeat is someone holding the key down, not asking for the action
      // again. Opening the palette sixty times a second is not useful.
      if (event.repeat) return;
      if (isTypingTarget(event.target)) return;
      if (runShortcut(event, latest.current)) event.preventDefault();
    };

    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);
}
