import type { Alert } from '@vitals/protocol';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { StreamHandler } from '@vitals/client';

import { ALERTS_POLL_MS, LiveMachine, type MobileClient } from './live';

const ALERT: Alert = {
  kind: 'diskSpace',
  severity: 'warning',
  subject: 'C:',
  title: 'alert.diskSpace.title',
  cause: 'alert.diskSpace.cause',
  values: { disk: 'C:', percent: 4 },
  route: 'storage',
  sinceSample: 1,
};

/** A client whose stream is driven by hand, so the test decides when the PC connects or drops. */
function fakeClient(replies: () => Promise<Alert[]>) {
  let handler: StreamHandler | undefined;
  const alerts = vi.fn(replies);
  const client: MobileClient = {
    health: () => Promise.resolve({ ok: true, version: '0', modelVersion: 1 }),
    host: () => Promise.resolve(null),
    control: () => Promise.resolve(),
    alerts,
    stream: (h) => {
      handler = h;
      return () => {
        handler = undefined;
      };
    },
  };
  return {
    client,
    alerts,
    open: () => handler?.onOpen?.(),
    drop: () => handler?.onDrop?.(new Error('dropped') as never),
  };
}

/** Lets the mocked `alerts()` promise settle inside fake timers. */
async function settle(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe('LiveMachine alerts', () => {
  it('does not fetch alerts until the stream has connected', async () => {
    const fake = fakeClient(() => Promise.resolve([]));
    const machine = new LiveMachine(fake.client);
    machine.start();
    await vi.advanceTimersByTimeAsync(ALERTS_POLL_MS * 3);
    expect(fake.alerts).not.toHaveBeenCalled();
    expect(machine.get().alerts).toBeNull();
    machine.close();
  });

  it('fetches on connect, then every 15 s, and publishes the list', async () => {
    const fake = fakeClient(() => Promise.resolve([ALERT]));
    const machine = new LiveMachine(fake.client);
    machine.start();
    fake.open();
    await settle();
    expect(fake.alerts).toHaveBeenCalledTimes(1);
    expect(machine.get().alerts).toEqual([ALERT]);

    await vi.advanceTimersByTimeAsync(ALERTS_POLL_MS);
    expect(fake.alerts).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(ALERTS_POLL_MS);
    expect(fake.alerts).toHaveBeenCalledTimes(3);
    machine.close();
  });

  it('stops polling when the stream drops and does not apply a reply that lands afterwards', async () => {
    let resolve: ((alerts: Alert[]) => void) | undefined;
    const fake = fakeClient(
      () =>
        new Promise<Alert[]>((r) => {
          resolve = r;
        }),
    );
    const machine = new LiveMachine(fake.client);
    machine.start();
    fake.open();
    expect(fake.alerts).toHaveBeenCalledTimes(1);

    fake.drop();
    // The in-flight reply arrives after the drop: it belongs to a dead
    // connection and must not repaint the card as if the PC were reachable.
    resolve?.([ALERT]);
    await settle();
    expect(machine.get().alerts).toBeNull();

    await vi.advanceTimersByTimeAsync(ALERTS_POLL_MS * 3);
    expect(fake.alerts).toHaveBeenCalledTimes(1);
    machine.close();
  });

  it('keeps the last known list when a poll fails, rather than flashing all-clear', async () => {
    let fail = false;
    const fake = fakeClient(() =>
      fail ? Promise.reject(new Error('503')) : Promise.resolve([ALERT]),
    );
    const machine = new LiveMachine(fake.client);
    machine.start();
    fake.open();
    await settle();
    expect(machine.get().alerts).toEqual([ALERT]);

    fail = true;
    await vi.advanceTimersByTimeAsync(ALERTS_POLL_MS);
    expect(fake.alerts).toHaveBeenCalledTimes(2);
    expect(machine.get().alerts).toEqual([ALERT]);
    // And it keeps trying, so a recovered server is seen.
    await vi.advanceTimersByTimeAsync(ALERTS_POLL_MS);
    expect(fake.alerts).toHaveBeenCalledTimes(3);
    machine.close();
  });

  it('stops polling on close', async () => {
    const fake = fakeClient(() => Promise.resolve([]));
    const machine = new LiveMachine(fake.client);
    machine.start();
    fake.open();
    await settle();
    machine.close();
    await vi.advanceTimersByTimeAsync(ALERTS_POLL_MS * 3);
    expect(fake.alerts).toHaveBeenCalledTimes(1);
  });
});
