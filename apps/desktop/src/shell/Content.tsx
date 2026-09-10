import { type ReactNode } from 'react';

/**
 * The scrolling content region.
 *
 * `<main>` with an id so a future skip-link has a target, and `tabIndex={-1}`
 * so that target is actually focusable — a skip link that moves the visual
 * viewport but not focus leaves a keyboard user's next Tab back in the
 * navigation they were trying to escape.
 *
 * `key` on the inner wrapper resets scroll position on navigation. Carrying
 * the previous section's scroll offset into a shorter one lands the user at
 * the bottom of a page they have not seen the top of.
 */
export function Content({
  routeKey,
  children,
}: {
  readonly routeKey: string;
  readonly children: ReactNode;
}) {
  return (
    <main
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
      <div
        key={routeKey}
        className="mx-auto w-full max-w-[var(--content-max)] px-4 py-4 sm:px-6 sm:py-5"
      >
        {children}
      </div>
    </main>
  );
}
