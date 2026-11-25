/**
 * Feeds the nav visibility store from `get_cloud_status`.
 *
 * Its own small module so the shell can import it without pulling a screen
 * into the entry chunk; the command is a dynamic import, exactly as
 * `features/health/visibility.ts` does it for `health_status`.
 *
 * # Why the AI page needs a second fact
 *
 * The page's Projects tab shows a signed-in user their Codwen projects. The
 * existing `agentq` fact hides the whole page on a Windows machine with no
 * agentq and no AI client — but such a user may still have an account, and
 * hiding the page would make their projects unreachable. This fact says
 * whether an account is stored, and `isRouteVisible('agents')` shows the page
 * when either fact says there is something to see.
 *
 * # Cost
 *
 * `get_cloud_status` returns at once when no key is stored; only a signed-in
 * user pays one `GET /v1/hosts`, the same read the account card on the Cloud
 * tab already makes. It is refreshed on sign-in and sign-out so the sidebar
 * follows the account without a relaunch.
 */

import { useEffect } from 'react';

import { useNavVisibility } from '../../shell/navigation';

/** Injectable so a test supplies a status without a Tauri host. */
export type CloudStatusReader = () => Promise<{ signedIn: boolean } | null>;

async function tauriCloudStatus(): Promise<{ signedIn: boolean } | null> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<{ signedIn: boolean }>('get_cloud_status');
}

/**
 * Publishes whether a Codwen account is stored.
 *
 * A probe that could not be read is "not signed in": the page then shows only
 * on the agentq fact, which is the visibility it had before the Projects tab
 * existed. That is the safe direction — never invent an account.
 */
export async function refreshCodwenVisibility(
  read: CloudStatusReader = tauriCloudStatus,
): Promise<void> {
  let signedIn = false;
  try {
    signedIn = (await read())?.signedIn ?? false;
  } catch {
    // A probe that cannot be read is "not signed in"; the default above stands.
  }
  useNavVisibility.getState().setCodwen(signedIn);
}

/** Once per shell mount; refreshed again after a sign-in or a sign-out. */
export function useCodwenVisibility(read?: CloudStatusReader): void {
  useEffect(() => {
    void refreshCodwenVisibility(read);
  }, [read]);
}
