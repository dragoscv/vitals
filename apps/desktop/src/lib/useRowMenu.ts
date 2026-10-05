/**
 * One right-click menu open at a time, opened only on purpose.
 *
 * Radix's `ContextMenu` opens on ANY `contextmenu` event and on a 700 ms
 * press from any pointer that is not a mouse — a precision touchpad reports
 * `touch` — so on 2026-10-05 a slow left click on a process row opened its
 * menu, pinned near the window's corner. The deliberate routes are:
 * a right click (`button === 2`), and Shift+F10 or the Menu key pressed
 * just before. Everything else is refused.
 *
 * Usage, per list:
 *
 * ```tsx
 * const menu = useRowMenu();
 * <div onKeyDown={menu.onKeyDown}>
 *   {rows.map((row) => (
 *     <ContextMenu key={row.id} {...menu.rootProps(row.id)}>
 *       <ContextMenuTrigger asChild>
 *         <div onContextMenu={menu.onContextMenu}>…</div>
 *       </ContextMenuTrigger>
 *       <ContextMenuContent>…</ContextMenuContent>
 *     </ContextMenu>
 *   ))}
 * </div>
 * ```
 */

import { useCallback, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react';

/** How long after the menu key a positionless `contextmenu` still counts. */
const KEY_WINDOW_MS = 1_000;

export interface RowMenu {
  /** The id of the row whose menu is open, or `null`. */
  readonly openId: string | null;
  /** Spread on each `ContextMenu` root. */
  readonly rootProps: (id: string) => { open: boolean; onOpenChange: (open: boolean) => void };
  /** Put on each trigger (the row). */
  readonly onContextMenu: (event: MouseEvent<HTMLElement>) => void;
  /** Put on the list container, so the keyboard route is recognised. */
  readonly onKeyDown: (event: KeyboardEvent<HTMLElement>) => void;
}

export function useRowMenu(): RowMenu {
  const [openId, setOpenId] = useState<string | null>(null);
  const requested = useRef(false);
  const menuKeyAt = useRef(Number.NEGATIVE_INFINITY);

  const onKeyDown = useCallback((event: KeyboardEvent<HTMLElement>) => {
    if (event.key === 'ContextMenu' || (event.key === 'F10' && event.shiftKey)) {
      menuKeyAt.current = performance.now();
    }
  }, []);

  const onContextMenu = useCallback((event: MouseEvent<HTMLElement>) => {
    const fromKeyboard = performance.now() - menuKeyAt.current < KEY_WINDOW_MS;
    if (event.button !== 2 && !fromKeyboard) {
      event.preventDefault();
      return;
    }
    // The keyboard route has no position, which Radix places at (0, 0) — the
    // window's corner. Send it again from just under the row instead.
    if (fromKeyboard && event.clientX === 0 && event.clientY === 0) {
      event.preventDefault();
      const target = event.currentTarget;
      const rect = target.getBoundingClientRect();
      target.dispatchEvent(
        new globalThis.MouseEvent('contextmenu', {
          bubbles: true,
          cancelable: true,
          clientX: Math.max(1, rect.left + 24),
          clientY: Math.max(1, rect.bottom),
        }),
      );
      return;
    }
    requested.current = true;
    menuKeyAt.current = Number.NEGATIVE_INFINITY;
  }, []);

  const rootProps = useCallback(
    (id: string) => ({
      open: openId === id,
      onOpenChange: (open: boolean) => {
        if (open) {
          if (!requested.current) return;
          requested.current = false;
          setOpenId(id);
          return;
        }
        // A right click on another row opens its menu before the old one
        // reports closing; that late close must not shut the new one.
        setOpenId((current) => (current === id ? null : current));
      },
    }),
    [openId],
  );

  return { openId, rootProps, onContextMenu, onKeyDown };
}
