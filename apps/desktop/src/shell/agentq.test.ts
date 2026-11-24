/**
 * The rule that decides whether the AI page is worth offering, and what
 * happens to that answer when the probe cannot be read.
 *
 * The row this covers (S26-01) asks for one observable thing: the tab and its
 * sidebar entry exist only where agentq is installed. The failure that matters
 * is hiding too much — a monitor that takes away a page the user can actually
 * use is worse than one that shows an empty section — so the cases that must
 * stay visible are the interesting ones here.
 */

import { afterEach, describe, expect, it, vi } from 'vitest';

import type { AgentqStatus } from '@vitals/protocol';

import { agentqPageIsUseful, refreshAgentqVisibility } from './agentq';
import { useNavVisibility } from './navigation';

/** A Windows machine that has never had agentq and never run an AI client. */
const empty: AgentqStatus = { script: false, coord: false, clients: false, windows: true };

function status(over: Partial<AgentqStatus>): AgentqStatus {
  return { ...empty, ...over };
}

afterEach(() => {
  useNavVisibility.setState({ agentq: null });
});

describe('whether the AI page would show anything', () => {
  it('offers it when nothing is installed and this is the only copy of Vitals', () => {
    // Not a Windows-only page: the phone and the watch pair with a desktop,
    // and the page is where a second copy is answered rather than counted.
    expect(agentqPageIsUseful(status({ windows: false }))).toBe(true);
  });

  it('takes it away when agentq is absent, no client has run, and another copy exists', () => {
    expect(agentqPageIsUseful(empty)).toBe(false);
  });

  it('keeps it when the script and the state directory are both there', () => {
    expect(agentqPageIsUseful(status({ script: true, coord: true }))).toBe(true);
  });

  it('takes it away when the script is installed and has never been run', () => {
    // A script with no state directory is an install that has produced
    // nothing: the sections it feeds would be empty, so `script` alone is
    // deliberately not enough.
    expect(agentqPageIsUseful(status({ script: true }))).toBe(false);
  });

  it('takes it away when a state directory exists with no script to read it', () => {
    expect(agentqPageIsUseful(status({ coord: true }))).toBe(false);
  });

  it('keeps it when an AI client keeps its files here, even with no agentq', () => {
    expect(agentqPageIsUseful(status({ clients: true }))).toBe(true);
  });
});

describe('publishing that answer to the sidebar', () => {
  it('hides the page when the probe reports an empty Windows install', async () => {
    await refreshAgentqVisibility(() => Promise.resolve(empty));
    expect(useNavVisibility.getState().agentq).toBe(false);
  });

  it('shows the page when agentq is installed', async () => {
    await refreshAgentqVisibility(() => Promise.resolve(status({ script: true, coord: true })));
    expect(useNavVisibility.getState().agentq).toBe(true);
  });

  it('shows the page when the probe cannot be read at all', async () => {
    const read = vi.fn(() => Promise.reject(new Error('no host')));
    await refreshAgentqVisibility(read);
    expect(read).toHaveBeenCalledTimes(1);
    // Fail open: a read error is not evidence that agentq is absent.
    expect(useNavVisibility.getState().agentq).toBe(true);
  });

  it('shows the page when there is no Tauri host to ask', async () => {
    await refreshAgentqVisibility(() => Promise.resolve(null));
    expect(useNavVisibility.getState().agentq).toBe(true);
  });
});
