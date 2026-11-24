/**
 * Feeds the nav visibility store from `get_agentq_status`.
 *
 * Its own small module so the shell can import it without pulling a screen
 * into the entry chunk; the command is a dynamic import, exactly as the
 * Platform visibility module next door does it for `get_capabilities`.
 *
 * # Why the AI page can disappear at all
 *
 * The page shows two things: agentq's coordination state and the AI sessions
 * running on this machine. On a Windows machine that has never installed
 * agentq and has never run an AI client, both sections are empty, and an
 * always-empty destination is a lie about what the application does. The
 * probe below answers whether either exists, and only a definite "neither"
 * hides the item — an unreadable probe never does.
 *
 * The rule lives here rather than in the status type because Rust reports
 * facts and the shell decides what to show; a second definition in Rust would
 * be one more thing to keep in step for a one-line expression.
 */

import { useEffect } from 'react';

import type { AgentqStatus } from '@vitals/protocol';

import { useNavVisibility } from './navigation';

/** Injectable so tests supply a status without a Tauri host. */
export type AgentqStatusReader = () => Promise<AgentqStatus | null>;

async function tauriAgentqStatus(): Promise<AgentqStatus | null> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<AgentqStatus>('get_agentq_status');
}

/**
 * Whether the AI page would show anything.
 *
 * `script && coord` rather than `script` alone: `agentq.ps1` present with no
 * state directory is an install that has never been run, so the sections it
 * feeds are still empty. On another platform `windows` is false and the page
 * keeps listing sessions, so the item stays — the page is not only about
 * agentq.
 */
export function agentqPageIsUseful(status: AgentqStatus): boolean {
  const agentq = status.script && status.coord;
  return agentq || status.clients || !status.windows;
}

/**
 * Publishes whether the AI page is worth offering.
 *
 * A probe that could not be read fails open — the item stays visible — which
 * is the right direction for this fact: `null` already means visible, and a
 * page the user can actually use must never be taken away by a read error.
 */
export async function refreshAgentqVisibility(
  read: AgentqStatusReader = tauriAgentqStatus,
): Promise<void> {
  let present: boolean;
  try {
    const status = await read();
    present = status === null ? true : agentqPageIsUseful(status);
  } catch {
    present = true;
  }
  useNavVisibility.getState().setAgentq(present);
}

/**
 * Once per shell mount. Called again when agentq is installed while Vitals is
 * open is not attempted: an install rewrites the profile's bin folder, which
 * the app has no notification for, and a relaunch is a reasonable price for
 * the one case it covers.
 */
export function useAgentqVisibility(read?: AgentqStatusReader): void {
  useEffect(() => {
    void refreshAgentqVisibility(read);
  }, [read]);
}
