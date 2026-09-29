import { describe, expect, it } from 'vitest';

import {
  PairingStore,
  cleanCode,
  isCode,
  loadPairings,
  pairWithCode,
  pairWithToken,
  redeemCode,
  type Pairing,
} from './pairing';

const TOKEN = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ';
const BASE = 'http://192.168.1.20:7331';

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

/** A fake PC: routes by path, records every request. */
function pc(routes: Record<string, (init: RequestInit | undefined) => Response>) {
  const calls: { url: string; init: RequestInit | undefined }[] = [];
  const fetchImpl = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = input instanceof Request ? input.url : String(input);
    calls.push({ url, init });
    const path = new URL(url).pathname;
    const route = routes[path];
    if (route === undefined) return new Response(null, { status: 404 });
    return route(init);
  }) as typeof fetch;
  return { fetchImpl, calls };
}

const healthy = {
  '/api/v1/health': () => json(200, { ok: true, version: '0.9.0', modelVersion: 1 }),
  '/api/v1/host': () => json(200, { hostname: 'DESK' }),
};

describe('code cleaning', () => {
  it('keeps only digits, so the grouped code the PC shows can be typed as seen', () => {
    expect(cleanCode(' 482 913 ')).toBe('482913');
    expect(isCode('482 913')).toBe(true);
  });

  it('refuses seven digits instead of truncating them into a guess the user never made', () => {
    expect(isCode('4829131')).toBe(false);
    expect(isCode('48291')).toBe(false);
    expect(isCode('abcdef')).toBe(false);
  });
});

describe('redeemCode', () => {
  it('maps every refusal the server can give to the one message it means', async () => {
    const cases: [number, string][] = [
      [403, 'badCode'],
      [400, 'badCode'],
      [404, 'olderPc'],
      [500, 'failed'],
    ];
    for (const [status, reason] of cases) {
      const { fetchImpl } = pc({
        '/api/v1/pair': () => json(status, { error: 'invalid pairing code' }),
      });
      const result = await redeemCode(BASE, '482913', 'TV', fetchImpl);
      expect(result, String(status)).toEqual({ ok: false, reason, status });
    }
  });

  it('refuses a 200 whose token is not a real token rather than saving garbage', async () => {
    const { fetchImpl } = pc({
      '/api/v1/pair': () => json(200, { token: 'short', scope: 'control' }),
    });
    expect(await redeemCode(BASE, '482913', 'TV', fetchImpl)).toMatchObject({
      ok: false,
      reason: 'failed',
    });
  });

  it('reports an unreachable PC when the request never gets an answer', async () => {
    const fetchImpl = (async () => {
      throw new TypeError('Failed to fetch');
    }) as typeof fetch;
    expect(await redeemCode(BASE, '482913', 'TV', fetchImpl)).toEqual({
      ok: false,
      reason: 'unreachable',
    });
  });
});

describe('pairWithCode', () => {
  it('sends the code and label, then proves the token before saving the scope the PC chose', async () => {
    const { fetchImpl, calls } = pc({
      ...healthy,
      '/api/v1/pair': () => json(200, { token: TOKEN, scope: 'control' }),
    });
    const result = await pairWithCode('192.168.1.20', '482 913', 'Living room TV', [], fetchImpl);
    expect(result).toMatchObject({
      ok: true,
      pairing: { label: 'DESK', baseUrl: BASE, scope: 'control', token: TOKEN },
    });
    const pair = calls.find((c) => c.url.endsWith('/api/v1/pair'));
    expect(JSON.parse(pair?.init?.body as string)).toEqual({
      code: '482913',
      label: 'Living room TV',
    });
    const host = calls.find((c) => c.url.endsWith('/api/v1/host'));
    expect(new Headers(host?.init?.headers).get('Authorization')).toBe(`Bearer ${TOKEN}`);
  });

  it('never sends a code to a public address', async () => {
    const { fetchImpl, calls } = pc(healthy);
    expect(await pairWithCode('8.8.8.8', '482913', 'TV', [], fetchImpl)).toEqual({
      ok: false,
      reason: 'notPrivate',
    });
    expect(calls).toHaveLength(0);
  });

  it('refuses a PC that speaks a model version this app does not understand', async () => {
    const { fetchImpl } = pc({
      '/api/v1/health': () => json(200, { ok: true, version: '2.0.0', modelVersion: 2 }),
      '/api/v1/pair': () => json(200, { token: TOKEN, scope: 'read' }),
    });
    expect(await pairWithCode(BASE, '482913', 'TV', [], fetchImpl)).toEqual({
      ok: false,
      reason: 'incompatible',
    });
  });
});

describe('pairWithToken', () => {
  it('reports a token the PC rejects as unauthorised, not as unreachable', async () => {
    const { fetchImpl } = pc({
      '/api/v1/health': healthy['/api/v1/health'],
      '/api/v1/host': () => json(401, { error: 'unauthorised' }),
    });
    expect(await pairWithToken(BASE, TOKEN, [], fetchImpl)).toMatchObject({
      ok: false,
      reason: 'unauthorised',
    });
  });

  it('replaces the existing entry when the same PC is paired again', async () => {
    const { fetchImpl } = pc(healthy);
    const old: Pairing = {
      id: 'x',
      label: 'Old',
      baseUrl: BASE,
      token: TOKEN,
      scope: 'read',
      createdMs: 5,
    };
    const result = await pairWithToken(BASE, TOKEN, [old], fetchImpl);
    expect(result).toMatchObject({ ok: true, pairing: { id: 'x', createdMs: 5 } });
  });
});

describe('PairingStore', () => {
  it('drops a stored pairing that points outside the LAN, so a tampered store cannot leak a token', () => {
    const saved: Pairing[] = [
      { id: 'a', label: 'A', baseUrl: BASE, token: TOKEN, scope: 'read', createdMs: 1 },
      {
        id: 'b',
        label: 'B',
        baseUrl: 'http://203.0.113.9:7331',
        token: TOKEN,
        scope: 'read',
        createdMs: 2,
      },
    ];
    const storage = { getItem: () => JSON.stringify(saved) };
    expect(loadPairings(storage).map((p) => p.id)).toEqual(['a']);
  });

  it('persists a learnt scope and tells subscribers', () => {
    const map = new Map<string, string>();
    const storage = {
      getItem: (k: string) => map.get(k) ?? null,
      setItem: (k: string, v: string) => void map.set(k, v),
    };
    const store = new PairingStore(storage);
    let told = 0;
    store.subscribe(() => told++);
    store.upsert({
      id: 'a',
      label: 'A',
      baseUrl: BASE,
      token: TOKEN,
      scope: 'unknown',
      createdMs: 1,
    });
    store.setScope('a', 'read');
    expect(told).toBe(2);
    expect(new PairingStore(storage).get()[0]?.scope).toBe('read');
  });
});
