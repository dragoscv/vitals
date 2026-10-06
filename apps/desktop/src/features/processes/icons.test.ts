import { describe, expect, it, vi } from 'vitest';

import type { ProcessKey } from '@vitals/protocol';

import { createIconStore } from './icons';

const key = (pid: number): ProcessKey => ({ pid, startTime: pid * 10 });
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('createIconStore', () => {
  it('sends rows that appear in the same frame as one batch, in order', async () => {
    const fetch = vi.fn((keys: readonly ProcessKey[]) =>
      Promise.resolve(keys.map((k) => (k.pid === 2 ? null : `data:${k.pid}`))),
    );
    const store = createIconStore(fetch);
    store.request('1:10', key(1));
    store.request('2:20', key(2));
    store.request('3:30', key(3));
    await settle();

    expect(fetch).toHaveBeenCalledTimes(1);
    expect(store.get('1:10')).toBe('data:1');
    expect(store.get('2:20')).toBeNull();
    expect(store.get('3:30')).toBe('data:3');
  });

  it('never asks twice for the same process, including one with no icon', async () => {
    const fetch = vi.fn((keys: readonly ProcessKey[]) => Promise.resolve(keys.map(() => null)));
    const store = createIconStore(fetch);
    store.request('1:10', key(1));
    store.request('1:10', key(1));
    await settle();
    store.request('1:10', key(1));
    await settle();
    expect(fetch).toHaveBeenCalledTimes(1);
  });

  it('answers none rather than staying unknown when the host call fails', async () => {
    const store = createIconStore(() => Promise.reject(new Error('no host')));
    store.request('1:10', key(1));
    await settle();
    expect(store.get('1:10')).toBeNull();
  });

  it('forgets processes that have exited', async () => {
    const store = createIconStore((keys) => Promise.resolve(keys.map(() => 'data:x')));
    store.request('1:10', key(1));
    store.request('2:20', key(2));
    await settle();
    store.retain(new Set(['2:20']));
    expect(store.get('1:10')).toBeUndefined();
    expect(store.get('2:20')).toBe('data:x');
  });
});
