/**
 * The bridge to the LAN server commands.
 *
 * Everything here is invoke-shaped and injectable, so the panel can be tested
 * against a fake without a Tauri host and the real wiring stays in one file.
 */

import { hasTauriHost } from '../../shell/host';

export type Scope = 'read' | 'control';

export interface Interface {
  readonly name: string;
  readonly address: string;
  readonly score: number;
  readonly virtualised: boolean;
}

export interface TokenSummary {
  readonly prefix: string;
  readonly scope: Scope;
  readonly label: string;
  readonly created: number;
}

export interface LanStatus {
  readonly running: boolean;
  readonly port: number | null;
  readonly interfaces: readonly Interface[];
  readonly tokens: readonly TokenSummary[];
}

export interface Pairing {
  readonly url: string;
  readonly qrSvg: string;
  readonly token: TokenSummary;
}

export interface LanApi {
  status(): Promise<LanStatus>;
  start(port?: number): Promise<number>;
  stop(): Promise<void>;
  pair(request: { label: string; scope: Scope; address?: string }): Promise<Pairing>;
  revoke(prefix: string): Promise<void>;
  revokeAll(): Promise<void>;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const core = await import('@tauri-apps/api/core');
  return core.invoke<T>(command, args);
}

export const tauriLanApi: LanApi = {
  status: () => invoke<LanStatus>('get_lan_status'),
  start: (port) => invoke<number>('start_lan_server', { port: port ?? null }),
  stop: () => invoke<void>('stop_lan_server'),
  pair: (request) =>
    invoke<Pairing>('create_pairing', {
      request: {
        label: request.label,
        scope: request.scope,
        address: request.address ?? null,
      },
    }),
  revoke: (prefix) => invoke<void>('revoke_pairing', { prefix }),
  revokeAll: () => invoke<void>('revoke_all_pairings'),
};

/**
 * What the panel gets when there is no host: a server that cannot start.
 *
 * The browser preview must render the panel — its layout is what a developer
 * is checking — without pretending a socket exists.
 */
export const noHostLanApi: LanApi = {
  status: () => Promise.resolve({ running: false, port: null, interfaces: [], tokens: [] }),
  start: () => Promise.reject(new Error('no host')),
  stop: () => Promise.resolve(),
  pair: () => Promise.reject(new Error('no host')),
  revoke: () => Promise.resolve(),
  revokeAll: () => Promise.resolve(),
};

export function defaultLanApi(): LanApi {
  return hasTauriHost() ? tauriLanApi : noHostLanApi;
}
