/**
 * Guards the two ways an alert list goes silently wrong.
 *
 * A window opened mid-episode must see the alert that is already raised — the
 * event only fires on a *change*, so without the initial read the panel says
 * "nothing needs your attention" while the disk is failing. And in a browser,
 * where there is no host at all, it must say the same thing without throwing
 * from inside a promise nobody awaits.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { Alert } from '@vitals/protocol';

import { ALERTS_EVENT, createTauriAlertSource } from './useAlerts';

const hasHost = vi.hoisted(() => ({ value: true }));

vi.mock('../../shell/host', () => ({
  hasTauriHost: () => hasHost.value,
}));

const invoke = vi.hoisted(() => vi.fn());
const listen = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({ invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen }));

/** Whatever the last `listen` call registered, so a test can fire an event. */
let emit: ((event: { payload: Alert[] }) => void) | undefined;
const unlisten = vi.fn();

function alert(overrides: Partial<Alert> = {}): Alert {
  return {
    kind: 'diskHealth',
    severity: 'critical',
    subject: 'C:',
    title: 'alert.diskHealth.title',
    cause: 'alert.diskHealth.cause',
    values: { disk: 'C:' },
    route: 'storage',
    sinceSample: 1,
    ...overrides,
  };
}

/**
 * Lets the dynamic imports inside `start()` settle.
 *
 * Microtask ticks are not enough: `import()` resolves on a real task even for
 * an already-mocked module, so draining the microtask queue returns before
 * anything has been called and every assertion fails for the wrong reason.
 */
async function flush(): Promise<void> {
  for (let i = 0; i < 5; i += 1) {
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

beforeEach(() => {
  hasHost.value = true;
  emit = undefined;
  invoke.mockReset();
  listen.mockReset();
  unlisten.mockReset();
  invoke.mockResolvedValue([]);
  listen.mockImplementation((_name: string, handler: typeof emit) => {
    emit = handler;
    return Promise.resolve(unlisten);
  });
});

describe('createTauriAlertSource', () => {
  it('starts empty, so nothing renders before the first answer', () => {
    expect(createTauriAlertSource().current()).toEqual([]);
  });

  it('reads the list that already exists when a window opens mid-episode', async () => {
    // The event fires on change only. Without this read, a window opened
    // while a disk is failing shows "nothing needs your attention".
    const raised = alert();
    invoke.mockResolvedValue([raised]);

    const source = createTauriAlertSource();
    source.subscribe(() => undefined);
    await flush();

    expect(invoke).toHaveBeenCalledWith('get_alerts');
    expect(source.current()).toEqual([raised]);
  });

  it('replaces the whole list when the event fires', async () => {
    const source = createTauriAlertSource();
    source.subscribe(() => undefined);
    await flush();

    // The name is the contract with `ALERTS_EVENT` in src-tauri; a typo here
    // is a channel that simply never delivers.
    expect(listen).toHaveBeenCalledWith(ALERTS_EVENT, expect.any(Function));

    const raised = alert({ kind: 'thermalCpu', subject: '' });
    emit?.({ payload: [raised] });

    expect(source.current()).toEqual([raised]);
  });

  it('notifies subscribers on an event, so the panel re-renders', async () => {
    let notified = 0;
    const source = createTauriAlertSource();
    source.subscribe(() => {
      notified += 1;
    });
    await flush();

    // From here, not from zero: the initial read publishes too, and this test
    // is about the event, not about how many times start-up settles.
    const before = notified;
    emit?.({ payload: [alert()] });
    expect(notified).toBe(before + 1);
  });

  it('does not let a late initial read overwrite a newer event', async () => {
    // The two race. The invoke's answer was true when it was asked; the event
    // is true now, and putting the list back a tick makes an alert flicker.
    let settle: ((value: Alert[]) => void) | undefined;
    invoke.mockReturnValue(
      new Promise<Alert[]>((resolve) => {
        settle = resolve;
      }),
    );

    const source = createTauriAlertSource();
    source.subscribe(() => undefined);
    await flush();

    const fresh = alert({ kind: 'batteryLow', subject: '' });
    emit?.({ payload: [fresh] });
    settle?.([alert({ kind: 'diskSpace' })]);
    await flush();

    expect(source.current()).toEqual([fresh]);
  });

  it('returns an empty list without touching IPC when there is no host', async () => {
    // `listen()` dereferences an internals global that does not exist in a
    // browser and throws from inside a promise nobody awaits.
    hasHost.value = false;

    const source = createTauriAlertSource();
    source.subscribe(() => undefined);
    await flush();

    expect(source.current()).toEqual([]);
    expect(invoke).not.toHaveBeenCalled();
    expect(listen).not.toHaveBeenCalled();
  });

  it('drops the channel when the last subscriber leaves', async () => {
    const source = createTauriAlertSource();
    const stop = source.subscribe(() => undefined);
    await flush();

    stop();
    expect(unlisten).toHaveBeenCalled();
  });

  it('reopens the channel when a subscriber returns after the last one left', async () => {
    // The metrics source had this exact defect and it cost an afternoon: a
    // stale `disposed` made the second subscription unlisten itself the
    // moment it resolved, so alerts silently froze after a route was hidden
    // and shown again.
    const source = createTauriAlertSource();
    const stop = source.subscribe(() => undefined);
    await flush();
    stop();
    unlisten.mockClear();

    source.subscribe(() => undefined);
    await flush();
    expect(unlisten).not.toHaveBeenCalled();

    const raised = alert({ kind: 'thermalCpu', subject: '' });
    emit?.({ payload: [raised] });
    expect(source.current()).toEqual([raised]);
  });

  it('keeps the same array identity while nothing changes', () => {
    // `useSyncExternalStore` compares by identity: a fresh `[]` per read is an
    // infinite render loop, not a wasted allocation.
    const source = createTauriAlertSource();
    expect(source.current()).toBe(source.current());
  });
});
