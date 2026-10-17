/**
 * codai account on the Cloud tab: sign in with the device flow (a code shown
 * here, approved in the browser), see connected hosts, sign out.
 *
 * Nothing is sent to codai until the user presses "Sign in". The key that
 * results is stored by Rust in Credential Manager and never reaches the
 * webview.
 */

import { Cloud, ExternalLink, LogOut, Monitor } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Badge, Button, EmptyState, Spinner } from '@vitals/ui';

import { errorMessage } from '../../lib/commandError';
import { hasTauriHost } from '../../shell/host';
import { AGENTS_NS } from './strings';

interface Host {
  readonly device_id: string;
  readonly name: string | null;
  readonly platform: string | null;
  readonly hostname: string | null;
}
interface CloudStatus {
  readonly signedIn: boolean;
  readonly hosts: readonly Host[];
  readonly error: string | null;
}
interface DeviceAuth {
  readonly device_code: string;
  readonly user_code: string;
  readonly verification_uri: string;
  readonly verification_uri_complete: string | null;
  readonly interval: number;
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import('@tauri-apps/api/core');
  return invoke<T>(cmd, args);
}

export function CloudAccount() {
  const { t } = useTranslation(AGENTS_NS);
  const [status, setStatus] = useState<CloudStatus | null>(null);
  const [auth, setAuth] = useState<DeviceAuth | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setStatus(await call<CloudStatus>('get_cloud_status'));
    } catch (cause: unknown) {
      setError(errorMessage(cause));
    }
  }, []);

  useEffect(() => {
    if (!hasTauriHost()) return undefined;
    let alive = true;
    call<CloudStatus>('get_cloud_status').then(
      (s) => alive && setStatus(s),
      (cause: unknown) => alive && setError(errorMessage(cause)),
    );
    return () => {
      alive = false;
    };
  }, []);

  async function signIn() {
    setBusy(true);
    setError(null);
    try {
      const a = await call<DeviceAuth>('start_cloud_sign_in');
      setAuth(a);
      // Best effort: if the browser cannot be opened the code on screen and
      // the address still work, so a failure here must not end the sign-in.
      try {
        const { openUrl } = await import('@tauri-apps/plugin-opener');
        await openUrl(a.verification_uri_complete ?? a.verification_uri);
      } catch (cause: unknown) {
        setError(errorMessage(cause));
      }
      await call<boolean>('finish_cloud_sign_in', {
        deviceCode: a.device_code,
        interval: a.interval,
      });
      setAuth(null);
      await refresh();
    } catch (cause: unknown) {
      setError(errorMessage(cause));
      setAuth(null);
    } finally {
      setBusy(false);
    }
  }

  async function signOut() {
    await call('cloud_sign_out');
    await refresh();
  }

  if (!hasTauriHost()) return null;
  if (status === null) return <Spinner />;

  if (!status.signedIn) {
    return (
      <div className="flex flex-col gap-2">
        <EmptyState
          icon={<Cloud />}
          title={t('cloud.signIn')}
          description={t('cloud.signInBody')}
        />
        {auth !== null && (
          <p role="status" className="text-center text-sm">
            {t('cloud.code')}{' '}
            <code className="rounded bg-[var(--color-bg-subtle)] px-2 py-1 text-base font-semibold tracking-widest">
              {auth.user_code}
            </code>
            <span className="mt-1 block text-2xs text-[var(--color-fg-subtle)]">
              {t('cloud.codeAt', { address: auth.verification_uri })}
            </span>
          </p>
        )}
        <div className="flex justify-center">
          <Button onClick={() => void signIn()} disabled={busy}>
            <ExternalLink aria-hidden className="size-4" />
            {busy ? t('cloud.waiting') : t('cloud.signInButton')}
          </Button>
        </div>
        {error !== null && (
          <p role="alert" className="text-center text-2xs text-[var(--color-danger-text)]">
            {error}
          </p>
        )}
      </div>
    );
  }

  return (
    <section aria-label={t('cloud.account')} className="flex flex-col gap-2">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold">{t('cloud.hosts')}</h3>
        <Button variant="ghost" size="sm" onClick={() => void signOut()}>
          <LogOut aria-hidden className="size-4" />
          {t('cloud.signOut')}
        </Button>
      </div>
      {status.error !== null && (
        <p className="text-2xs text-[var(--color-warn-text)]">{status.error}</p>
      )}
      {status.hosts.length === 0 ? (
        <p className="text-2xs text-[var(--color-fg-subtle)]">{t('cloud.noHosts')}</p>
      ) : (
        <ul className="flex flex-wrap gap-2">
          {status.hosts.map((h) => (
            <li key={h.device_id}>
              <Badge tone="ok" icon={<Monitor />}>
                {h.name ?? h.hostname ?? h.device_id.slice(0, 8)}
                {h.platform !== null && ` · ${h.platform}`}
              </Badge>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
