/**
 * The lag watchdog's settings (ADR-0032).
 *
 * Not an `AppSettings` field. The watchdog is a separate process that must
 * obey these while Vitals is closed, so the truth is the file it reads
 * (`%APPDATA%\Vitals\watchdog.json`) and the logon entry that starts it.
 * The backend writes both in one command and answers with what Windows now
 * says, so the switch can never show a state the machine is not in.
 */

import { useCallback, useEffect, useState } from 'react';

import { hasTauriHost } from '../shell/host';

export type WatchdogSensitivity = 'relaxed' | 'normal' | 'sensitive';

export type WatchdogSound =
  | { readonly kind: 'default' }
  | { readonly kind: 'silent' }
  | { readonly kind: 'system'; readonly value: string }
  | { readonly kind: 'file'; readonly value: string };

export interface WatchdogConfig {
  readonly enabled: boolean;
  readonly sensitivity: WatchdogSensitivity;
  readonly sound: WatchdogSound;
  /** 0–100; applies to a chosen file only. */
  readonly volume: number;
}

export interface WatchdogStatus {
  /** This build includes the watchdog. */
  readonly available: boolean;
  /** Windows starts it at sign-in. */
  readonly registered: boolean;
  readonly running: boolean;
}

/** Windows' own sounds, as `ms-winsoundevent:Notification.*` names. */
export const systemSounds = [
  'IM',
  'Mail',
  'Reminder',
  'SMS',
  'Looping.Alarm',
  'Looping.Alarm2',
  'Looping.Alarm3',
  'Looping.Alarm4',
  'Looping.Alarm5',
  'Looping.Call',
  'Looping.Call2',
  'Looping.Call3',
] as const;

export const sensitivities: readonly WatchdogSensitivity[] = ['relaxed', 'normal', 'sensitive'];

export const defaultWatchdogConfig: WatchdogConfig = {
  enabled: true,
  sensitivity: 'normal',
  sound: { kind: 'default' },
  volume: 80,
};

const STORAGE_KEY = 'vitals.watchdog';

/** Audio the watchdog can play through MCI. */
export const audioExtensions = ['wav', 'mp3', 'm4a', 'wma', 'aac', 'flac'] as const;

export interface WatchdogApi {
  status(): Promise<WatchdogStatus>;
  save(config: WatchdogConfig): Promise<WatchdogStatus>;
  test(sound: WatchdogSound, volume: number): Promise<void>;
  pickFile(): Promise<string | null>;
}

export const tauriWatchdogApi: WatchdogApi = {
  async status() {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<WatchdogStatus>('get_watchdog_status');
  },
  async save(config) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<WatchdogStatus>('set_watchdog_config', { config });
  },
  async test(sound, volume) {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke<void>('test_watchdog_sound', { sound, volume });
  },
  async pickFile() {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const picked = await open({
      multiple: false,
      directory: false,
      filters: [{ name: 'Audio', extensions: [...audioExtensions] }],
    });
    return typeof picked === 'string' ? picked : null;
  },
};

/**
 * The last config the user saved, for the panel to start from.
 *
 * Kept in localStorage as a mirror only: the backend is authoritative for
 * whether it is running, and the file is authoritative for the watchdog.
 * Without a mirror the panel would open on defaults and a single click
 * would overwrite a carefully chosen sound.
 */
function loadMirror(): WatchdogConfig {
  try {
    const raw = globalThis.localStorage.getItem(STORAGE_KEY);
    if (raw === null) return defaultWatchdogConfig;
    return parseConfig(JSON.parse(raw) as unknown);
  } catch {
    return defaultWatchdogConfig;
  }
}

/** Accepts only a well-formed config; anything else falls back field by field. */
export function parseConfig(value: unknown): WatchdogConfig {
  if (typeof value !== 'object' || value === null) return defaultWatchdogConfig;
  const record = value as Record<string, unknown>;
  const sensitivity = sensitivities.find((s) => s === record['sensitivity']);
  const volume = record['volume'];
  return {
    enabled:
      typeof record['enabled'] === 'boolean' ? record['enabled'] : defaultWatchdogConfig.enabled,
    sensitivity: sensitivity ?? defaultWatchdogConfig.sensitivity,
    sound: parseSound(record['sound']),
    volume:
      typeof volume === 'number' && volume >= 0 && volume <= 100
        ? Math.round(volume)
        : defaultWatchdogConfig.volume,
  };
}

function parseSound(value: unknown): WatchdogSound {
  if (typeof value !== 'object' || value === null) return defaultWatchdogConfig.sound;
  const { kind, value: inner } = value as { kind?: unknown; value?: unknown };
  if (kind === 'default' || kind === 'silent') return { kind };
  if (kind === 'system' && typeof inner === 'string' && systemSounds.some((s) => s === inner)) {
    return { kind, value: inner };
  }
  if (kind === 'file' && typeof inner === 'string' && inner.trim() !== '') {
    return { kind, value: inner };
  }
  return defaultWatchdogConfig.sound;
}

export interface WatchdogState {
  readonly config: WatchdogConfig;
  /** `null` until the first read answers. */
  readonly status: WatchdogStatus | null;
  readonly busy: boolean;
  readonly error: string | null;
  readonly update: (patch: Partial<WatchdogConfig>) => void;
  readonly test: () => void;
  readonly pickFile: () => void;
}

export function useWatchdog(api: WatchdogApi = tauriWatchdogApi): WatchdogState {
  const [config, setConfig] = useState<WatchdogConfig>(loadMirror);
  const [status, setStatus] = useState<WatchdogStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!hasTauriHost()) {
      setStatus({ available: false, registered: false, running: false });
      return;
    }
    let cancelled = false;
    api
      .status()
      .then((value) => {
        if (!cancelled) setStatus(value);
      })
      .catch((failure: unknown) => {
        if (!cancelled) setError(describe(failure));
      });
    return () => {
      cancelled = true;
    };
  }, [api]);

  const save = useCallback(
    (next: WatchdogConfig) => {
      const previous = config;
      setConfig(next);
      setBusy(true);
      setError(null);
      api
        .save(next)
        .then((value) => {
          setStatus(value);
          globalThis.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
        })
        .catch((failure: unknown) => {
          // The backend refused (a file that vanished, say): show why and
          // put the controls back to what is actually in force.
          setConfig(previous);
          setError(describe(failure));
        })
        .finally(() => setBusy(false));
    },
    [api, config],
  );

  const update = useCallback(
    (patch: Partial<WatchdogConfig>) => save({ ...config, ...patch }),
    [config, save],
  );

  const test = useCallback(() => {
    setError(null);
    api.test(config.sound, config.volume).catch((failure: unknown) => setError(describe(failure)));
  }, [api, config.sound, config.volume]);

  const pickFile = useCallback(() => {
    api
      .pickFile()
      .then((path) => {
        if (path !== null) update({ sound: { kind: 'file', value: path } });
      })
      .catch((failure: unknown) => setError(describe(failure)));
  }, [api, update]);

  return { config, status, busy, error, update, test, pickFile };
}

function describe(failure: unknown): string {
  if (failure instanceof Error) return failure.message;
  if (typeof failure === 'object' && failure !== null && 'message' in failure) {
    const { message } = failure;
    if (typeof message === 'string') return message;
  }
  return String(failure);
}

/** The value a sound has in the sound picker. */
export function soundKey(sound: WatchdogSound): string {
  switch (sound.kind) {
    case 'default':
    case 'silent':
      return sound.kind;
    case 'system':
      return `system:${sound.value}`;
    case 'file':
      return 'file';
  }
}

/** The sound a picker value stands for; `null` for "choose a file…". */
export function soundFromKey(key: string, current: WatchdogSound): WatchdogSound | null {
  if (key === 'default' || key === 'silent') return { kind: key };
  if (key.startsWith('system:')) {
    const value = key.slice('system:'.length);
    return systemSounds.some((s) => s === value) ? { kind: 'system', value } : null;
  }
  if (key === 'file') return current.kind === 'file' ? current : null;
  return null;
}
