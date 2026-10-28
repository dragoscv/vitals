import { useState, type FormEvent } from 'react';
import { useTranslation } from 'react-i18next';

import { Button, Input, Skeleton } from '@vitals/ui';

import type { HealthApi } from '../../features/health/api';
import { HEALTH_NS, registerHealthStrings } from '../../features/health/strings';
import { SettingsSection } from '../SettingsRow';
import { type CodeOutcome, useHealthConnection } from './useHealthConnection';

export interface HealthPanelProps {
  /** Injectable so the panel is testable without a Tauri host. */
  readonly api?: HealthApi;
  /** How often to ask vmui whether the code was approved. Tests shorten it. */
  readonly pollMs?: number;
}

const FAILURES = new Set([
  'invalidUrl',
  'insecureUrl',
  'unreachable',
  'wrongCredentials',
  'slowDown',
]);

const OUTCOME_KEY: Record<CodeOutcome, string> = {
  pending: 'settings.codeWaiting',
  rejected: 'settings.codeRejected',
  expired: 'settings.codeExpired',
};

function defaultDeviceName(): string {
  return 'Vitals';
}

/**
 * "Show my health from vmui on this computer."
 *
 * Two ways in: a four-digit code the owner approves in vmui (no password
 * typed here at all), or the vmui email and password, sent once and never
 * kept. Either way the key lands in the OS secure store and only its first
 * eight characters are ever shown.
 */
export function HealthPanel({ api, pollMs }: HealthPanelProps = {}) {
  // Registered here as well as by the page: the settings chunk can open
  // before the Health page has ever loaded.
  useState(() => {
    registerHealthStrings();
    return null;
  });
  const { t } = useTranslation(HEALTH_NS);
  const conn = useHealthConnection(api, pollMs);
  const [base, setBase] = useState<string | null>(null);
  const [name, setName] = useState(defaultDeviceName);
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [confirming, setConfirming] = useState(false);

  const address = base ?? conn.status?.base ?? '';
  const failure =
    conn.error === null
      ? null
      : FAILURES.has(conn.error)
        ? t(`settings.failure.${conn.error}`)
        : t('settings.failure.generic', { message: conn.error });

  const onSignIn = (event: FormEvent): void => {
    event.preventDefault();
    const secret = password;
    // Cleared from the form at once: the field must not hold it while the
    // request runs, and not at all afterwards.
    setPassword('');
    void conn.signIn(address, email, secret);
  };

  if (conn.status === null) {
    return <Skeleton className="h-40" />;
  }

  const connected = conn.status.configured;

  return (
    <div>
      <SettingsSection title={t('settings.title')}>
        <p className="py-2 text-2xs text-[var(--color-fg-muted)]">{t('settings.intro')}</p>

        <p role="status" className="py-1 text-sm">
          {connected && conn.status.base !== null
            ? t('settings.connected', { base: conn.status.base })
            : t('settings.notConnected')}
          {connected && conn.status.tokenPrefix !== null && (
            <span className="ml-2 font-mono text-2xs text-[var(--color-fg-muted)]">
              {t('settings.keyPrefix', { prefix: conn.status.tokenPrefix })}
            </span>
          )}
        </p>

        {failure !== null && (
          <p role="alert" className="py-1 text-2xs text-[var(--color-danger-text)]">
            {failure}
          </p>
        )}

        {connected && (
          <div className="flex flex-wrap items-center gap-2 py-2">
            {confirming ? (
              <>
                <span className="text-2xs">{t('settings.disconnectConfirm')}</span>
                <Button
                  variant="danger"
                  size="sm"
                  disabled={conn.busy}
                  onClick={() => {
                    setConfirming(false);
                    void conn.disconnect();
                  }}
                >
                  {t('settings.disconnectYes')}
                </Button>
                <Button variant="ghost" size="sm" onClick={() => setConfirming(false)}>
                  {t('settings.cancel')}
                </Button>
              </>
            ) : (
              <Button variant="secondary" size="sm" onClick={() => setConfirming(true)}>
                {t('settings.disconnect')}
              </Button>
            )}
          </div>
        )}
      </SettingsSection>

      {!connected && (
        <SettingsSection title={t('settings.pair')}>
          <div className="space-y-3 py-2">
            <Input
              label={t('settings.address')}
              hint={t('settings.addressHint')}
              type="url"
              inputMode="url"
              autoComplete="url"
              value={address}
              onChange={(e) => setBase(e.target.value)}
            />

            {conn.code === null ? (
              <>
                <Input
                  label={t('settings.deviceName')}
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
                <div className="flex items-center gap-2">
                  <Button
                    variant="primary"
                    size="sm"
                    disabled={conn.busy || address.trim() === ''}
                    onClick={() => {
                      void conn.pair(address, name);
                    }}
                  >
                    {t('settings.pair')}
                  </Button>
                  <span className="text-2xs text-[var(--color-fg-muted)]">
                    {t('settings.pairHint')}
                  </span>
                </div>
              </>
            ) : (
              <div className="space-y-2 rounded-[var(--radius-control)] border border-[var(--color-border-subtle)] p-3">
                <p className="text-sm font-medium">{t('settings.codeTitle')}</p>
                <p className="text-2xs text-[var(--color-fg-muted)]">{t('settings.codeBody')}</p>
                <p
                  className="tnum font-mono text-3xl tracking-[0.3em]"
                  aria-label={conn.code.split('').join(' ')}
                >
                  {conn.code}
                </p>
                {conn.codeOutcome !== null && (
                  <p role="status" className="text-2xs">
                    {t(OUTCOME_KEY[conn.codeOutcome])}
                  </p>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => {
                    conn.cancelPair();
                  }}
                >
                  {t('settings.cancel')}
                </Button>
              </div>
            )}
          </div>
        </SettingsSection>
      )}

      {!connected && conn.code === null && (
        <SettingsSection title={t('settings.orSignIn')}>
          <form className="space-y-3 py-2" onSubmit={onSignIn}>
            <Input
              label={t('settings.email')}
              type="email"
              autoComplete="username"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
            <Input
              label={t('settings.password')}
              hint={t('settings.passwordHint')}
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
            <Button
              type="submit"
              variant="secondary"
              size="sm"
              disabled={
                conn.busy || address.trim() === '' || email.trim() === '' || password === ''
              }
            >
              {t('settings.signIn')}
            </Button>
          </form>
        </SettingsSection>
      )}
    </div>
  );
}
