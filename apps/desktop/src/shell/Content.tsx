import { useEffect, useRef, type ReactNode } from 'react';

/** How long to wait for a lazily loaded screen to render its heading. */
const HEADING_WAIT_MS = 2_000;

/**
 * Moves focus to the visible screen's heading once it exists.
 *
 * Without this, focus stays on the sidebar button that was pressed, and a
 * screen-reader user hears nothing about the page that just appeared — the
 * content changed somewhere they are not. The heading is made focusable
 * with `tabIndex={-1}` here so it is reachable by script but never a Tab stop.
 *
 * A screen still loading its chunk has no heading yet, so this watches the
 * region until one appears, and gives up after {@link HEADING_WAIT_MS} rather
 * than steal focus seconds later from wherever the user has gone since.
 */
function focusHeadingWhenReady(root: HTMLElement): () => void {
  const find = (): HTMLElement | null =>
    root.querySelector<HTMLElement>('[data-route-visible] h1, [data-route-visible] h2');
  const focus = (heading: HTMLElement): void => {
    if (!heading.hasAttribute('tabindex')) heading.setAttribute('tabindex', '-1');
    heading.focus({ preventScroll: true });
  };
  const now = find();
  if (now !== null) {
    focus(now);
    return () => {};
  }
  if (typeof MutationObserver !== 'function') return () => {};
  const state: { timer?: ReturnType<typeof setTimeout>; observer?: MutationObserver } = {};
  const stop = (): void => {
    state.observer?.disconnect();
    if (state.timer !== undefined) clearTimeout(state.timer);
  };
  const observer = new MutationObserver(() => {
    const heading = find();
    if (heading === null) return;
    stop();
    focus(heading);
  });
  state.observer = observer;
  state.timer = setTimeout(stop, HEADING_WAIT_MS);
  observer.observe(root, { childList: true, subtree: true, attributes: true });
  return stop;
}

/**
 * The scrolling content region.
 *
 * `<main>` with an id so the title bar's skip link has a target, and
 * `tabIndex={-1}` so that target is actually focusable — a skip link that moves the visual
 * viewport but not focus leaves a keyboard user's next Tab back in the
 * navigation they were trying to escape.
 *
 * After a navigation (not on first load, where focus belongs to the start of
 * the document), focus moves to the new screen's heading.
 *
 * Scroll is reset to the top on navigation. Carrying the previous section's
 * offset into a shorter one lands the user at the bottom of a page they have
 * not seen the top of.
 *
 * It is reset with an effect, NOT by keying the wrapper on the route. A key
 * change remounts the entire subtree, and the subtree is `RouteView`, whose
 * whole purpose is to keep every visited screen mounted inside an
 * `<Activity>` so its state survives. The key silently destroyed that: each
 * navigation rebuilt the visited set from scratch and every screen reloaded.
 * Nothing failed, because the only test of the keep-alive rendered
 * `RouteView` on its own.
 */
export function Content({
  routeKey,
  children,
}: {
  readonly routeKey: string;
  readonly children: ReactNode;
}) {
  const main = useRef<HTMLElement | null>(null);
  const firstRoute = useRef(true);

  useEffect(() => {
    main.current?.scrollTo({ top: 0 });
  }, [routeKey]);

  useEffect(() => {
    if (firstRoute.current) {
      firstRoute.current = false;
      return undefined;
    }
    const root = main.current;
    if (root === null) return undefined;
    return focusHeadingWhenReady(root);
  }, [routeKey]);

  return (
    <main
      ref={main}
      id="main-content"
      tabIndex={-1}
      className="min-h-0 min-w-0 flex-1 overflow-x-hidden overflow-y-auto outline-none"
    >
      {/*
       * Content is capped and centred rather than filling the window without limit.
       *
       * `--content-max` bounds grids and tables; it is generous because this is a
       * data-dense app and the ultrawide breakpoints (3xl–5xl in styles.css) let
       * grids add columns rather than stretch tiles. Text-heavy blocks cap
       * themselves at `--reading-max`. The min-width side is handled by the
       * layout, not here: the sidebar collapses so 720px still leaves a usable
       * column.
       *
       * `@container/main` names this element as a container query root so
       * features can respond to the width they actually get (`@3xl/main:`)
       * rather than to the window, which differs from it by the sidebar.
       *
       * A flex column with a definite height, and every wrapper below it down
       * to the screen root is one too (routes.tsx). That is what lets a screen
       * give its table or card stack `flex-1` and scroll THAT region, leaving
       * its title and toolbar in place. Without a height anywhere in the
       * chain, `h-full` on a screen resolved to auto, the whole page grew to
       * its content and this `<main>` scrolled instead — 14 765 px of it on
       * Installed apps, with the search box scrolled away.
       */}
      {/* Horizontal padding is the scroll gutter plus 4 px: a scroll region
          pulls itself out by the gutter so shadows have room, and must stay
          inside this column or `main`'s overflow-x clip cuts them again. */}
      <div className="@container/main mx-auto flex h-full min-h-0 w-full max-w-[var(--content-max)] flex-col px-[calc(var(--scroll-gutter-x)+0.25rem)] py-4 sm:py-5">
        {children}
      </div>
    </main>
  );
}
