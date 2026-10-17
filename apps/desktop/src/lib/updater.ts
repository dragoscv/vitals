/**
 * The update flow, as a state machine.
 *
 * Every state a user can be in is named here, including the two that are
 * easiest to get wrong. `unconfigured` exists because a build without a
 * signing key, and a build running in a browser tab, both *cannot* check for
 * updates — and reporting that as `upToDate` is a lie that hides a broken
 * release pipeline for as long as nobody happens to look at GitHub. The panel
 * says so plainly instead.
 *
 * `error` is kept separate from `unconfigured` for the same reason in
 * reverse: a network failure is temporary and worth retrying, a missing
 * public key is not.
 */

import { hasTauriHost } from '../shell/host';

import type * as Core from '@tauri-apps/api/core';

/**
 * The IPC module, loaded once. The panel asks the backend three things at
 * mount; three concurrent dynamic imports of the same module are three
 * chances for a bundler or test runner to resolve them differently.
 */
let coreModule: Promise<typeof Core> | null = null;
function core(): Promise<typeof Core> {
  coreModule ??= import('@tauri-apps/api/core');
  return coreModule;
}

export type UpdateState =
  | { readonly kind: 'idle' }
  | { readonly kind: 'checking' }
  | { readonly kind: 'upToDate' }
  | { readonly kind: 'available'; readonly version: string; readonly notes: string | null }
  | { readonly kind: 'downloading'; readonly version: string; readonly percent: number | null }
  | { readonly kind: 'ready'; readonly version: string }
  | { readonly kind: 'installing'; readonly version: string }
  | { readonly kind: 'error'; readonly message: string }
  | { readonly kind: 'unconfigured' };

/**
 * A handle on a pending update, narrowed to what this module needs.
 *
 * Declared structurally rather than imported so the plugin module is only
 * ever loaded dynamically — a static import would pull the Tauri IPC shim
 * into the main bundle for a panel most sessions never open, and would throw
 * on the browser path this file is explicitly meant to survive.
 */
interface PendingUpdate {
  readonly version: string;
  readonly body?: string | undefined;
  download(onEvent?: (event: DownloadEvent) => void): Promise<void>;
  install(): Promise<void>;
}

type DownloadEvent =
  | { readonly event: 'Started'; readonly data: { readonly contentLength?: number | undefined } }
  | { readonly event: 'Progress'; readonly data: { readonly chunkLength: number } }
  | { readonly event: 'Finished' };

/**
 * Errors that mean "this build cannot ever check", not "this attempt failed".
 *
 * Matched on the message because the plugin surfaces them as plain strings.
 * Deliberately broad: a false `unconfigured` shows an honest sentence, while
 * a false `error` shows a retry button that can never succeed.
 */
function isConfigurationFailure(message: string): boolean {
  const lowered = message.toLowerCase();
  return (
    lowered.includes('pubkey') ||
    lowered.includes('public key') ||
    lowered.includes('signature') ||
    lowered.includes('minisign') ||
    lowered.includes('updater is not configured') ||
    lowered.includes('updater plugin') ||
    lowered.includes('not configured')
  );
}

function messageOf(error: unknown): string {
  if (error instanceof Error) return error.message;
  return typeof error === 'string' ? error : JSON.stringify(error);
}

/**
 * Whether this build can meaningfully ask about updates at all.
 *
 * A dev build has no installer to replace and no signed manifest to verify,
 * so asking produces a confusing error rather than an answer.
 */
export function updatesAvailableInThisBuild(): boolean {
  return hasTauriHost() && !import.meta.env.DEV;
}

/**
 * Ask the endpoint whether there is something newer.
 *
 * Never throws. Every failure is a state, because the caller is a panel and a
 * panel has nowhere to rethrow to.
 */
export async function checkForUpdate(): Promise<
  | { readonly state: UpdateState; readonly update: null }
  | { readonly state: UpdateState; readonly update: PendingUpdate }
> {
  if (!updatesAvailableInThisBuild()) {
    return { state: { kind: 'unconfigured' }, update: null };
  }

  try {
    const { check } = await import('@tauri-apps/plugin-updater');
    const update = (await check()) as PendingUpdate | null;

    if (update === null) return { state: { kind: 'upToDate' }, update: null };

    const body = update.body;
    return {
      state: {
        kind: 'available',
        version: update.version,
        notes: body !== undefined && body.trim() !== '' ? body : null,
      },
      update,
    };
  } catch (error) {
    const message = messageOf(error);
    return {
      state: isConfigurationFailure(message)
        ? { kind: 'unconfigured' }
        : { kind: 'error', message },
      update: null,
    };
  }
}

/**
 * Download and install, reporting progress as a percentage.
 *
 * The percentage is `null` until the server has told us how big the download
 * is: some endpoints omit `Content-Length`, and a bar that jumps to a
 * fabricated number is worse than an honest indeterminate one. Same rule as
 * every other reading in this application — unmeasured is not zero.
 *
 * Download and install are two calls so the backend can prepare the setup
 * in between (DEV-3): a per-user copy's setup is told to run without asking
 * for administrator rights, and the attempt is recorded so the next launch
 * can say whether it worked.
 */
export async function downloadAndInstall(
  update: PendingUpdate,
  onProgress: (percent: number | null) => void,
): Promise<UpdateState> {
  let total: number | null = null;
  let received = 0;

  try {
    await update.download((event) => {
      switch (event.event) {
        case 'Started': {
          const length = event.data.contentLength;
          total = length !== undefined && length > 0 ? length : null;
          onProgress(null);
          break;
        }
        case 'Progress': {
          received += event.data.chunkLength;
          onProgress(total === null ? null : Math.min(100, (received / total) * 100));
          break;
        }
        case 'Finished': {
          onProgress(100);
          break;
        }
      }
    });
    const { invoke } = await core();
    await invoke('prepare_update_install', { version: update.version });
    await update.install();
    return { kind: 'ready', version: update.version };
  } catch (error) {
    return { kind: 'error', message: messageOf(error) };
  }
}

/**
 * Restart into the newly installed version.
 *
 * Returns an error state rather than throwing: if the relaunch fails the user
 * is still sitting in the old app and needs to be told, not left staring at a
 * button that did nothing.
 */
export async function restartIntoUpdate(): Promise<UpdateState | null> {
  try {
    const { relaunch } = await import('@tauri-apps/plugin-process');
    await relaunch();
    return null;
  } catch (error) {
    return { kind: 'error', message: messageOf(error) };
  }
}

/**
 * The version the background updater has downloaded and verified, waiting
 * to install when the app quits. `null` when there is none, or no host.
 */
export async function pendingBackgroundUpdate(): Promise<string | null> {
  if (!updatesAvailableInThisBuild()) return null;
  try {
    const { invoke } = await core();
    return await invoke<string | null>('get_pending_update');
  } catch {
    return null;
  }
}

/**
 * How quitting installs the waiting update. `needsApproval` is a copy in
 * Program Files run without administrator rights: it does not install on
 * quit, because the prompt would appear as the window vanished and nobody
 * would answer it. It is offered as "Restart to update" instead.
 */
export type QuitInstall = 'silent' | 'needsApproval' | 'notOurs';

export async function quitInstallMode(): Promise<QuitInstall> {
  if (!updatesAvailableInThisBuild()) return 'notOurs';
  try {
    const { invoke } = await core();
    return await invoke<QuitInstall>('get_update_install_mode');
  } catch {
    return 'silent';
  }
}

/**
 * Installs the background download now. The app exits on success, so a
 * resolved promise is only ever seen on failure paths' absence; a rejection
 * (a declined prompt) leaves the download in place for another try.
 */
export async function installBackgroundUpdateNow(): Promise<UpdateState | null> {
  try {
    const { invoke } = await core();
    await invoke('install_update_now');
    return null;
  } catch (error) {
    return { kind: 'error', message: messageOf(error) };
  }
}

/** What became of the last install attempt; reported once per attempt. */
export type UpdateOutcome =
  | { readonly kind: 'installed'; readonly version: string }
  | { readonly kind: 'failed'; readonly version: string; readonly error: string };

export async function takeUpdateOutcome(): Promise<UpdateOutcome | null> {
  if (!hasTauriHost()) return null;
  try {
    const { invoke } = await core();
    return await invoke<UpdateOutcome | null>('take_update_outcome');
  } catch {
    return null;
  }
}

/** An old Program Files copy left behind after a move to a per-user install. */
export async function leftoverInstall(): Promise<string | null> {
  if (!hasTauriHost()) return null;
  try {
    const { invoke } = await core();
    return await invoke<string | null>('get_leftover_install');
  } catch {
    return null;
  }
}

export async function removeLeftoverInstall(): Promise<void> {
  const { invoke } = await core();
  await invoke('remove_leftover_install');
}
