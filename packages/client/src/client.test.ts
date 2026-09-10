import type { Frame } from '@vitals/protocol';
import { describe, expect, it, vi } from 'vitest';

import type { BackoffOptions } from './backoff';
import { VitalsClient } from './client';
import { VitalsError } from './errors';

const BASE = 'http://vitals.test:4820';
const TOKEN = 'tok/en+with=chars';

const FRAME: Frame = {
  seq: 7,
  timestampMs: 1_700_000_000_000,
  elapsedMs: 1000,
  payload: { kind: 'delta', system: {} as never, changed: [], exited: [] } as never,
};

type Reply = { status: number; body?: string; contentType?: string };

/** A request body is only ever a JSON string here; anything else is a test bug. */
function asText(body: BodyInit | null | undefined): string {
  if (typeof body !== 'string') throw new Error('expected a string request body');
  return body;
}

/** A `fetch` that replays scripted replies and records every request. */
function scripted(...replies: Reply[]) {
  const calls: Array<{ url: string; init: RequestInit | undefined }> = [];
  const impl: typeof fetch = (input, init) => {
    calls.push({ url: input instanceof Request ? input.url : input.toString(), init });
    const next = replies.shift();
    if (next === undefined) return Promise.reject(new TypeError('fetch failed'));
    return Promise.resolve(
      new Response(next.body ?? null, {
        status: next.status,
        headers: { 'Content-Type': next.contentType ?? 'application/json' },
      }),
    );
  };
  return { impl, calls };
}

function client(
  fetchImpl: typeof fetch,
  extra: Partial<ConstructorParameters<typeof VitalsClient>[0]> = {},
) {
  return new VitalsClient({ baseUrl: `${BASE}/`, token: TOKEN, fetch: fetchImpl, ...extra });
}

async function failure(promise: Promise<unknown>): Promise<VitalsError> {
  try {
    await promise;
  } catch (error) {
    if (error instanceof VitalsError) return error;
    throw new Error('expected a VitalsError', { cause: error });
  }
  throw new Error('expected the promise to reject');
}

describe('VitalsClient HTTP methods', () => {
  it('sends the bearer token on authenticated routes and not on health', async () => {
    const { impl, calls } = scripted(
      { status: 200, body: '{"ok":true,"version":"1","modelVersion":3}' },
      { status: 200, body: JSON.stringify(FRAME) },
    );
    const c = client(impl);
    await c.health();
    await c.snapshot();

    expect(new Headers(calls[0]?.init?.headers).get('Authorization')).toBeNull();
    expect(new Headers(calls[1]?.init?.headers).get('Authorization')).toBe(`Bearer ${TOKEN}`);
    expect(calls[0]?.url).toBe(`${BASE}/api/v1/health`);
  });

  it('returns null from snapshot on 204 so a caller can tell "not sampling yet" from all-zero readings', async () => {
    const { impl } = scripted({ status: 204 });
    await expect(client(impl).snapshot()).resolves.toBeNull();
  });

  it('returns the parsed frame from snapshot once the server has sampled', async () => {
    const { impl } = scripted({ status: 200, body: JSON.stringify(FRAME) });
    await expect(client(impl).snapshot()).resolves.toEqual(FRAME);
  });

  it('returns null from host on 204', async () => {
    const { impl } = scripted({ status: 204 });
    await expect(client(impl).host()).resolves.toBeNull();
  });

  it('rejects with kind unauthorised on 401 from any authenticated route', async () => {
    const { impl } = scripted({ status: 401, body: '{"error":"a bearer token is required"}' });
    const error = await failure(client(impl).snapshot());
    expect(error.kind).toBe('unauthorised');
    expect(error.status).toBe(401);
  });

  it('rejects with kind forbidden and the server reason on 403 from control', async () => {
    const { impl } = scripted({ status: 403, body: '{"kind":"forbidden"}' });
    const error = await failure(
      client(impl).control({ action: 'terminate', key: { pid: 1, startTime: 2 } }),
    );
    expect(error.kind).toBe('forbidden');
    expect(error.detail).toEqual({ kind: 'forbidden' });
  });

  it('rejects with kind http and the parsed ControlError on a 404 not-found refusal', async () => {
    const { impl } = scripted({ status: 404, body: '{"kind":"not-found"}' });
    const error = await failure(
      client(impl).control({ action: 'suspend', key: { pid: 1, startTime: 2 } }),
    );
    expect(error.kind).toBe('http');
    expect(error.status).toBe(404);
    expect(error.detail).toEqual({ kind: 'not-found' });
  });

  it('posts the control request as tagged JSON and resolves on 204', async () => {
    const { impl, calls } = scripted({ status: 204 });
    await client(impl).control({
      action: 'set-priority',
      key: { pid: 42, startTime: 7 },
      priority: 'below-normal',
    });
    expect(calls[0]?.init?.method).toBe('POST');
    expect(JSON.parse(asText(calls[0]?.init?.body))).toEqual({
      action: 'set-priority',
      key: { pid: 42, startTime: 7 },
      priority: 'below-normal',
    });
  });

  it('rejects with kind network when fetch itself throws', async () => {
    const { impl } = scripted();
    const error = await failure(client(impl).health());
    expect(error.kind).toBe('network');
    expect(error.cause).toBeInstanceOf(TypeError);
  });

  it('returns the Prometheus text verbatim from metrics', async () => {
    const { impl } = scripted({
      status: 200,
      body: 'vitals_cpu_percent 12.5\n',
      contentType: 'text/plain',
    });
    await expect(client(impl).metrics()).resolves.toBe('vitals_cpu_percent 12.5\n');
  });
});

// ── Streaming ──────────────────────────────────────────────────────────

/** Manual timers, shared by the SSE and WebSocket tests. */
function manualBackoff(): Partial<BackoffOptions> & { fire(): void; delays: number[] } {
  const delays: number[] = [];
  let queued: (() => void) | undefined;
  return {
    initialMs: 100,
    maxMs: 800,
    random: () => 0,
    schedule(fn, ms) {
      delays.push(ms);
      queued = fn;
      return () => {
        queued = undefined;
      };
    },
    fire() {
      const run = queued;
      queued = undefined;
      if (run === undefined) throw new Error('nothing was scheduled');
      run();
    },
    delays,
  };
}

/** A fake `EventSource` the test opens, feeds and kills by hand. */
class FakeEventSource {
  static instances: FakeEventSource[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent<string>) => void) | null = null;
  onerror: (() => void) | null = null;
  closed = false;
  constructor(readonly url: string) {
    FakeEventSource.instances.push(this);
  }
  close() {
    this.closed = true;
  }
  open() {
    this.onopen?.();
  }
  message(data: string) {
    this.onmessage?.({ data } as MessageEvent<string>);
  }
  error() {
    this.onerror?.();
  }
}

describe('VitalsClient.stream', () => {
  function streaming() {
    FakeEventSource.instances = [];
    const backoff = manualBackoff();
    const c = client(vi.fn(), {
      EventSource: FakeEventSource as unknown as typeof EventSource,
      backoff,
    });
    const onFrame = vi.fn();
    const onDrop = vi.fn();
    const onRetry = vi.fn();
    const stop = c.stream({ onFrame, onDrop, onRetry });
    return { backoff, onFrame, onDrop, onRetry, stop };
  }

  it('puts the URL-encoded token in the query string because EventSource cannot set headers', () => {
    streaming();
    expect(FakeEventSource.instances[0]?.url).toBe(
      `${BASE}/api/v1/stream?token=${encodeURIComponent(TOKEN)}`,
    );
  });

  it('delivers each parsed frame to the handler', () => {
    const { onFrame } = streaming();
    FakeEventSource.instances[0]?.open();
    FakeEventSource.instances[0]?.message(JSON.stringify(FRAME));
    expect(onFrame).toHaveBeenCalledWith(FRAME);
  });

  it('reconnects with growing, capped delays when the connection keeps dropping', () => {
    const { backoff, onDrop, onRetry } = streaming();

    for (let i = 0; i < 5; i += 1) {
      const current = FakeEventSource.instances[i];
      expect(current).toBeDefined();
      current?.error();
      expect(current?.closed).toBe(true);
      backoff.fire();
    }

    expect(FakeEventSource.instances).toHaveLength(6);
    // 100 → 200 → 400 → 800 (cap) → 800, halved by random() === 0.
    expect(backoff.delays).toEqual([50, 100, 200, 400, 400]);
    expect(onDrop).toHaveBeenCalledTimes(5);
    expect(onRetry).toHaveBeenLastCalledWith(400, 4);
  });

  it('resets the backoff after a reconnect that succeeds', () => {
    const { backoff } = streaming();
    FakeEventSource.instances[0]?.error();
    backoff.fire();
    FakeEventSource.instances[1]?.error();
    backoff.fire();
    // A screen unlocked; the phone is back.
    FakeEventSource.instances[2]?.open();
    FakeEventSource.instances[2]?.error();
    expect(backoff.delays).toEqual([50, 100, 50]);
  });

  it('closes the source and schedules no retry once unsubscribed', () => {
    const { backoff, stop } = streaming();
    FakeEventSource.instances[0]?.error();
    stop();
    expect(FakeEventSource.instances[0]?.closed).toBe(true);
    expect(() => {
      backoff.fire();
    }).toThrow('nothing was scheduled');
    // A late event from the dead source must not resurrect the stream.
    FakeEventSource.instances[0]?.error();
    expect(backoff.delays).toEqual([50]);
    expect(FakeEventSource.instances).toHaveLength(1);
  });
});

/** A fake `WebSocket` with the handful of members the client touches. */
class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((event: MessageEvent<unknown>) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;
  sent: string[] = [];
  closed = false;
  constructor(readonly url: string) {
    FakeWebSocket.instances.push(this);
  }
  send(data: string) {
    this.sent.push(data);
  }
  close() {
    this.closed = true;
  }
  open() {
    this.onopen?.();
  }
  message(data: string) {
    this.onmessage?.({ data } as MessageEvent<unknown>);
  }
  drop() {
    this.onerror?.();
    this.onclose?.();
  }
}

describe('VitalsClient.connect', () => {
  function connected() {
    FakeWebSocket.instances = [];
    const backoff = manualBackoff();
    const c = client(vi.fn(), {
      WebSocket: FakeWebSocket as unknown as typeof WebSocket,
      backoff,
    });
    const onFrame = vi.fn();
    const onDrop = vi.fn();
    const connection = c.connect({ onFrame, onDrop });
    return { backoff, onFrame, onDrop, connection };
  }

  it('upgrades the base URL scheme to ws and carries the token in the query', () => {
    connected();
    expect(FakeWebSocket.instances[0]?.url).toBe(
      `ws://vitals.test:4820/api/v1/ws?token=${encodeURIComponent(TOKEN)}`,
    );
  });

  it('routes frames to the handler and control replies to the awaiting caller', async () => {
    const { onFrame, connection } = connected();
    const socket = FakeWebSocket.instances[0];
    socket?.open();

    const reply = connection.control({ action: 'resume', key: { pid: 1, startTime: 2 } });
    expect(JSON.parse(socket?.sent[0] ?? '')).toEqual({
      action: 'resume',
      key: { pid: 1, startTime: 2 },
    });

    socket?.message(JSON.stringify(FRAME));
    socket?.message('{"ok":false,"error":{"kind":"not-found"}}');

    expect(onFrame).toHaveBeenCalledWith(FRAME);
    await expect(reply).resolves.toEqual({ ok: false, error: { kind: 'not-found' } });
  });

  it('rejects in-flight control requests with kind closed when the socket drops', async () => {
    const { connection } = connected();
    const socket = FakeWebSocket.instances[0];
    socket?.open();
    const reply = connection.control({ action: 'terminate', key: { pid: 1, startTime: 2 } });
    socket?.drop();
    const error = await failure(reply);
    expect(error.kind).toBe('closed');
  });

  it('rejects a control request sent while disconnected rather than queueing it', async () => {
    const { connection } = connected();
    const error = await failure(
      connection.control({ action: 'terminate', key: { pid: 1, startTime: 2 } }),
    );
    expect(error.kind).toBe('closed');
    expect(FakeWebSocket.instances[0]?.sent).toEqual([]);
  });

  it('treats error followed by close as one drop and reconnects with backoff', () => {
    const { backoff, onDrop } = connected();
    FakeWebSocket.instances[0]?.drop();
    expect(onDrop).toHaveBeenCalledTimes(1);
    expect(backoff.delays).toEqual([50]);
    backoff.fire();
    FakeWebSocket.instances[1]?.drop();
    expect(backoff.delays).toEqual([50, 100]);
    expect(FakeWebSocket.instances).toHaveLength(2);
  });

  it('stops reconnecting once closed by the caller', () => {
    const { backoff, connection } = connected();
    FakeWebSocket.instances[0]?.drop();
    connection.close();
    expect(() => {
      backoff.fire();
    }).toThrow('nothing was scheduled');
    expect(FakeWebSocket.instances[0]?.closed).toBe(true);
  });
});
