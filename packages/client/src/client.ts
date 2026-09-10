/**
 * The typed client for a Vitals server (`crates/vitals-server`).
 *
 * Zero runtime dependencies on purpose: it uses the platform's `fetch`,
 * `EventSource` and `WebSocket`, which exist unpolyfilled in every browser
 * and in Node 22+. Each is also injectable, both for tests and for a runtime
 * that supplies its own (React Native, a proxying fetch).
 */

import type { Alert, Frame, HostInfo, ProcessKey } from '@vitals/protocol';

import { type Attempt, type BackoffOptions, DEFAULT_BACKOFF, Reconnector } from './backoff';
import type { ControlError, ControlReply, ControlRequest, Health } from './control';
import { VitalsError } from './errors';

/** Constructor arguments for {@link VitalsClient}. */
export interface VitalsClientOptions {
  /** Where the server listens, e.g. `http://192.168.1.20:4820`. A trailing slash is tolerated. */
  baseUrl: string;
  /** The bearer token the desktop issued. */
  token: string;
  /** Replaces the global `fetch`. */
  fetch?: typeof globalThis.fetch;
  /** Replaces the global `EventSource`. */
  EventSource?: typeof globalThis.EventSource;
  /** Replaces the global `WebSocket`. */
  WebSocket?: typeof globalThis.WebSocket;
  /** Reconnect tuning; anything omitted falls back to {@link DEFAULT_BACKOFF}. */
  backoff?: Partial<BackoffOptions>;
}

/** Receives frames and lifecycle events from a live transport. */
export interface StreamHandler {
  /** One frame. Keyframes and deltas both arrive here; narrow with `@vitals/protocol` guards. */
  onFrame(frame: Frame): void;
  /** The transport is connected. Fires again after every successful reconnect. */
  onOpen?(): void;
  /**
   * The transport dropped. The client will retry on its own; this exists so
   * a UI can show "reconnecting" instead of a frozen last frame.
   */
  onDrop?(error: VitalsError): void;
  /** A retry has been scheduled `delayMs` from now. `attempt` counts consecutive failures from zero. */
  onRetry?(delayMs: number, attempt: number): void;
}

/** Stops a stream and cancels any pending reconnect. Idempotent. */
export type Unsubscribe = () => void;

/** A live WebSocket session: frames in, control requests out. */
export interface VitalsConnection {
  /**
   * Sends a control request and resolves with the host's reply.
   *
   * Rejects with a `closed` {@link VitalsError} if the socket drops before
   * the reply arrives — the request may or may not have been applied, and
   * the caller must re-read the process list rather than retry blindly.
   */
  control(request: ControlRequest): Promise<ControlReply>;
  /** Closes the socket for good; no reconnect follows. Idempotent. */
  close(): void;
}

const JSON_HEADERS = { 'Content-Type': 'application/json' } as const;

/** Talks to one Vitals server. Stateless apart from its configuration; share one per server. */
export class VitalsClient {
  private readonly baseUrl: string;
  private readonly token: string;
  private readonly fetchImpl: typeof globalThis.fetch;
  private readonly EventSourceImpl: typeof globalThis.EventSource | undefined;
  private readonly WebSocketImpl: typeof globalThis.WebSocket | undefined;
  private readonly backoff: BackoffOptions;

  constructor(options: VitalsClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/+$/, '');
    this.token = options.token;
    // Bound once so an injected `fetch` that is a method (e.g. from an axios
    // adapter) keeps its `this`, and so the browser's own `fetch` is not
    // invoked with the client as receiver — which throws "Illegal invocation".
    this.fetchImpl = options.fetch ?? globalThis.fetch.bind(globalThis);
    this.EventSourceImpl = options.EventSource ?? globalThis.EventSource;
    this.WebSocketImpl = options.WebSocket ?? globalThis.WebSocket;
    this.backoff = { ...DEFAULT_BACKOFF, ...options.backoff };
  }

  /**
   * `GET /api/v1/health` — the one unauthenticated call.
   *
   * Use it to tell "wrong address" from "wrong token": a 401 from anything
   * else looks identical whether the token is missing or merely unknown.
   */
  async health(): Promise<Health> {
    const response = await this.request('/api/v1/health', {}, false);
    return this.json<Health>(response);
  }

  /**
   * `GET /api/v1/snapshot` — the most recent keyframe.
   *
   * Resolves `null` when the server answers `204 No Content`, which it does
   * before its first sampling tick. That is not a failure: a monitor that has
   * not sampled yet and a monitor whose every reading is zero are different
   * states, and a UI must render "starting…" for the first, not a row of
   * zeroes. Modelling it as an exception would push every caller to catch
   * and swallow, which is how the distinction gets lost.
   */
  async snapshot(): Promise<Frame | null> {
    const response = await this.request('/api/v1/snapshot');
    if (response.status === 204) return null;
    return this.json<Frame>(response);
  }

  /** `GET /api/v1/host` — static machine facts, or `null` before the host has been probed. */
  async host(): Promise<HostInfo | null> {
    const response = await this.request('/api/v1/host');
    if (response.status === 204) return null;
    return this.json<HostInfo>(response);
  }

  /**
   * `GET /api/v1/alerts` — every condition currently raised on the host.
   *
   * An empty array is the healthy answer, not a failure, and is by far the
   * most common one. The `title` and `cause` on each alert are i18n keys
   * (`alert.<kind>.title`), not prose — see `Alert` in `@vitals/protocol`.
   */
  async alerts(): Promise<Alert[]> {
    const response = await this.request('/api/v1/alerts');
    return this.json<Alert[]>(response);
  }

  /**
   * `GET /metrics` — the Prometheus exposition text, verbatim.
   *
   * Returns an empty string before the first tick (the server's 204) so a
   * scraper that concatenates outputs need not special-case it.
   */
  async metrics(): Promise<string> {
    const response = await this.request('/metrics');
    if (response.status === 204) return '';
    return response.text();
  }

  /**
   * `POST /api/v1/control` — act on a process over plain HTTP.
   *
   * Resolves on the server's 204. Rejects with `forbidden` for a read-only
   * token or an access-denied action, and with `http` (carrying the parsed
   * {@link ControlError} in `detail`) for not-found, unsupported and internal
   * refusals, so a caller can show the host's reason verbatim.
   */
  async control(request: ControlRequest): Promise<void> {
    const response = await this.request('/api/v1/control', {
      method: 'POST',
      headers: JSON_HEADERS,
      body: JSON.stringify(request),
    });
    if (response.status !== 204) {
      throw new VitalsError('protocol', `expected 204 from control, got ${response.status}`, {
        status: response.status,
      });
    }
  }

  /**
   * Turns Windows 11 efficiency mode on or off for one process — a named
   * wrapper over {@link control} so a caller cannot misspell the tag or omit
   * the flag. Same rejections as `control`; `unsupported` when the host's
   * Windows build has no EcoQoS.
   */
  setEfficiencyMode(key: ProcessKey, enabled: boolean): Promise<void> {
    return this.control({ action: 'set-efficiency-mode', key, enabled });
  }

  /**
   * `GET /api/v1/stream` — frames over server-sent events.
   *
   * Reconnects for as long as the returned function has not been called.
   * `EventSource` does retry on its own, but with a fixed interval and it
   * stops for good on some failures (a 5xx during a server restart), so the
   * built-in behaviour is disabled by closing the source on every error and
   * letting {@link Reconnector} own the schedule.
   *
   * The token travels in the query string because a browser cannot set
   * headers on an `EventSource`. A 401 is invisible here — the browser
   * reports only "error" — so validate the token with {@link health} or
   * {@link snapshot} first; otherwise a bad token costs one request per
   * backoff ceiling, indefinitely.
   */
  stream(handler: StreamHandler): Unsubscribe {
    const EventSourceCtor = this.EventSourceImpl;
    if (EventSourceCtor === undefined) {
      throw new VitalsError(
        'protocol',
        'EventSource is not available in this runtime; pass one in the client options',
      );
    }
    const url = this.url('/api/v1/stream', true);

    const reconnector = new Reconnector(
      (signals: Attempt) => {
        const source = new EventSourceCtor(url);
        let settled = false;
        const drop = (why: string) => {
          if (settled) return;
          settled = true;
          source.close();
          handler.onDrop?.(new VitalsError('network', why));
          signals.dropped();
        };
        source.onopen = () => {
          signals.opened();
          handler.onOpen?.();
        };
        source.onmessage = (event: MessageEvent<string>) => {
          const frame = parseFrame(event.data);
          if (frame === undefined) {
            drop('the server sent an event that is not a frame');
            return;
          }
          handler.onFrame(frame);
        };
        source.onerror = () => {
          drop('the event stream closed');
        };
        return () => {
          settled = true;
          source.close();
        };
      },
      this.backoff,
      (delay, attempt) => handler.onRetry?.(delay, attempt),
    );
    reconnector.start();

    return () => {
      reconnector.stop();
    };
  }

  /**
   * `GET /api/v1/ws` — frames in, control commands out, on one socket.
   *
   * Reconnects like {@link stream}. Control replies carry no correlation id
   * on the wire; the server answers each text message in order on a single
   * task, so replies are matched to requests FIFO. A drop rejects every
   * request still in flight — their outcome is unknown, and retrying "end
   * task" against a PID that may have been recycled is exactly what the
   * `ProcessKey` design exists to prevent.
   */
  connect(handler: StreamHandler): VitalsConnection {
    const WebSocketCtor = this.WebSocketImpl;
    if (WebSocketCtor === undefined) {
      throw new VitalsError(
        'protocol',
        'WebSocket is not available in this runtime; pass one in the client options',
      );
    }
    const url = this.url('/api/v1/ws', true).replace(/^http/, 'ws');

    let live: WebSocket | undefined;
    let pending: Array<{
      resolve: (reply: ControlReply) => void;
      reject: (error: VitalsError) => void;
    }> = [];
    const failPending = (error: VitalsError) => {
      const waiting = pending;
      pending = [];
      for (const entry of waiting) entry.reject(error);
    };

    const reconnector = new Reconnector(
      (signals: Attempt) => {
        const socket = new WebSocketCtor(url);
        let settled = false;
        const drop = (why: string) => {
          if (settled) return;
          settled = true;
          if (live === socket) live = undefined;
          socket.close();
          const error = new VitalsError('network', why);
          failPending(new VitalsError('closed', why, { cause: error }));
          handler.onDrop?.(error);
          signals.dropped();
        };
        socket.onopen = () => {
          live = socket;
          signals.opened();
          handler.onOpen?.();
        };
        socket.onmessage = (event: MessageEvent<unknown>) => {
          if (typeof event.data !== 'string') return;
          const parsed = parseJson(event.data);
          if (isControlReply(parsed)) {
            const entry = pending.shift();
            if (entry === undefined) {
              // A reply with no request is a server bug or a shared socket
              // being used from two places; neither is recoverable here.
              drop('the server replied to a request this client did not send');
              return;
            }
            entry.resolve(parsed);
            return;
          }
          if (isFrame(parsed)) {
            handler.onFrame(parsed);
            return;
          }
          drop('the server sent a message that is neither a frame nor a reply');
        };
        // `close` follows `error`, so both routes converge on the same
        // idempotent `drop`; whichever fires first wins.
        socket.onerror = () => {
          drop('the socket errored');
        };
        socket.onclose = () => {
          drop('the socket closed');
        };
        return () => {
          settled = true;
          if (live === socket) live = undefined;
          socket.close();
        };
      },
      this.backoff,
      (delay, attempt) => handler.onRetry?.(delay, attempt),
    );
    reconnector.start();

    return {
      control(request) {
        return new Promise<ControlReply>((resolve, reject) => {
          if (live === undefined) {
            reject(new VitalsError('closed', 'the socket is not connected'));
            return;
          }
          pending.push({ resolve, reject });
          live.send(JSON.stringify(request));
        });
      },
      close() {
        reconnector.stop();
        live = undefined;
        failPending(new VitalsError('closed', 'the connection was closed by the caller'));
      },
    };
  }

  // ── Plumbing ──────────────────────────────────────────────────────────

  private url(path: string, tokenInQuery = false): string {
    // `encodeURIComponent` because a token is opaque bytes from the server's
    // point of view; assuming it is URL-safe would break on the day the
    // desktop changes how it mints them.
    const query = tokenInQuery ? `?token=${encodeURIComponent(this.token)}` : '';
    return `${this.baseUrl}${path}${query}`;
  }

  /**
   * One HTTP round trip with the status codes the API uses mapped to
   * {@link VitalsError}s. Transport failures — DNS, refused, aborted — are
   * `network`; the caller never sees a raw `TypeError` from `fetch`.
   */
  private async request(path: string, init: RequestInit = {}, auth = true): Promise<Response> {
    const headers = new Headers(init.headers);
    if (auth) headers.set('Authorization', `Bearer ${this.token}`);

    let response: Response;
    try {
      response = await this.fetchImpl(this.url(path), { ...init, headers });
    } catch (cause) {
      throw new VitalsError('network', `could not reach ${this.baseUrl}`, { cause });
    }

    if (response.ok || response.status === 204) return response;

    const detail = await readControlError(response);
    const options = {
      status: response.status,
      ...(detail !== undefined && { detail }),
    };
    switch (response.status) {
      case 401:
        // The server sends the same body for "no token" and "unknown token"
        // so as not to be an oracle; there is nothing more specific to say.
        throw new VitalsError('unauthorised', 'the server rejected the bearer token', options);
      case 403:
        throw new VitalsError('forbidden', detail?.kind ?? 'this token cannot act', options);
      default:
        throw new VitalsError('http', `${path} answered ${response.status}`, options);
    }
  }

  private async json<T>(response: Response): Promise<T> {
    try {
      return (await response.json()) as T;
    } catch (cause) {
      throw new VitalsError('protocol', 'the server sent a body that is not JSON', {
        status: response.status,
        cause,
      });
    }
  }
}

/**
 * Best-effort parse of an error body. Refusals from `/control` carry a
 * {@link ControlError}; the 401 body is `{ error: string }` and everything
 * else may be empty — all of which yield `undefined` here.
 */
async function readControlError(response: Response): Promise<ControlError | undefined> {
  let text: string;
  try {
    text = await response.text();
  } catch {
    return undefined;
  }
  const parsed = parseJson(text);
  return isControlError(parsed) ? parsed : undefined;
}

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text) as unknown;
  } catch {
    return undefined;
  }
}

function parseFrame(text: string): Frame | undefined {
  const parsed = parseJson(text);
  return isFrame(parsed) ? parsed : undefined;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

/**
 * Structural check, deliberately shallow. The generated types are the
 * contract; re-validating every field here would be a second, hand-written
 * copy of the Rust model that drifts. Enough to route the message.
 */
function isFrame(value: unknown): value is Frame {
  return (
    isRecord(value) &&
    typeof value.seq === 'number' &&
    typeof value.timestampMs === 'number' &&
    isRecord(value.payload) &&
    typeof value.payload.kind === 'string'
  );
}

function isControlReply(value: unknown): value is ControlReply {
  return isRecord(value) && typeof value.ok === 'boolean' && !('seq' in value);
}

function isControlError(value: unknown): value is ControlError {
  return isRecord(value) && typeof value.kind === 'string';
}
