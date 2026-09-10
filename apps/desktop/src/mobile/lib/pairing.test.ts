import { describe, expect, it, vi } from 'vitest';

import { claimPairing, PAIRINGS_KEY, readPairings, tokenFromFragment } from './pairing';

class MemoryStorage implements Storage {
  private map = new Map<string, string>();
  get length() {
    return this.map.size;
  }
  clear() {
    this.map.clear();
  }
  getItem(key: string) {
    return this.map.get(key) ?? null;
  }
  key(index: number) {
    return Array.from(this.map.keys())[index] ?? null;
  }
  removeItem(key: string) {
    this.map.delete(key);
  }
  setItem(key: string, value: string) {
    this.map.set(key, value);
  }
}

function fakeLocation(hash: string): Location {
  return {
    hash,
    origin: 'http://192.168.1.20:7331',
    pathname: '/mobile.html',
    search: '',
  } as Location;
}

describe('claimPairing', () => {
  it('reads the token from the fragment, stores the pairing, and strips the fragment from the URL before validating', async () => {
    const storage = new MemoryStorage();
    const replaceState = vi.fn();
    const order: string[] = [];
    const probe = {
      health: vi.fn(async () => {
        order.push('health');
        return { ok: true, version: '0.1.0', modelVersion: 1 };
      }),
      host: vi.fn(async () => ({ hostname: 'DESK-01' }) as never),
    };

    const pairing = await claimPairing({
      location: fakeLocation('#t=SECRET-TOKEN'),
      history: {
        state: null,
        replaceState: (...args: unknown[]) => {
          order.push('strip');
          replaceState(...args);
        },
      } as unknown as History,
      storage,
      makeProbe: () => probe,
      now: () => 1234,
    });

    expect(pairing).toMatchObject({
      baseUrl: 'http://192.168.1.20:7331',
      token: 'SECRET-TOKEN',
      name: 'DESK-01',
      addedAt: 1234,
      readOnly: false,
    });
    expect(replaceState).toHaveBeenCalledWith(null, '', '/mobile.html');
    // The token must leave the address bar even if the PC never answers.
    expect(order).toEqual(['strip', 'health']);
    expect(readPairings(storage)).toHaveLength(1);
    expect(storage.getItem(PAIRINGS_KEY)).toContain('SECRET-TOKEN');
  });

  it('stores nothing when the PC rejects the token, so no dead card is left behind', async () => {
    const storage = new MemoryStorage();
    await expect(
      claimPairing({
        location: fakeLocation('#t=BAD'),
        history: { state: null, replaceState: () => {} } as unknown as History,
        storage,
        makeProbe: () => ({
          health: () => Promise.reject(new Error('401')),
          host: () => Promise.resolve(null),
        }),
        now: () => 0,
      }),
    ).rejects.toThrow();
    expect(readPairings(storage)).toEqual([]);
  });

  it('does nothing when the URL carries no pairing', async () => {
    const replaceState = vi.fn();
    const result = await claimPairing({
      location: fakeLocation(''),
      history: { state: null, replaceState } as unknown as History,
      storage: new MemoryStorage(),
      makeProbe: () => {
        throw new Error('must not probe');
      },
      now: () => 0,
    });
    expect(result).toBeUndefined();
    expect(replaceState).not.toHaveBeenCalled();
  });

  it('accumulates pairings for several PCs and replaces the token when the same PC is re-scanned', async () => {
    const storage = new MemoryStorage();
    const history = { state: null, replaceState: () => {} } as unknown as History;
    const probe = {
      health: () => Promise.resolve({ ok: true, version: '0', modelVersion: 1 }),
      host: () => Promise.resolve(null),
    };
    await claimPairing({
      location: fakeLocation('#t=ONE'),
      history,
      storage,
      makeProbe: () => probe,
      now: () => 1,
    });
    await claimPairing({
      location: { ...fakeLocation('#t=TWO'), origin: 'http://10.0.0.5:7331' },
      history,
      storage,
      makeProbe: () => probe,
      now: () => 2,
    });
    await claimPairing({
      location: fakeLocation('#t=THREE'),
      history,
      storage,
      makeProbe: () => probe,
      now: () => 3,
    });

    const stored = readPairings(storage);
    expect(stored.map((p) => p.token).sort()).toEqual(['THREE', 'TWO']);
  });
});

describe('tokenFromFragment', () => {
  it('decodes a percent-encoded token and ignores unrelated fragments', () => {
    expect(tokenFromFragment('#t=a%2Fb')).toBe('a/b');
    expect(tokenFromFragment('#section')).toBeUndefined();
    expect(tokenFromFragment('')).toBeUndefined();
  });
});
