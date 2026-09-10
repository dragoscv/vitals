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
       * Content is capped and centred rather than filling the window.
       *
       * At 21:9 and beyond an uncapped grid produces card rows a metre wide and
       * text lines far past the ~75 character readability limit. `--content-max`
       * is generous — this is a data-dense app and cards genuinely benefit from
       * width — but it is finite, and the leftover space becomes margin instead
       * of stretch. The min-width side is handled by the layout, not here: the
       * sidebar collapses so 720px still leaves a usable column.
       */}
      <div className="mx-auto w-full max-w-[var(--content-max)] px-4 py-4 sm:px-6 sm:py-5">
        {children}
      </div>
    </main>
  );
}
