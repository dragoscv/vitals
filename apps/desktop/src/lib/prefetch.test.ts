import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import {
  clearPrefetched,
  peekPrefetched,
  prefetch,
  REUSE_WITHIN_MS,
  takePrefetched,
} from './prefetch';

beforeEach(() => {
  vi.useFakeTimers();
  clearPrefetched();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('prefetch', () => {
  it('lets a screen render the background read synchronously, before any effect runs', async () => {
    await prefetch('k', () => Promise.resolve({ rows: 3 }), 60_000);

    expect(peekPrefetched<{ rows: number }>('k')?.value).toEqual({ rows: 3 });
  });

  it('hands a young read to the first load and then forgets it, so a refresh is a real read', async () => {
    const load = vi.fn(() => Promise.resolve('first'));
    await prefetch('k', load, 60_000);

    await expect(takePrefetched('k')).resolves.toBe('first');
    expect(takePrefetched('k')).toBeUndefined();
    expect(peekPrefetched('k')).toBeUndefined();
    expect(load).toHaveBeenCalledTimes(1);
  });

  it('reuses a read still in flight rather than starting a second one', async () => {
    let resolve!: (value: string) => void;
    const load = vi.fn(() => new Promise<string>((r) => (resolve = r)));
    void prefetch('k', load, 60_000);

    const taken = takePrefetched<string>('k');
    resolve('late');

    await expect(taken).resolves.toBe('late');
    expect(load).toHaveBeenCalledTimes(1);
  });

  it('shows an older read but makes the screen read again, so launch data is never presented as current', async () => {
    await prefetch('k', () => Promise.resolve('old'), 10 * 60_000);
    vi.advanceTimersByTime(REUSE_WITHIN_MS + 1);

    expect(peekPrefetched('k')?.value).toBe('old');
    expect(takePrefetched('k')).toBeUndefined();
  });

  it('ignores an entry past its maximum age entirely', async () => {
    await prefetch('k', () => Promise.resolve('stale'), 1000);
    vi.advanceTimersByTime(1001);

    expect(peekPrefetched('k')).toBeUndefined();
  });

  it('drops a failed read, so the screen loads for itself and can explain the error', async () => {
    await prefetch('k', () => Promise.reject(new Error('denied')), 60_000);

    expect(peekPrefetched('k')).toBeUndefined();
    expect(takePrefetched('k')).toBeUndefined();
  });

  it('does not read twice for a key that is already fresh', async () => {
    const load = vi.fn(() => Promise.resolve(1));
    await prefetch('k', load, 60_000);
    await prefetch('k', load, 60_000);

    expect(load).toHaveBeenCalledTimes(1);
  });
});
