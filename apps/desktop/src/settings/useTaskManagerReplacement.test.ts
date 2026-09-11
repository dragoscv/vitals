import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { TaskManagerReplacement } from '@vitals/protocol';

import type * as HostModule from '../shell/host';

import {
  type TaskManagerReplacementApi,
  useTaskManagerReplacement,
} from './useTaskManagerReplacement';

vi.mock('../shell/host', async (importOriginal) => {
  const actual = await importOriginal<typeof HostModule>();
  return { ...actual, hasTauriHost: vi.fn(() => true) };
});

const { hasTauriHost } = await import('../shell/host');
const mockedHasHost = vi.mocked(hasTauriHost);

const off: TaskManagerReplacement = { enabled: false, replacedBy: null, path: null };
const ours: TaskManagerReplacement = {
  enabled: true,
  replacedBy: null,
  path: String.raw`C:\Program Files\Vitals\vitals-desktop.exe`,
};

function api(overrides: Partial<TaskManagerReplacementApi> = {}): TaskManagerReplacementApi {
  return {
    read: vi.fn(() => Promise.resolve(off)),
    write: vi.fn((enabled: boolean) => Promise.resolve(enabled ? ours : off)),
    ...overrides,
  };
}

beforeEach(() => {
  mockedHasHost.mockReturnValue(true);
});

describe('useTaskManagerReplacement', () => {
  it('reads the registry on mount instead of trusting a stored boolean', async () => {
    // The state is machine-wide and can change while Vitals is closed, so
    // the only acceptable source is the backend, asked every time.
    const a = api({ read: vi.fn(() => Promise.resolve(ours)) });
    const { result } = renderHook(() => useTaskManagerReplacement(a));

    expect(result.current.status).toBeNull();
    await waitFor(() => expect(result.current.status?.enabled).toBe(true));
    expect(a.read).toHaveBeenCalledTimes(1);
  });

  it('shows the state the backend reports after a write, not the state that was asked for', async () => {
    // A declined UAC prompt or a hook owned by another tool means the write
    // did not happen. If the switch optimistically flipped it would lie.
    const a = api({
      write: vi.fn(() => Promise.reject(new Error('the user dismissed the elevation prompt'))),
    });
    const { result } = renderHook(() => useTaskManagerReplacement(a));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    act(() => result.current.set(true));

    await waitFor(() => expect(result.current.busy).toBe(false));
    expect(result.current.status?.enabled).toBe(false);
    expect(result.current.error).toBe('the user dismissed the elevation prompt');
  });

  it('applies a successful write and clears the previous error', async () => {
    const write = vi
      .fn<TaskManagerReplacementApi['write']>()
      .mockRejectedValueOnce(new Error('first refused'))
      .mockResolvedValueOnce(ours);
    const a = api({ write });
    const { result } = renderHook(() => useTaskManagerReplacement(a));
    await waitFor(() => expect(result.current.status).not.toBeNull());

    act(() => result.current.set(true));
    await waitFor(() => expect(result.current.error).toBe('first refused'));

    act(() => result.current.set(true));
    await waitFor(() => expect(result.current.status?.enabled).toBe(true));
    expect(result.current.error).toBeNull();
    expect(write).toHaveBeenLastCalledWith(true);
  });

  it('never touches the backend in a browser and settles as off', async () => {
    mockedHasHost.mockReturnValue(false);
    const a = api();
    const { result } = renderHook(() => useTaskManagerReplacement(a));

    await waitFor(() => expect(result.current.status).not.toBeNull());
    expect(result.current.status?.enabled).toBe(false);
    expect(a.read).not.toHaveBeenCalled();
  });
});
