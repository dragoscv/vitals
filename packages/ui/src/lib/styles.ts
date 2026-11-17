/**
 * Class fragments shared across components.
 *
 * These exist so that "what a focusable control looks like" is defined once.
 * The alternative — repeating the ring utilities in twenty files — guarantees
 * that some control eventually ships with a subtly different or missing focus
 * indicator, which is a WCAG 2.4.7 failure that nobody notices because it only
 * shows up when navigating by keyboard.
 *
 * Every colour is a `var(--color-*)` token: the theme has independent axes
 * (light/dark, accent hue) and a literal colour silently opts out of them.
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
 * A class from `theme.css` rather than a set of Tailwind utilities, because the
 * rule it carries cannot be expressed as one: the open and closed states must
 * name two *different* keyframe animations. Radix's presence layer reads the
 * element's computed `animation-name` to decide whether it is still on its way
 * out, and a transition leaves that at `none` — which presence reads as "not
 * animating", unmounting the overlay in the same tick the close begins. That is
 * why these overlays used to pop in and out with no animation at all, and why
 * replacing this with `transition-*` utilities silently brings the bug back.
 *
 * The keyframes, the `@starting-style` enhancement and the reduced-motion
 * escape hatch all live in `theme.css`.
 */
export const overlayMotion = 'vitals-overlay-motion';

/**
 * The same pairing for a full-bleed scrim, which fades without scaling — a
 * scaled backdrop pulls its own edges in off the window and leaves a border of
 * un-dimmed app around the dialog it is meant to hide.
 */
export const overlayScrim = 'vitals-overlay-scrim';
