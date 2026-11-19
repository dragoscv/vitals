/**
 * The bridge to the MQTT commands.
 *
 * Everything here is invoke-shaped and injectable, so the panel can be tested
 * against a fake without a Tauri host and the real wiring stays in one file.
 * The password is never sent: the backend reads it from the OS secret store,
 * so a secret cannot reach the webview or the settings file by construction.
 */

import { hasTauriHost } from '../../shell/host';

export interface MqttStatus {
  /** Whether the publish loop is up — not whether the broker is reachable. */
  readonly running: boolean;
  /** The last refused start, if any. */
  readonly error: string | null;
  /** Whether a broker password is stored in the OS secret store. */
  readonly passwordSet: boolean;
}

export interface MqttRequest {
  readonly url: string;
  /** Omitted (or empty) for a broker that allows anonymous access. */
  readonly username?: string;
}

export interface MqttApi {
  status(): Promise<MqttStatus>;
  start(request: MqttRequest): Promise<MqttStatus>;
  stop(): Promise<MqttStatus>;
  /** Stores a password, or removes it with `null`. Resolves to "is one set". */
  setPassword(password: string | null): Promise<boolean>;
}

async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const core = await import('@tauri-apps/api/core');
  return core.invoke<T>(command, args);
}

export const tauriMqttApi: MqttApi = {
  status: () => invoke<MqttStatus>('get_mqtt_status'),
  // The empty string is sent as `null` so the backend sees "no username"
  // rather than a username that happens to be blank.
  start: (request) =>
    invoke<MqttStatus>('start_mqtt', {
      url: request.url,
      username: request.username === undefined || request.username === '' ? null : request.username,
    }),
  stop: () => invoke<MqttStatus>('stop_mqtt'),
  setPassword: (password) => invoke<boolean>('set_mqtt_password', { password }),
};

/**
 * What the panel gets when there is no host: publishers that cannot start.
 *
 * The browser preview must render the panel — its layout is what a developer
 * is checking — without pretending a socket exists.
 */
export const noHostMqttApi: MqttApi = {
  status: () => Promise.resolve({ running: false, error: null, passwordSet: false }),
  start: () => Promise.reject(new Error('no host')),
  stop: () => Promise.resolve({ running: false, error: null, passwordSet: false }),
  setPassword: () => Promise.reject(new Error('no host')),
};

export function defaultMqttApi(): MqttApi {
  return hasTauriHost() ? tauriMqttApi : noHostMqttApi;
}
