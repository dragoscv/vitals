/**
 * Theme for the overlay: dark, always.
 *
 * The HUD floats over whatever is on screen — a photo wallpaper, a game, a
 * white document — so it cannot borrow the desktop window's light/dark
 * choice. A dark translucent panel is legible over all of them; a light one
 * disappears against half of them. Only the accent follows the app default so
 * the sparklines match the charts in the main window.
 */

/** Applies the overlay's fixed palette to `root`. */
export function applyOverlayTheme(root: HTMLElement): void {
  root.classList.add('dark');
  root.dataset['accent'] = 'blue';
  // `solid` rather than mica/acrylic: the window is already transparent, and
  // a backdrop filter on a transparent Windows webview is expensive and
  // renders inconsistently over fullscreen games — the one place a HUD
  // matters most.
  root.dataset['surface'] = 'solid';
}
