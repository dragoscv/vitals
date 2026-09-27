# 0030 — Fluent depth redesign; View Transitions for navigation

- Status: Accepted
- Date: 2026-09-27
- Tracker: S12

## Context

The desktop app was functionally rich and visually flat: cards were bordered
rectangles on a plain background, the sidebar marked the active item with a
tint, navigation was a 140 ms fade, and every number jumped to its new value.
Eleven CSS custom properties were referenced but never defined, so the
per-core bars, alert colours and HUD sparklines rendered with no colour at
all. The user asked for the app to look modern, with animation, transitions
and morphing, and for everything to work.

Two constraints from earlier decisions still hold: the Motion library stays
off the critical path (ADR 0017), and the size budget is a hard gate raised
only deliberately (ADR 0018).

## Decision

**Depth from light, not from blur.** Cards get a vertical gradient surface,
a one-pixel highlight on the top edge and a layered shadow; hover lifts the
shadow, never the card. The accent is used as light — a glow token — on the
active nav item, the primary button and the selected rail entry. Two static
radial pools of the accent hue sit behind the content column. Tokens live
in `theme.css`; nothing is hard-coded per component.

**Navigation is a View Transition.** `AppShell.navigate` wraps the route
change in `document.startViewTransition` with `flushSync`, so the outgoing
screen cross-fades and blurs while the incoming one rises; the page title
is a named element that morphs between its old and new box. Only the
visible route's title carries the name — every visited screen stays in the
DOM under `<Activity>`, and two elements sharing a name abort the
transition (observed live). The sidebar and title bar are named and pinned
so they do not fade with the content. The WAAPI fade in `RouteTransition`
remains for a WebView without the API and for tests.

**One morphing indicator, not a shared `layoutId`.** The sidebar renders a
single absolutely positioned pill moved by `transform` with a spring easing.
Motion's shared-layout pattern is exactly the React 19 `removeChild` crash
recorded in user memory, so it was not used.

**Numbers roll.** `AnimatedValue` tweens the first number of an already
formatted string, keeping the formatter's unit and separators. The tween is
`aria-hidden`; a visually hidden copy carries the final value, so a live
region announces once.

**Budget raised to the measured value.** Initial load 185 919 → 188 880 B
gzip; shipped 376 596 → 382 460 B. Recorded in `size-budget.json`'s comment
with the breakdown. HUD and mobile stay under their existing budgets.

## Consequences

- The app reads as one object rather than a spreadsheet, at a cost of
  about 3 KB gzip on first paint.
- Reduced motion collapses every new animation: the CSS durations via the
  existing media query and the app's own `data-reduce-motion` override
  (which the media query could not see and which is now honoured in CSS),
  `AnimatedValue` and the sidebar spring via `useReducedMotion`, View
  Transitions by skipping `startViewTransition` entirely.
- A new screen inherits the look by using `Card` and the shared `h2`
  heading class; it must not name its own elements for the transition.
- The fixes that surfaced alongside the redesign — disk throughput that was
  always zero, GPU adapters labelled by LUID, drive kinds "Unknown", an
  empty User column — are recorded under S12 in the tracker, not here.
