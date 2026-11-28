/**
 * Theme for a widget window: dark, always.
 *
 * The same decision as the HUD's, for the same reason: a widget floats over
 * whatever is on screen — a photo wallpaper, a game, a white document — so it
 * cannot borrow the main window's light/dark choice. A dark translucent panel
 * is legible over all of them and a light one disappears against half.
 *
 * # Why a copy of `hud/lib/theme.ts`
 *
 * Two lines of code, and still not imported from `hud/`: a widget window must
 * not depend on the HUD tree. The two overlays are independent features (one
 * is a fixed three-reading strip, the other is N configurable panels), and a
 * shared module would turn any HUD change into a widget-window regression for
 * no saving at all.
 */

/**
 * Applies the overlay's fixed palette to `root`.
 *
 * `accent` is the one colour taken from the user's choice in the main window,
 * so a widget's ring and sparklines match the charts they are read next to.
 * It is a plain string rather than an `Accent` because it arrives over an
 * event channel from another webview; the caller checks it before passing it.
 * Defaulted so an early render, before the main window has answered, still
 * gets a complete palette.
 */
export function applyWidgetTheme(root: HTMLElement, accent: string = 'green'): void {
  root.classList.add('dark');
  root.dataset['accent'] = accent;
}
