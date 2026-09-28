import { PanelLeftClose, PanelLeftOpen, Settings } from 'lucide-react';
import { useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { cn, focusRing, Tooltip, useReducedMotion } from '@vitals/ui';

import { navItems, type RouteId } from './navigation';
import { SHELL_NS } from './strings';

export interface SidebarProps {
  readonly active: RouteId;
  readonly onNavigate: (route: RouteId) => void;
  readonly collapsed: boolean;
  readonly onToggleCollapsed: () => void;
  readonly onOpenSettings: () => void;
}

const EXPANDED_WIDTH = 224;
const COLLAPSED_WIDTH = 52;

interface IndicatorBox {
  readonly top: number;
  readonly height: number;
}

/**
 * Where the active item sits inside the list, in list coordinates.
 *
 * Measured rather than computed from an index × row height, so a font scale,
 * a zoom level or a future group heading cannot put the pill beside the wrong
 * item.
 */
function useIndicator(
  list: React.RefObject<HTMLUListElement | null>,
  active: string,
  collapsed: boolean,
): IndicatorBox | null {
  const [box, setBox] = useState<IndicatorBox | null>(null);

  useLayoutEffect(() => {
    const root = list.current;
    if (root === null) return;
    const measure = (): void => {
      const button = root.querySelector<HTMLElement>(`[data-nav-item="${active}"]`);
      if (button === null) return;
      // Bounding rects, not `offsetTop`: each `<li>` is its own positioned
      // box (the tooltip anchor needs it), so `offsetTop` is always 0 and
      // the pill sat on the first item whatever was active. Live, 2026-09-27.
      const list = root.getBoundingClientRect();
      const item = button.getBoundingClientRect();
      const next = {
        top: Math.round(item.top - list.top + root.scrollTop),
        height: Math.round(item.height),
      };
      setBox((current) =>
        current?.top === next.top && current.height === next.height ? current : next,
      );
    };
    measure();
    // The sidebar width animates on collapse and rows can reflow with it.
    if (typeof ResizeObserver === 'undefined') return;
    const observer = new ResizeObserver(measure);
    observer.observe(root);
    return () => {
      observer.disconnect();
    };
  }, [list, active, collapsed]);

  return box;
}

export function Sidebar({
  active,
  onNavigate,
  collapsed,
  onToggleCollapsed,
  onOpenSettings,
}: SidebarProps) {
  const { t } = useTranslation();
  const { t: ts } = useTranslation(SHELL_NS);
  const listRef = useRef<HTMLUListElement>(null);
  const indicator = useIndicator(listRef, active, collapsed);
  const reduced = useReducedMotion();
  // The first placement is not a movement: sliding in from the top on launch
  // would animate something the user never asked to change.
  const [placed, setPlaced] = useState(false);
  const hasIndicator = indicator !== null;
  useLayoutEffect(() => {
    if (!hasIndicator || placed) return;
    // A frame later, so the transition class arrives after the first
    // position has been painted rather than in the same style pass.
    const frame = requestAnimationFrame(() => {
      setPlaced(true);
    });
    return () => {
      cancelAnimationFrame(frame);
    };
  }, [hasIndicator, placed]);
  const animate = placed && !reduced;

  /**
   * Roving arrow-key navigation.
   *
   * Twelve destinations means twelve Tab stops if each link is independently
   * focusable, and a keyboard user reaching the content has to pass all of
   * them on every visit. The WAI-ARIA practice for a navigation list is one
   * stop for the group with arrows inside it, which is what this implements:
   * only the active item carries `tabIndex={0}`.
   *
   * Focus moves without selecting. Activating on arrow would mount and start a
   * sampler for every section the user passes through on the way to the one
   * they wanted.
   */
  function onKeyDown(event: KeyboardEvent<HTMLUListElement>) {
    const keys = ['ArrowDown', 'ArrowUp', 'Home', 'End'];
    if (!keys.includes(event.key)) return;

    const buttons = Array.from(
      listRef.current?.querySelectorAll<HTMLButtonElement>('[data-nav-item]') ?? [],
    );
    if (buttons.length === 0) return;

    const current = buttons.findIndex((button) => button === document.activeElement);
    const from =
      current === -1 ? buttons.findIndex((b) => b.dataset['navItem'] === active) : current;

    let next: number;
    switch (event.key) {
      case 'ArrowDown':
        next = (from + 1) % buttons.length;
        break;
      case 'ArrowUp':
        next = (from - 1 + buttons.length) % buttons.length;
        break;
      case 'Home':
        next = 0;
        break;
      default:
        next = buttons.length - 1;
    }

    event.preventDefault();
    buttons[next]?.focus();
  }

  return (
    <nav
      aria-label={t('a11y.mainNavigation')}
      className="surface-chrome flex shrink-0 flex-col border-r border-[var(--color-border-subtle)] transition-[width] duration-(--duration-normal) ease-(--ease-out-quart)"
      style={{ width: collapsed ? COLLAPSED_WIDTH : EXPANDED_WIDTH }}
    >
      <ul
        ref={listRef}
        onKeyDown={onKeyDown}
        className="relative flex min-h-0 flex-1 flex-col gap-0.5 overflow-x-hidden overflow-y-auto p-2"
      >
        {/*
         * One indicator for the whole list, always mounted, moved by
         * transform. It morphs between items — stretching on the spring
         * overshoot — rather than one fading out and another fading in.
         *
         * Deliberately not a shared Motion `layoutId` on each item: that is
         * the React 19 pattern that throws `removeChild` when the active item
         * changes, because the old node is gone before the layout animation
         * reads it.
         *
         * A shape and not only a colour (the bar on its leading edge), so the
         * active item is identifiable without distinguishing the accent tint.
         */}
        {indicator !== null && (
          <li
            aria-hidden="true"
            data-nav-indicator
            className={cn(
              'pointer-events-none absolute inset-x-2 top-0 rounded-[var(--radius-control)]',
              // Contained: the list scrolls, so it clips at its 8 px padding.
              'bg-[var(--color-accent-subtle)] shadow-[var(--glow-accent-contained)]',
              animate &&
                'transition-[transform,height] duration-(--duration-slow) ease-(--ease-spring)',
            )}
            style={{
              transform: `translateY(${indicator.top}px)`,
              height: indicator.height,
            }}
          >
            <span className="absolute top-1/2 left-0 h-5 w-[3px] -translate-y-1/2 rounded-r-full bg-[var(--color-accent)] shadow-[0_0_8px_var(--color-accent)]" />
          </li>
        )}
        {navItems.map((item) => {
          const label = t(item.labelKey);
          const isActive = item.id === active;
          const Icon = item.icon;

          const button = (
            <button
              type="button"
              data-nav-item={item.id}
              // Collapsed, only the icon renders, so the visible text that
              // normally names the button is gone. The tooltip is not a
              // substitute: it names the trigger via `aria-describedby`, which
              // is a description, not a name.
              aria-label={collapsed ? label : undefined}
              // `aria-current` rather than `aria-selected`: these are links to
              // locations, not options in a listbox, and `aria-selected` on a
              // button is ignored by every screen reader.
              aria-current={isActive ? 'page' : undefined}
              tabIndex={isActive ? 0 : -1}
              onClick={() => onNavigate(item.id)}
              className={cn(
                'group relative flex h-9 w-full items-center gap-2.5 rounded-[var(--radius-control)] px-2.5 text-left',
                'transition-colors duration-(--duration-fast)',
                collapsed && 'justify-center px-0',
                isActive
                  ? 'text-[var(--color-accent)]'
                  : 'text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-inset)]/70 hover:text-[var(--color-fg-default)]',
                focusRing,
                'focus-visible:-outline-offset-2',
              )}
            >
              <Icon
                aria-hidden="true"
                className={cn(
                  'size-4 shrink-0 transition-transform duration-(--duration-normal) ease-(--ease-spring)',
                  'group-hover:scale-110',
                  isActive && 'drop-shadow-[0_0_6px_var(--color-accent)]',
                )}
              />
              {!collapsed && <span className="truncate text-sm font-medium">{label}</span>}
            </button>
          );

          return (
            <li key={item.id} className="relative">
              {/* Collapsed, the icon is the only label there is, so the
                  tooltip stops being decoration and becomes the name. */}
              {collapsed ? (
                <Tooltip content={label} side="right">
                  {button}
                </Tooltip>
              ) : (
                button
              )}
            </li>
          );
        })}
      </ul>

      <div
        className={cn(
          'flex shrink-0 items-center gap-1 border-t border-[var(--color-border-subtle)] p-2',
          collapsed && 'flex-col',
        )}
      >
        <SidebarAction
          label={ts('sidebar.openSettings')}
          collapsed={collapsed}
          onClick={onOpenSettings}
          icon={<Settings aria-hidden="true" className="size-4 shrink-0" />}
        >
          {t('nav.settings')}
        </SidebarAction>
        <SidebarAction
          label={collapsed ? ts('sidebar.expand') : ts('sidebar.collapse')}
          collapsed
          onClick={onToggleCollapsed}
          icon={
            collapsed ? (
              <PanelLeftOpen aria-hidden="true" className="size-4 shrink-0" />
            ) : (
              <PanelLeftClose aria-hidden="true" className="size-4 shrink-0" />
            )
          }
        />
      </div>
    </nav>
  );
}

function SidebarAction({
  label,
  collapsed,
  onClick,
  icon,
  children,
}: {
  readonly label: string;
  readonly collapsed: boolean;
  readonly onClick: () => void;
  readonly icon: React.ReactNode;
  readonly children?: React.ReactNode;
}) {
  const showText = !collapsed && children !== undefined;

  const button = (
    <button
      type="button"
      aria-label={showText ? undefined : label}
      onClick={onClick}
      className={cn(
        'flex h-8 items-center gap-2.5 rounded-[var(--radius-control)] text-[var(--color-fg-muted)]',
        'transition-colors duration-(--duration-fast)',
        'hover:bg-[var(--color-bg-inset)] hover:text-[var(--color-fg-default)]',
        showText ? 'flex-1 px-2.5' : 'w-8 justify-center',
        focusRing,
        'focus-visible:-outline-offset-2',
      )}
    >
      {icon}
      {showText && <span className="truncate text-sm font-medium">{children}</span>}
    </button>
  );

  return showText ? (
    button
  ) : (
    <Tooltip content={label} side="right">
      {button}
    </Tooltip>
  );
}
