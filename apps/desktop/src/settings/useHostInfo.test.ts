import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type * as HostModule from '../shell/host';

import { useHostInfo } from './useHostInfo';

vi.mock('../shell/host', async (importOriginal) => {
  const actual = await importOriginal<typeof HostModule>();
  return { ...actual, hasTauriHost: vi.fn(() => false) };
});

const invoke = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }));

const { hasTauriHost } = await import('../shell/host');
const mockedHasHost = vi.mocked(hasTauriHost);

beforeEach(() => {
  invoke.mockReset();
  mockedHasHost.mockReturnValue(false);
});

describe('useHostInfo', () => {
  it('settles immediately when there is no host, rather than loading forever', async () => {
    // The failure this guards: `pending` never clearing, which renders as a
    // skeleton that stays on screen and reads as a hang.
    const { result } = renderHook(() => useHostInfo());

    await waitFor(() => expect(result.current.pending).toBe(false));
    expect(result.current.unavailable).toBe(true);
    expect(result.current.info).toBeNull();
    expect(invoke).not.toHaveBeenCalled();
  });

  it('reports unavailable rather than throwing when the command is unsupported', async () => {
    // A platform with no backend answers `Unsupported`. That is a legitimate
    // answer, not a crash, and it must land in the same state as having no
    // host at all — otherwise the panel would need two ways to say nothing.
    mockedHasHost.mockReturnValue(true);
    invoke.mockRejectedValue({ kind: 'unsupported', message: 'no backend' });

    const { result } = renderHook(() => useHostInfo());

    await waitFor(() => expect(result.current.pending).toBe(false));
    expect(result.current.unavailable).toBe(true);
    expect(result.current.info).toBeNull();
  });

  it('surfaces the facts when the backend has them', async () => {
    mockedHasHost.mockReturnValue(true);
    invoke.mockResolvedValue({ hostname: 'DRAGOS', cpuModel: 'Intel Core i9-14900K' });

    const { result } = renderHook(() => useHostInfo());

    await waitFor(() => expect(result.current.info).not.toBeNull());
    expect(result.current.unavailable).toBe(false);
    expect(result.current.info?.hostname).toBe('DRAGOS');
    expect(invoke).toHaveBeenCalledWith('get_host_info');
  });

  it('asks once, not once per render', async () => {
    // The effect has an empty dependency list on purpose: these facts change
    // at most once per boot, and re-fetching on every render would spend a
    // registry walk and a CPUID sweep for a value that cannot have moved.
    mockedHasHost.mockReturnValue(true);
    invoke.mockResolvedValue({ hostname: 'DRAGOS' });

    const { rerender, result } = renderHook(() => useHostInfo());
    await waitFor(() => expect(result.current.info).not.toBeNull());

    rerender();
    rerender();

    expect(invoke).toHaveBeenCalledTimes(1);
  });
});
