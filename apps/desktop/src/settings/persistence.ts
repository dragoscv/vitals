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

export interface SettingsBackend {
  load(): Promise<unknown>;
  save(value: unknown): Promise<void>;
}

/**
 * Used when there is no Tauri host.
 *
 * Silently forgetting changes is the right failure here: the only contexts
 * without a host are tests and `vite preview`, and in both a persistence error
 * would be noise obscuring the thing actually being looked at.
 */
const noopBackend: SettingsBackend = {
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

export function createBackend(): SettingsBackend {
  if (!hasTauriHost()) return noopBackend;

  return {
    async load() {
      const store = await openStore();
      return store.get<unknown>(KEY);
    },
    async save(value) {
      const store = await openStore();
      await store.set(KEY, value);
      // Explicit rather than `autoSave`: the plugin's auto-save debounce can
      // still be pending when the process exits, and a setting that silently
      // fails to survive a restart is worse than one that never saved at all.
      await store.save();
    },
  };
}
