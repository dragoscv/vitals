/**
 * Class fragments shared across components.
 *
 * These exist so that "what a focusable control looks like" is defined once.
 * The alternative — repeating the ring utilities in twenty files — guarantees
 * that some control eventually ships with a subtly different or missing focus
 * indicator, which is a WCAG 2.4.7 failure that nobody notices because it only
 * shows up when navigating by keyboard.
 *
 * Every colour is a `var(--color-*)` token: the theme has three independent
 * axes (light/dark, accent hue, surface translucency) and a literal colour
 * silently opts out of all three.
 */

/**
 * The focus indicator.
 *
 * `focus-visible` rather than `focus` so a mouse click does not leave a ring
 * behind. The offset keeps the ring clear of the control's own border, which
 * matters most on the danger variant where a red border and an accent ring
 * would otherwise touch and read as one muddy edge.
 */
export const focusRing =
  'outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--color-accent)]';

/**
 * Applied to controls that can be disabled.
 *
 * `pointer-events-none` is deliberately absent: removing pointer events also
 * removes hover, so a disabled control can no longer carry a tooltip
 * explaining *why* it is disabled — which is usually the only thing the user
 * wants to know at that moment.
 */
export const disabledControl = 'disabled:cursor-not-allowed disabled:opacity-50';

/** Matches the `disabled` styling for Radix parts that use `data-disabled`. */
export const disabledData =
  'data-[disabled]:cursor-not-allowed data-[disabled]:opacity-50 data-disabled:cursor-not-allowed data-disabled:opacity-50';

/** Surface treatment for anything that floats above the page. */
export const overlaySurface =
  'z-50 rounded-[var(--radius-control)] border border-[var(--color-border-default)] bg-[var(--color-bg-raised)] text-[var(--color-fg-default)] shadow-[var(--shadow-overlay)]';

/**
 * Enter/exit animation for floating surfaces.
 *
 * Expressed as a CSS transition on `data-state` rather than with the
 * `tailwindcss-animate` plugin, which this repo does not install — one more
 * dependency for two keyframes is not a trade worth making. Reduced motion is
 * honoured by theme.css, which collapses transition duration to 0.01ms rather
 * than removing it, so Radix's presence machinery still observes the
 * transition end and unmounts instead of orphaning the node.
 */
export const overlayMotion =
  'transition-[opacity,transform] duration-(--duration-fast) ease-(--ease-out-quart) data-[state=closed]:scale-95 data-[state=closed]:opacity-0 data-[state=open]:scale-100 data-[state=open]:opacity-100';
