/**
 * Theme for the phone: follows the OS, always.
 *
 * The desktop lets the user pick; the phone has no settings store worth
 * building for one toggle, and a page that ignores dark mode on an OLED phone
 * at night is the first thing people notice.
 */

/** Applies `.dark` from `prefers-color-scheme` and keeps it in step. Returns a disposer. */
export function followSystemTheme(
  root: HTMLElement,
  matchMedia: typeof window.matchMedia,
): () => void {
  const query = matchMedia('(prefers-color-scheme: dark)');
  const apply = () => {
    root.classList.toggle('dark', query.matches);
  };
  apply();
  // The accent is the desktop's default; every derived token follows.
  root.dataset['accent'] = 'green';
  query.addEventListener('change', apply);
  return () => query.removeEventListener('change', apply);
}
