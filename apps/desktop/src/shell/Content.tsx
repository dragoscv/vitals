import { useEffect, useRef, type ReactNode } from 'react';

/**
 * The scrolling content region.
 *
 * `<main>` with an id so a future skip-link has a target, and `tabIndex={-1}`
 * so that target is actually focusable — a skip link that moves the visual
 * viewport but not focus leaves a keyboard user's next Tab back in the
 * navigation they were trying to escape.
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

  useEffect(() => {
    main.current?.scrollTo({ top: 0 });
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
       */}
      <div className="@container/main mx-auto w-full max-w-[var(--content-max)] px-4 py-4 sm:px-6 sm:py-5">
        {children}
      </div>
    </main>
  );
}
