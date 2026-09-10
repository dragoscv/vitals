/**
 * A screen's view state — search text, sort, filter — mirrored into the URL.
 *
 * # Why the hash, and why it carries the route
 *
 * There is no router. Routes are state in the shell, restored from settings on
 * launch, and this hook must not take that over. So it uses the one part of
 * the URL nothing else in the desktop window reads: the fragment, in the shape
 * `#processes?q=chrome&sort=cpu`. The route segment is there so a screen can
 * tell whether the fragment is *its* state — every visited screen stays
 * mounted inside an `<Activity>`, and the Apps screen must not adopt a query
 * that was typed into Processes.
 *
 * # `replaceState`, never `pushState`
 *
 * A keystroke is not a navigation. Pushing an entry per character would fill
 * the back stack with `c`, `ch`, `chr`, `chro`, and the user's next Alt+Left
 * would walk back through their own typing. Writes are debounced as well, so
 * a fast typist produces one history mutation rather than fifteen.
 *
 * # Inert by default
 *
 * A screen showing its default view writes no fragment at all. A URL that
 * says `?q=&sort=name` on every screen is noise, and it would mean the mere
 * act of opening a screen mutates history. Only a deviation from the defaults
 * is worth recording; the moment the user clears back to them, the fragment
 * goes too.
 *
 * # Hidden screens do not write
 *
 * `<Activity mode="hidden">` tears down effects, and the write happens in an
 * effect. So only the visible screen ever touches the fragment, and a screen
 * that becomes visible again re-asserts its own state. A pending debounced
 * write is flushed synchronously when the effect is torn down, so hiding a
 * screen 50 ms after a keystroke does not lose that keystroke.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

/** Every value is a string: it lives in a URL. Unions of literals qualify. */
export type UrlStateShape = Record<string, string>;

/**
 * Turns a raw fragment value back into a typed one, or `null` to reject it.
 *
 * A URL is user-editable input. A screen whose sort is a closed union must not
 * end up with `sort: 'DROP TABLE'` because someone pasted a link — the parser
 * is where that is refused and the default reinstated.
 */
export type UrlStateParser<Value extends string> = (raw: string) => Value | null;

export type UrlStateParsers<Shape extends UrlStateShape> = {
  readonly [Key in keyof Shape]?: UrlStateParser<Shape[Key]>;
};

/** Builds a parser that accepts exactly the listed literals. */
export function oneOf<const Value extends string>(values: readonly Value[]): UrlStateParser<Value> {
  return (raw) => ((values as readonly string[]).includes(raw) ? (raw as Value) : null);
}

/** How long typing must pause before the URL is touched. */
export const URL_STATE_DEBOUNCE_MS = 150;

export interface ParsedHash {
  readonly route: string | null;
  readonly params: URLSearchParams;
}

/** `#processes?q=chrome` → `{ route: 'processes', params: {q: 'chrome'} }`. */
export function parseHash(hash: string): ParsedHash {
  const body = hash.startsWith('#') ? hash.slice(1) : hash;
  if (body === '') return { route: null, params: new URLSearchParams() };

  const separator = body.indexOf('?');
  if (separator === -1) return { route: body, params: new URLSearchParams() };

  return {
    route: body.slice(0, separator),
    params: new URLSearchParams(body.slice(separator + 1)),
  };
}

/**
 * The fragment for a route's state, or `''` when every value is a default.
 *
 * Keys are emitted in the order of `defaults`, not insertion order, so two
 * screens in the same state produce byte-identical URLs — the kind of
 * property that only matters the day someone diffs two bug reports.
 */
export function buildHash<Shape extends UrlStateShape>(
  route: string,
  state: Shape,
  defaults: Shape,
): string {
  const params = new URLSearchParams();
  for (const key of Object.keys(defaults)) {
    const value = state[key];
    if (value !== undefined && value !== defaults[key]) params.set(key, value);
  }

  const query = params.toString();
  return query === '' ? '' : `#${route}?${query}`;
}

/**
 * Reads the current fragment into a state object, if it belongs to `route`.
 *
 * Anything absent, unparseable or belonging to another screen resolves to the
 * default — a bad link degrades to the normal view, never to an error.
 */
export function readUrlState<Shape extends UrlStateShape>(
  route: string,
  defaults: Shape,
  parsers: UrlStateParsers<Shape>,
  hash: string,
): Shape {
  const parsed = parseHash(hash);
  if (parsed.route !== route) return defaults;

  const state: Record<string, string> = { ...defaults };
  for (const key of Object.keys(defaults)) {
    const raw = parsed.params.get(key);
    if (raw === null) continue;
    const parser = parsers[key];
    const value = parser === undefined ? raw : parser(raw);
    if (value !== null) state[key] = value;
  }
  return state as Shape;
}

function writeHash(hash: string): void {
  const current = window.location.hash;
  // Compared before writing: `replaceState` with an identical URL is still a
  // history mutation as far as the engine is concerned, and this effect runs
  // on every render in which the state object changed identity.
  if (current === hash || (current === '' && hash === '')) return;
  history.replaceState(history.state, '', `${location.pathname}${location.search}${hash}`);
}

/**
 * State that survives a reload and is shareable as a URL.
 *
 * Returns the current state and a `patch` function that merges. The initial
 * value is read from the fragment once, on mount; later external changes to
 * the fragment are not observed, because nothing in this window makes them.
 */
export function useUrlState<Shape extends UrlStateShape>(
  route: string,
  defaults: Shape,
  parsers: UrlStateParsers<Shape> = {},
): readonly [Shape, (patch: Partial<Shape>) => void] {
  const [state, setState] = useState<Shape>(() =>
    readUrlState(route, defaults, parsers, window.location.hash),
  );

  // Held in refs so the effect below depends only on `state`. Reconstructing
  // the write on every render of a parent would be harmless but would make
  // the dependency list lie about what actually triggers a write.
  const routeRef = useRef(route);
  const defaultsRef = useRef(defaults);
  routeRef.current = route;
  defaultsRef.current = defaults;

  // The write that the debounce has not yet performed, if any.
  const pending = useRef<string | null>(null);

  useEffect(() => {
    const hash = buildHash(routeRef.current, state, defaultsRef.current);
    pending.current = hash;
    const timer = setTimeout(() => {
      pending.current = null;
      writeHash(hash);
    }, URL_STATE_DEBOUNCE_MS);

    return () => {
      // Only the timer is cancelled here. Writing in this cleanup would run on
      // every keystroke — React tears an effect down before re-running it —
      // and turn the debounce into a no-op.
      clearTimeout(timer);
    };
  }, [state]);

  useEffect(
    () => () => {
      // Runs only when the screen is hidden or unmounted (an `<Activity>` in
      // hidden mode tears down effects). A keystroke followed within 150 ms
      // by a route change must not lose the keystroke, so whatever the
      // debounce still owes is written now.
      if (pending.current !== null) {
        writeHash(pending.current);
        pending.current = null;
      }
    },
    [],
  );

  const patch = useCallback((next: Partial<Shape>) => {
    setState((current) => {
      let changed = false;
      const merged: Record<string, string> = { ...current };
      // `Partial<Shape>` widens to `any` under `Object.entries`; the shape's
      // values are strings by construction, so this is a narrowing, not a lie.
      const entries = Object.entries(next) as [string, string | undefined][];
      for (const [key, value] of entries) {
        if (value !== undefined && merged[key] !== value) {
          merged[key] = value;
          changed = true;
        }
      }
      // Same identity when nothing changed, so the write effect does not fire.
      return changed ? (merged as Shape) : current;
    });
  }, []);

  return [state, patch] as const;
}
