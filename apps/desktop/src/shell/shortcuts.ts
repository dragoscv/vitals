/**
 * The one place a keyboard shortcut is described.
 *
 * The registry is the source of truth for both the handler and the help
 * sheet, so a shortcut cannot exist without being documented and cannot be
 * documented without existing. Those two used to drift in every application
 * that keeps them apart, and a test here asserts every entry reaches the
 * sheet.
 */

import { navItems, type RouteId } from './navigation';

/** What a shortcut can ask the shell to do. */
export interface ShortcutActions {
  readonly openPalette: () => void;
  readonly openHelp: () => void;
  readonly openSettings: () => void;
  readonly navigate: (route: RouteId) => void;
}

export interface Shortcut {
  readonly id: string;
  /**
   * The chord, as parts to be rendered as separate keys.
   *
   * Not a pre-joined string: the help sheet draws each part in its own `<kbd>`,
   * and a locale that joins them differently must be free to.
   */
  readonly keys: readonly string[];
  /** Key into the `shell` namespace, under `shortcuts.*`. */
  readonly labelKey: string;
  readonly matches: (event: ShortcutEvent) => boolean;
  readonly run: (actions: ShortcutActions) => void;
}

/** The parts of a `KeyboardEvent` a shortcut is allowed to look at. */
export type ShortcutEvent = Pick<
  KeyboardEvent,
  'key' | 'ctrlKey' | 'shiftKey' | 'altKey' | 'metaKey'
>;

/** Ctrl (or ⌘) held, and nothing else. */
function ctrlOnly(event: ShortcutEvent): boolean {
  return (event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey;
}

const numberedRoutes = navItems.slice(0, 9);

/**
 * Every shortcut the webview handles, in the order the help sheet lists them.
 */
export const shortcuts: readonly Shortcut[] = [
  {
    id: 'palette',
    keys: ['Ctrl', 'K'],
    labelKey: 'shortcuts.palette',
    matches: (event) => ctrlOnly(event) && event.key.toLowerCase() === 'k',
    run: (actions) => actions.openPalette(),
  },
  {
    id: 'help',
    // Shift+/ on most layouts. Matched on the produced character rather than
    // the physical key so it still works on a layout where ? lives elsewhere.
    keys: ['?'],
    labelKey: 'shortcuts.help',
    matches: (event) => !event.ctrlKey && !event.metaKey && !event.altKey && event.key === '?',
    run: (actions) => actions.openHelp(),
  },
  {
    id: 'settings',
    keys: ['Ctrl', ','],
    labelKey: 'shortcuts.settings',
    matches: (event) => ctrlOnly(event) && event.key === ',',
    run: (actions) => actions.openSettings(),
  },
  {
    id: 'sections',
    keys: ['Ctrl', '1', '\u2013', '9'],
    labelKey: 'shortcuts.sections',
    matches: (event) => ctrlOnly(event) && /^[1-9]$/.test(event.key),
    run: () => {
      // Handled by `runShortcut`, which knows which digit was pressed. A
      // registry entry cannot, because it only describes the chord.
    },
  },
  {
    id: 'hud',
    keys: ['Ctrl', 'Shift', 'H'],
    labelKey: 'shortcuts.hud',
    matches: (event) =>
      (event.ctrlKey || event.metaKey) &&
      event.shiftKey &&
      !event.altKey &&
      event.key.toLowerCase() === 'h',
    // The overlay toggle is ALSO bound in `lib/useHud.ts`, which owns the
    // round trip to the backend and the settings write-back. This entry exists
    // so the chord appears in the help sheet from the same source of truth as
    // every other one; `runShortcut` deliberately does not act on it, to avoid
    // toggling twice. There is no Tauri global shortcut for it — verified by
    // grepping `src-tauri` for `global_shortcut` — so it only works while the
    // window has focus.
    run: () => {},
  },
];

/**
 * Whether a keystroke should be ignored because the user is typing.
 *
 * A palette that opens on Ctrl+K while someone is halfway through a filter
 * query is merely annoying; `?` stealing a character out of a search box makes
 * the box unusable, and that is the one this mainly exists for.
 */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (target === null || !(target instanceof Element)) return false;
  const element = target as HTMLElement;
  if (element.isContentEditable) return true;
  const tag = element.tagName;
  if (tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if (tag !== 'INPUT') return false;
  // Checkboxes, radios and buttons produce no text, so a shortcut over one is
  // not interrupting anything.
  const type = (element as HTMLInputElement).type;
  return !['checkbox', 'radio', 'button', 'submit', 'reset', 'range'].includes(type);
}

/**
 * Runs whichever shortcut the event matches.
 *
 * Returns `true` when one was handled, so the caller can decide whether to
 * consume the event. Chord matching happens here rather than in the listener
 * so the whole mapping is testable without a DOM.
 */
export function runShortcut(event: ShortcutEvent, actions: ShortcutActions): boolean {
  for (const shortcut of shortcuts) {
    if (!shortcut.matches(event)) continue;

    if (shortcut.id === 'sections') {
      const index = Number(event.key) - 1;
      const item = numberedRoutes[index];
      if (item === undefined) return false;
      actions.navigate(item.id);
      return true;
    }

    // Bound elsewhere; see the comment on the entry. Reported as handled so
    // the browser's own Ctrl+Shift+H does not also fire.
    if (shortcut.id === 'hud') return true;

    shortcut.run(actions);
    return true;
  }

  return false;
}
