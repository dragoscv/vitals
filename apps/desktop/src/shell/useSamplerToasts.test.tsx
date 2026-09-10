import { render } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { registerShellStrings } from './strings';
import { SAMPLER_ERROR_EVENT, useSamplerToasts } from './useSamplerToasts';

const hasHost = vi.hoisted(() => ({ value: true }));
vi.mock('./host', () => ({ hasTauriHost: () => hasHost.value }));

const listen = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/event', () => ({ listen }));

const toastError = vi.hoisted(() => vi.fn());
vi.mock('@vitals/ui/toast', () => ({ toast: { error: toastError } }));

let emit: ((event: { payload: string }) => void) | undefined;
const unlisten = vi.fn();

async function flush(): Promise<void> {
  for (let i = 0; i < 5; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

function Probe(): null {
  useSamplerToasts();
  return null;
}

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
  await i18n.changeLanguage('en');
});

beforeEach(() => {
  hasHost.value = true;
  emit = undefined;
  listen.mockReset();
  toastError.mockReset();
  unlisten.mockReset();
  listen.mockImplementation((_name: string, handler: typeof emit) => {
    emit = handler;
    return Promise.resolve(unlisten);
  });
});

describe('useSamplerToasts', () => {
  it('turns a sampler error into one toast that names the failure', async () => {
    render(<Probe />);
    await flush();
    expect(listen).toHaveBeenCalledWith(SAMPLER_ERROR_EVENT, expect.any(Function));

    emit?.({ payload: 'GPU counters unavailable' });
    await flush();

    expect(toastError).toHaveBeenCalledTimes(1);
    const [title, options] = toastError.mock.calls[0] as [string, { description: string }];
    expect(title).toBe('A reading failed');
    expect(options.description).toContain('GPU counters unavailable');
    expect(options.description).not.toContain('{{');
  });

  it('does not stack the same error every tick', async () => {
    // The sampler retries once a second; a failing subsystem would otherwise
    // produce a toast a second for as long as it stays broken.
    render(<Probe />);
    await flush();

    emit?.({ payload: 'same' });
    emit?.({ payload: 'same' });
    emit?.({ payload: 'different' });
    await vi.waitFor(() => {
      expect(toastError).toHaveBeenCalledTimes(2);
    });

    expect(toastError.mock.calls.map((c) => (c[1] as { id: string }).id)).toEqual([
      'sampler:same',
      'sampler:different',
    ]);
  });

  it('subscribes to nothing without a Tauri host', async () => {
    hasHost.value = false;
    render(<Probe />);
    await flush();
    expect(listen).not.toHaveBeenCalled();
  });

  it('drops the channel on unmount', async () => {
    const view = render(<Probe />);
    await flush();
    view.unmount();
    expect(unlisten).toHaveBeenCalled();
  });
});
