/**
 * Ctrl+K — go anywhere, do anything.
 *
 * The listbox is hand-rolled rather than taken from a menu primitive. A
 * combobox that filters as you type has to keep DOM focus in the input while
 * the *visual* selection moves through the list, which is exactly what
 * `aria-activedescendant` is for; a roving-tabindex menu moves real focus and
 * would take the caret out of the search field on every arrow press.
 */

import { Search } from 'lucide-react';
import {
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
} from 'react';
import { useTranslation } from 'react-i18next';

import { DialogContent, DialogRoot, Input, cn } from '@vitals/ui';

import { DIAGNOSE_EVENT, navItems, type RouteId } from './navigation';
import { SHELL_NS } from './strings';

// The "Why is my PC slow?" entry navigates to the dashboard and fires
// `DIAGNOSE_EVENT` (defined in `navigation.ts`, a leaf both sides import) so
// this lazy module never has to reach into the dashboard's state.
export { DIAGNOSE_EVENT };

export interface CommandPaletteProps {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly onNavigate: (route: RouteId) => void;
  readonly onOpenSettings: () => void;
  readonly onToggleHud: () => void;
}

interface Command {
  readonly id: string;
  readonly name: string;
  readonly description: string;
  /** Extra words the filter matches on, never rendered. */
  readonly keywords: string;
  readonly run: () => void;
}

/** Case- and accent-insensitive, so "Retea" finds "Rețea". */
function fold(value: string): string {
  return value
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '')
    .toLowerCase();
}

export function CommandPalette({
  open,
  onOpenChange,
  onNavigate,
  onOpenSettings,
  onToggleHud,
}: CommandPaletteProps) {
  const { t } = useTranslation();
  const { t: ts } = useTranslation(SHELL_NS);
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const listId = useId();
  const listRef = useRef<HTMLUListElement | null>(null);

  const close = useCallback(() => {
    onOpenChange(false);
  }, [onOpenChange]);

  const commands = useMemo<readonly Command[]>(() => {
    const routes: Command[] = navItems.map((item) => ({
      id: `route:${item.id}`,
      name: t(item.labelKey),
      description: ts('palette.goTo'),
      keywords: `${item.id} ${t(item.labelKey)}`,
      run: () => {
        onNavigate(item.id);
      },
    }));

    const actions: Command[] = [
      {
        id: 'action:settings',
        name: ts('palette.openSettings'),
        description: ts('palette.openSettingsHint'),
        keywords: 'settings preferences options setari preferinte optiuni',
        run: onOpenSettings,
      },
      {
        id: 'action:diagnose',
        name: ts('palette.diagnose'),
        description: ts('palette.diagnoseHint'),
        keywords: 'slow diagnose why lag incet lent diagnostic de ce',
        run: () => {
          // Navigate first: the dashboard has to be the visible route before
          // its dialog can mean anything, and on a cold session it also has to
          // mount before it can hear the event.
          onNavigate('dashboard');
          // Deferred a frame: the palette is itself a Radix dialog and is
          // closing in this same tick. A second dialog opened synchronously
          // inside that teardown was dismissed along with it — seen live,
          // where the dashboard heard the event and the verdict never showed.
          requestAnimationFrame(() => {
            window.dispatchEvent(new CustomEvent(DIAGNOSE_EVENT));
          });
        },
      },
      {
        id: 'action:hud',
        name: ts('palette.toggleHud'),
        description: ts('palette.toggleHudHint'),
        keywords: 'overlay hud always on top suprapunere',
        run: onToggleHud,
      },
    ];

    return [...routes, ...actions];
  }, [t, ts, onNavigate, onOpenSettings, onToggleHud]);

  const results = useMemo(() => {
    const needle = fold(query.trim());
    if (needle === '') return commands;
    return commands.filter((command) =>
      fold(`${command.name} ${command.keywords}`).includes(needle),
    );
  }, [commands, query]);

  // Clamp rather than reset: typing another character usually narrows the list
  // under a selection the user is already tracking, and jumping them back to
  // the top on every keystroke makes the list feel like it is fighting them.
  const index = results.length === 0 ? -1 : Math.min(active, results.length - 1);
  const activeCommand = index >= 0 ? results[index] : undefined;

  // No reset-on-open here: `AppShell` unmounts the palette when it closes, so
  // every opening starts from fresh state. Reopening into someone's last
  // search is the behaviour every palette gets complained about for, and
  // unmounting is the cheapest way to never do it.

  useEffect(() => {
    if (activeCommand === undefined) return;
    const element = listRef.current?.querySelector(`#${CSS.escape(activeCommand.id)}`);
    element?.scrollIntoView({ block: 'nearest' });
  }, [activeCommand]);

  const onKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      event.preventDefault();
      if (results.length === 0) return;
      const delta = event.key === 'ArrowDown' ? 1 : -1;
      // Wraps, because a list this short is faster to reach from either end.
      setActive((current) => {
        const from = Math.min(current, results.length - 1);
        return (from + delta + results.length) % results.length;
      });
      return;
    }

    if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      setActive(event.key === 'Home' ? 0 : results.length - 1);
      return;
    }

    if (event.key === 'Enter') {
      event.preventDefault();
      if (activeCommand === undefined) return;
      // Close first: a command that opens another dialog must not race this
      // one's exit, or Radix restores focus over the top of it.
      close();
      activeCommand.run();
    }
  };

  return (
    <DialogRoot open={open} onOpenChange={onOpenChange}>
      <DialogContent
        size="lg"
        title={ts('palette.title')}
        hideTitle
        closeLabel={ts('palette.close')}
        className="top-[12%] translate-y-0 p-0"
      >
        <Input
          autoFocus
          ariaLabel={ts('palette.title')}
          placeholder={ts('palette.placeholder')}
          leadingIcon={<Search />}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
          role="combobox"
          aria-expanded
          aria-controls={listId}
          aria-autocomplete="list"
          {...(activeCommand && { 'aria-activedescendant': activeCommand.id })}
        />

        <ul
          ref={listRef}
          id={listId}
          role="listbox"
          aria-label={ts('palette.title')}
          className="mt-2 max-h-72 overflow-y-auto"
        >
          {results.map((command, position) => (
            <li
              key={command.id}
              id={command.id}
              role="option"
              aria-selected={position === index}
              className={cn(
                'cursor-pointer rounded-[var(--radius-control)] px-2 py-1.5',
                position === index && 'bg-[var(--color-bg-subtle)]',
              )}
              // Pointer down, not click: the input must not lose focus first,
              // and `mousedown` is where that would happen.
              onPointerDown={(event) => {
                event.preventDefault();
                close();
                command.run();
              }}
              onPointerMove={() => {
                setActive(position);
              }}
            >
              <span className="block text-sm">{command.name}</span>
              <span className="block text-2xs text-[var(--color-fg-muted)]">
                {command.description}
              </span>
            </li>
          ))}
        </ul>

        {results.length === 0 && (
          <p role="status" className="px-2 py-4 text-2xs text-[var(--color-fg-muted)]">
            {ts('palette.noResults')}
          </p>
        )}
      </DialogContent>
    </DialogRoot>
  );
}
