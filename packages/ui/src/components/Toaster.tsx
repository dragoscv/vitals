import { useSyncExternalStore } from 'react';
import { Toaster as SonnerToaster, toast } from 'sonner';
import { useReducedMotion } from '../lib/useReducedMotion';

/**
 * Fire a transient notification.
 *
 * Re-exported so no feature imports `sonner` directly: the library is an
 * implementation detail, and a single import site is what makes it possible
 * to swap or wrap it without touching twenty call sites.
 */
export { toast };
export type { ExternalToast, ToasterProps as SonnerToasterProps } from 'sonner';

export interface ToasterProps {
  /**
   * Accessible name of the notification region, translated by the caller.
   *
   * This package carries no i18n runtime — it is shared with the overlay
   * window, which never initialises i18next — so every user-visible string
   * arrives as a prop. Without these, sonner falls back to hardcoded English
   * ("Notifications", "Close toast") in a Romanian UI.
   */
  readonly regionLabel: string;
  readonly closeLabel: string;
  /** Milliseconds a toast stays up. */
  readonly duration?: number;
}

/**
 * Reads the resolved colour mode from the document.
 *
 * Not sonner's own `theme="system"`: that consults the OS media query, while
 * this app resolves light/dark itself (a user can pin dark on a light system)
 * and publishes the result as a `.dark` class on the root. Consulting the
 * media query here would leave the toast the only light surface on a dark
 * screen. Read from the DOM rather than a React context for the same reason
 * `useReducedMotion` does: the overlay window mounts no providers.
 */
function subscribeToMode(onChange: () => void): () => void {
  if (typeof MutationObserver !== 'function' || typeof document === 'undefined') return () => {};
  const observer = new MutationObserver(onChange);
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ['class'] });
  return () => observer.disconnect();
}

function readMode(): 'light' | 'dark' {
  if (typeof document === 'undefined') return 'light';
  return document.documentElement.classList.contains('dark') ? 'dark' : 'light';
}

/**
 * The application's toast surface.
 *
 * Mounted once, at the shell. Everything else calls {@link toast}.
 *
 * ## Styling is tokens only
 *
 * sonner ships its own palette via `richColors`; that is switched off and the
 * surface is dressed in `var(--color-*)` instead. The theme has three
 * independent axes (light/dark, accent hue, surface translucency) and any
 * literal colour silently opts out of all three — a toast would be the one
 * element that ignores the user's accent and stays opaque over an acrylic
 * window.
 *
 * ## A close button, always
 *
 * Auto-dismiss alone is a trap for anyone reading slowly, using a screen
 * magnifier, or interrupted mid-sentence: the only way to re-read the message
 * is to reproduce whatever caused it. The button costs nothing and makes the
 * toast dismissible from the keyboard.
 */
export function Toaster({
  regionLabel,
  closeLabel,
  duration = 5000,
}: ToasterProps): React.JSX.Element {
  const reduced = useReducedMotion();
  const mode = useSyncExternalStore(subscribeToMode, readMode, () => 'light' as const);

  return (
    <SonnerToaster
      theme={mode}
      // sonner's palette is bypassed entirely — see the note above.
      richColors={false}
      closeButton
      duration={duration}
      position="bottom-right"
      containerAriaLabel={regionLabel}
      // The slide-in is decoration: the toast's arrival is already announced
      // through the live region, so under reduced motion it can simply appear.
      // theme.css collapses animation duration under the media query, but not
      // under the app's own `data-reduce-motion` override, which is the case
      // this class covers.
      className={reduced ? 'vitals-toaster-still' : ''}
      toastOptions={{
        closeButtonAriaLabel: closeLabel,
        classNames: {
          toast:
            'rounded-[var(--radius-control)] border border-[var(--color-border-default)] bg-[var(--color-bg-raised)] text-[var(--color-fg-default)] shadow-[var(--shadow-overlay)]',
          title: 'text-sm font-medium text-[var(--color-fg-default)]',
          description: 'text-2xs text-[var(--color-fg-muted)]',
          actionButton:
            'rounded-[var(--radius-control)] bg-[var(--color-accent)] text-[var(--color-fg-on-accent)]',
          cancelButton:
            'rounded-[var(--radius-control)] bg-[var(--color-bg-inset)] text-[var(--color-fg-default)]',
          closeButton:
            'border-[var(--color-border-default)] bg-[var(--color-bg-raised)] text-[var(--color-fg-muted)]',
          error: 'text-[var(--color-status-danger)]',
          success: 'text-[var(--color-status-ok)]',
          warning: 'text-[var(--color-status-warn)]',
          info: 'text-[var(--color-status-info)]',
        },
      }}
    />
  );
}
