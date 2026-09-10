/**
 * The dashboard layout, persisted.
 *
 * Stored separately from `AppSettings` rather than as another field on it.
 * The two have different shapes of change: settings are a flat record of
 * scalars edited one at a time in a dialog, while a layout is an ordered list
 * mutated by drag-like interactions that fire in bursts. Folding the list into
 * the settings object would mean every widget nudge rewrites the entire
 * settings file, and any settings write races the layout.
 *
 * Persistence is debounced for the same reason the settings store debounces:
 * holding the "move down" button emits a change per repeat, and each one is an
 * open, serialise and fsync in an app whose pitch is that it costs nothing.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { hasTauriHost } from '../../shell/host';
import {
  addWidget,
  defaultLayout,
  moveWidget,
  parseLayout,
  removeWidget,
  resizeWidget,
  type DashboardLayout,
  type WidgetId,
  type WidgetSize,
} from './widgets';

const FILE = 'dashboard.json';
const KEY = 'layout';
const WRITE_DEBOUNCE_MS = 250;

interface StoreLike {
  get<T>(key: string): Promise<T | undefined>;
  set(key: string, value: unknown): Promise<void>;
  save(): Promise<void>;
}

export interface LayoutBackend {
  load(): Promise<unknown>;
  save(value: unknown): Promise<void>;
}

/**
 * Used when there is no Tauri host.
 *
 * Forgetting silently is right in tests and `vite preview`: a persistence
 * error there would be noise obscuring whatever was actually being looked at.
 */
const noopBackend: LayoutBackend = {
  load: () => Promise.resolve(undefined),
  save: () => Promise.resolve(),
};

let cached: Promise<StoreLike> | null = null;

/**
 * Dynamic import, deliberately.
 *
 * A static `import` of the Tauri store plugin throws during module
 * initialisation in any context without the IPC, which takes the bundle down
 * before React mounts. That failure mode has already cost this app one
 * eternal splash screen; it is not repeated here.
 */
async function openStore(): Promise<StoreLike> {
  cached ??= import('@tauri-apps/plugin-store').then((module) =>
    module.load(FILE, { autoSave: false }),
  );
  return cached;
}

export function createLayoutBackend(): LayoutBackend {
  if (!hasTauriHost()) return noopBackend;

  return {
    async load() {
      const store = await openStore();
      return store.get<unknown>(KEY);
    },
    async save(value) {
      const store = await openStore();
      await store.set(KEY, value);
      await store.save();
    },
  };
}

export interface LayoutController {
  readonly layout: DashboardLayout;
  /** False until the stored layout has been read; the grid renders skeletons. */
  readonly hydrated: boolean;
  add: (id: WidgetId) => void;
  remove: (id: WidgetId) => void;
  resize: (id: WidgetId, size: WidgetSize) => void;
  move: (id: WidgetId, direction: 'up' | 'down') => void;
  reset: () => void;
}

export function useLayout(backend: LayoutBackend = createLayoutBackend()): LayoutController {
  const [layout, setLayout] = useState<DashboardLayout>(defaultLayout);
  const [hydrated, setHydrated] = useState(false);

  // The default argument allocates a fresh backend on every render, so the
  // value itself can never be a stable dependency. The ref is stable, and the
  // effects read `.current` at the moment they actually need it.
  // Written in an effect, never during render: React 19's concurrent
  // scheduler may discard a render, and `react-hooks/refs` forbids the
  // render-time write for exactly that reason.
  const backendRef = useRef(backend);
  useEffect(() => {
    backendRef.current = backend;
  });

  useEffect(() => {
    let cancelled = false;

    void backendRef.current
      .load()
      .then((raw) => {
        if (cancelled) return;
        setLayout(parseLayout(raw));
      })
      .catch((error: unknown) => {
        // Defaults, never a throw. A corrupt layout file must not be able to
        // prevent the dashboard from rendering — that turns a cosmetic problem
        // into an app the user cannot open.
        console.error('failed to read dashboard layout; using defaults', error);
      })
      .finally(() => {
        if (!cancelled) setHydrated(true);
      });

    return () => {
      cancelled = true;
    };
    // Runs once. `backend` is deliberately not a dependency: the default
    // argument constructs a new object every render, so depending on it would
    // re-read the file on every render forever. A ref keeps the current one
    // reachable without making it reactive.
  }, [backendRef]);

  useEffect(() => {
    if (!hydrated) return;

    const timer = setTimeout(() => {
      void backendRef.current.save(layout).catch((error: unknown) => {
        console.error('failed to save dashboard layout', error);
      });
    }, WRITE_DEBOUNCE_MS);

    return () => {
      clearTimeout(timer);
    };
  }, [layout, hydrated, backendRef]);

  const add = useCallback((id: WidgetId) => {
    setLayout((current) => addWidget(current, id));
  }, []);

  const remove = useCallback((id: WidgetId) => {
    setLayout((current) => removeWidget(current, id));
  }, []);

  const resize = useCallback((id: WidgetId, size: WidgetSize) => {
    setLayout((current) => resizeWidget(current, id, size));
  }, []);

  const move = useCallback((id: WidgetId, direction: 'up' | 'down') => {
    setLayout((current) => moveWidget(current, id, direction));
  }, []);

  const reset = useCallback(() => {
    setLayout(defaultLayout);
  }, []);

  return { layout, hydrated, add, remove, resize, move, reset };
}
