import { cn } from './cn';
import { disabledData, overlayMotion, overlaySurface } from './styles';

/**
 * Shared class strings for the dropdown and context menus.
 *
 * The two are separate Radix primitives with identical anatomy. Sharing the
 * styling here rather than duplicating it means a right-click menu and a
 * toolbar menu can never drift apart visually — which matters because in this
 * app they frequently expose the *same* actions for the same process, and a
 * user who notices they look different will reasonably assume they do
 * different things.
 */
export const menuContent = cn(
  overlaySurface,
  overlayMotion,
  'min-w-44 origin-(--radix-popper-transform-origin) p-1',
);

export const menuItem = cn(
  'relative flex cursor-default items-center gap-2 rounded-[var(--radius-control)] px-2 py-1.5 text-2xs outline-none select-none',
  // Radix drives selection with `data-highlighted`, which follows both the
  // pointer and the arrow keys. Styling `:hover` instead is the classic bug
  // where keyboard navigation moves an invisible cursor.
  'data-highlighted:bg-[var(--color-accent-subtle)] data-highlighted:text-[var(--color-accent)]',
  disabledData,
  '[&_svg]:size-3.5 [&_svg]:shrink-0',
);

/** Irreversible actions. Tinted rather than only red-on-highlight so the
 *  warning survives keyboard highlighting too. */
export const menuItemDanger = cn(
  'text-[var(--color-status-danger)]',
  'data-highlighted:bg-[var(--color-status-danger)]/12 data-highlighted:text-[var(--color-status-danger)]',
);

export const menuLabel = 'px-2 py-1 text-2xs font-semibold text-[var(--color-fg-subtle)]';

export const menuSeparator = '-mx-1 my-1 h-px bg-[var(--color-border-subtle)]';

/** Reserved gutter so checkable items do not shift text when they toggle. */
export const menuIndicatorSlot = 'absolute left-2 inline-flex size-3.5 items-center justify-center';

export const menuItemInset = 'pl-7';

export const menuShortcut = 'ml-auto pl-4 text-2xs tracking-wide text-[var(--color-fg-subtle)]';
