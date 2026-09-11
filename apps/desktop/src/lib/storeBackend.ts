/**
 * One durable key in one JSON file, backed by `@tauri-apps/plugin-store`.
 *
 * Shared by the settings store and the dashboard layout. They keep separate
 * files on purpose — a layout nudge must not rewrite the settings file — but
 * the open/load/save plumbing was copied between them, and the two copies had
 * already started to drift in their comments. One implementation here.
 *
 * The plugin is loaded lazily via a dynamic import so that a browser or test
 * context — where the Tauri IPC does not exist — never evaluates the plugin
 * module. A static import throws during module initialisation and takes the
 * whole bundle down before React mounts; that failure has already cost this
 * app one eternal splash screen.
 */

import { hasTauriHost } from '../shell/host';

interface StoreLike {
  get<T>(key: string): Promise<T | undefined>;
  set(key: string, value: unknown): Promise<void>;
  save(): Promise<void>;
}

export interface StoreBackend {
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
const noopBackend: StoreBackend = {
  load: () => Promise.resolve(undefined),
  save: () => Promise.resolve(),
};

/** One open store per file, however many backends point at it. */
const stores = new Map<string, Promise<StoreLike>>();

function openStore(file: string): Promise<StoreLike> {
  let store = stores.get(file);
  if (store === undefined) {
    store = import('@tauri-apps/plugin-store').then((module) =>
      module.load(file, { autoSave: false }),
    );
    stores.set(file, store);
  }
  return store;
}

export function createStoreBackend(file: string, key: string): StoreBackend {
  if (!hasTauriHost()) return noopBackend;

  return {
    async load() {
      const store = await openStore(file);
      return store.get<unknown>(key);
    },
    async save(value) {
      const store = await openStore(file);
      await store.set(key, value);
      // Explicit rather than `autoSave`: the plugin's auto-save debounce can
      // still be pending when the process exits, and a setting that silently
      // fails to survive a restart is worse than one that never saved at all.
      await store.save();
    },
  };
}
