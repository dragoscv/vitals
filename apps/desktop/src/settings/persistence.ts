/**
 * Durable settings storage.
 *
 * Backed by `@tauri-apps/plugin-store`, which writes JSON into the app's
 * config directory. Loaded lazily via a dynamic import so that a browser or
 * test context — where the Tauri IPC does not exist — never even evaluates the
 * plugin module. A static import would throw during module initialisation and
 * take the whole bundle down before React mounts.
 */

import { hasTauriHost } from '../shell/host';

const FILE = 'settings.json';
const KEY = 'app';

interface StoreLike {
  get<T>(key: string): Promise<T | undefined>;
  set(key: string, value: unknown): Promise<void>;
  save(): Promise<void>;
}

/**
 * One durable JSON value, loaded once and saved whole.
 *
 * Every persisted blob in the app — settings, dashboard layout — is one of
 * these keyed into the same `settings.json`. Sharing the file and the
 * `StoreLike` adapter means one lazy plugin import and one place where the
 * explicit-save rule below is enforced; a second copy of this in
 * `useLayout.ts` drifted for months.
 */
export interface KeyedBackend {
  load(): Promise<unknown>;
  save(value: unknown): Promise<void>;
}

export type SettingsBackend = KeyedBackend;

/**
 * Used when there is no Tauri host.
 *
 * Silently forgetting changes is the right failure here: the only contexts
 * without a host are tests and `vite preview`, and in both a persistence error
 * would be noise obscuring the thing actually being looked at.
 */
const noopBackend: KeyedBackend = {
  load: () => Promise.resolve(undefined),
  save: () => Promise.resolve(),
};

let cached: Promise<StoreLike> | null = null;

async function openStore(): Promise<StoreLike> {
  cached ??= import('@tauri-apps/plugin-store').then((module) =>
    module.load(FILE, { autoSave: false }),
  );
  return cached;
}

/** A backend for one key of the shared store. */
export function createKeyedBackend(key: string): KeyedBackend {
  if (!hasTauriHost()) return noopBackend;

  return {
    async load() {
      const store = await openStore();
      return store.get<unknown>(key);
    },
    async save(value) {
      const store = await openStore();
      await store.set(key, value);
      // Explicit rather than `autoSave`: the plugin's auto-save debounce can
      // still be pending when the process exits, and a setting that silently
      // fails to survive a restart is worse than one that never saved at all.
      await store.save();
    },
  };
}

/** The application settings blob. */
export function createBackend(): SettingsBackend {
  return createKeyedBackend(KEY);
}
