import { PanelLeftClose, PanelLeftOpen, Settings } from 'lucide-react';
import { useRef, type KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { cn, focusRing, Tooltip } from '@vitals/ui';

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
      className="surface-chrome duration-(--duration-normal) ease-(--ease-out-quart) flex shrink-0 flex-col border-r border-[var(--color-border-subtle)] transition-[width]"
      style={{ width: collapsed ? COLLAPSED_WIDTH : EXPANDED_WIDTH }}
    >
      <ul
        ref={listRef}
        onKeyDown={onKeyDown}
        className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto overflow-x-hidden p-2"
      >
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
                'group flex h-9 w-full items-center gap-2.5 rounded-[var(--radius-control)] px-2.5 text-left',
                'duration-(--duration-fast) transition-colors',
                collapsed && 'justify-center px-0',
                isActive
                  ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]'
                  : 'text-[var(--color-fg-muted)] hover:bg-[var(--color-bg-inset)] hover:text-[var(--color-fg-default)]',
                focusRing,
                'focus-visible:-outline-offset-2',
              )}
            >
              <Icon aria-hidden="true" className="size-4 shrink-0" />
              {!collapsed && <span className="truncate text-sm font-medium">{label}</span>}
            </button>
          );

          return (
            <li key={item.id} className="relative">
              {/*
               * The active marker is a shape, not only a colour: an accent tint
               * alone fails 1.4.1 for a user who cannot distinguish it, and the
               * graphite accent is barely a tint at all.
               */}
              {isActive && (
                <span
                  aria-hidden="true"
                  className="absolute left-0 top-1/2 h-5 w-0.5 -translate-y-1/2 rounded-r-full bg-[var(--color-accent)]"
                />
              )}
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
        'duration-(--duration-fast) transition-colors',
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
