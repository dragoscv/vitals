import { isTauri } from '@tauri-apps/api/core';

/**
 * Whether a Tauri host is present.
 *
 * Delegates to Tauri's own `isTauri()` rather than sniffing for a global. An
 * earlier version tested `'__TAURI_INTERNALS__' in globalThis`, which is false
 * in the shipped app — and because every Tauri-backed feature guards on this,
 * one wrong predicate silently disabled the caption buttons, settings
 * persistence and the version readout simultaneously, with no error anywhere
 * to point at the cause. There is exactly one supported way to ask this
 * question, so it is asked once, here.
 *
 * Importing `@tauri-apps/api/core` statically is safe: it only touches the
 * global at call time, and `lib/ready.ts` already depends on the same module.
 */
export function hasTauriHost(): boolean {
  return isTauri();
}
