import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Badge, Button, Input, Select, Skeleton, Switch } from '@vitals/ui';

import { SHELL_NS } from '../../shell/strings';
import { SettingsRow, SettingsSection } from '../SettingsRow';
import type { LanApi, Scope } from './api';
import { useLan } from './useLan';

export interface RemoteAccessPanelProps {
  /** Injectable so the panel is testable without a Tauri host. */
  readonly api?: LanApi;
}

/**
 * "See this computer from your phone."
 *
 * Three states, and the panel must make which one it is in obvious: off (the
 * default), on but unpaired, and paired. The failure this layout is designed
 * against is a user who turns the switch on, sees nothing change, and does not
 * realise a pairing is a separate step.
 */
export function RemoteAccessPanel({ api }: RemoteAccessPanelProps = {}) {
  const { t } = useTranslation(SHELL_NS);
  const lan = useLan(api);
  const [label, setLabel] = useState('');
  const [scope, setScope] = useState<Scope>('read');
  const [address, setAddress] = useState<string | undefined>(undefined);

  const running = lan.status?.running ?? false;
  const interfaces = lan.status?.interfaces ?? [];
  const tokens = lan.status?.tokens ?? [];
  const chosen = address ?? interfaces[0]?.address;

  return (
    <>
      <SettingsSection title={t('settings.remote.title')}>
        <p className="pb-1 text-2xs text-[var(--color-fg-muted)]">
          {t('settings.remote.explainer')}
        </p>

        <SettingsRow
          label={t('settings.remote.enable')}
          description={t('settings.remote.enableHint')}
        >
          {({ labelId, describedBy }) =>
            lan.pending ? (
              <Skeleton className="h-5 w-9" />
            ) : (
              <Switch
                aria-labelledby={labelId}
                aria-describedby={describedBy}
                checked={running}
                disabled={lan.busy}
                onCheckedChange={(next) => {
                  void (next ? lan.start() : lan.stop());
                }}
              />
            )
          }
        </SettingsRow>

        {running && (
          <SettingsRow label={t('settings.remote.status')}>
            {() => (
              <Badge tone="ok">
                {t('settings.remote.listening', { port: lan.status?.port ?? 0 })}
              </Badge>
            )}
          </SettingsRow>
        )}

        {lan.error !== null && (
          <p
            role="alert"
            className="rounded-[var(--radius-control)] border border-[var(--color-status-danger)] bg-[var(--color-bg-inset)] p-2.5 text-2xs"
          >
            {lan.error}
          </p>
        )}
      </SettingsSection>

      {running && (
        <SettingsSection title={t('settings.remote.pairTitle')}>
          <SettingsRow
            label={t('settings.remote.interface')}
            // Spread rather than `cond ? x : undefined`: under
            // `exactOptionalPropertyTypes` the prop is `string`, not
            // `string | undefined`, so passing undefined is a type error.
            {...(interfaces.length > 1 && {
              description: t('settings.remote.interfaceHint'),
            })}
          >
            {({ labelId }) =>
              interfaces.length === 0 ? (
                <span className="text-2xs text-[var(--color-fg-muted)]">
                  {t('settings.remote.noInterfaces')}
                </span>
              ) : (
                <Select
                  ariaLabel={t('settings.remote.interface')}
                  aria-labelledby={labelId}
                  {...(chosen !== undefined && { value: chosen })}
                  onValueChange={setAddress}
                  options={interfaces.map((iface) => ({
                    value: iface.address,
                    // The adapter name matters: it is how someone recognises
                    // "this is my Wi-Fi" versus "this is the Hyper-V switch".
                    label: `${iface.name} — ${iface.address}`,
                  }))}
                />
              )
            }
          </SettingsRow>

          <SettingsRow label={t('settings.remote.deviceName')}>
            {({ labelId }) => (
              <Input
                aria-labelledby={labelId}
                value={label}
                placeholder={t('settings.remote.deviceNamePlaceholder')}
                onChange={(event) => setLabel(event.currentTarget.value)}
              />
            )}
          </SettingsRow>

          <SettingsRow
            label={t('settings.remote.allowControl')}
            description={t('settings.remote.allowControlHint')}
          >
            {({ labelId, describedBy }) => (
              <Switch
                aria-labelledby={labelId}
                aria-describedby={describedBy}
                checked={scope === 'control'}
                onCheckedChange={(next) => {
                  setScope(next ? 'control' : 'read');
                }}
              />
            )}
          </SettingsRow>

          <SettingsRow label={t('settings.remote.createPairing')}>
            {({ labelId }) => (
              <Button
                aria-labelledby={labelId}
                disabled={lan.busy || chosen === undefined}
                onClick={() => {
                  void lan.pair(label.trim() || t('settings.remote.unnamedDevice'), scope, chosen);
                }}
              >
                {t('settings.remote.showQr')}
              </Button>
            )}
          </SettingsRow>

          {lan.pairing !== null && (
            <div className="mt-2 flex flex-col items-center gap-2 rounded-[var(--radius-widget)] border border-[var(--color-border-subtle)] bg-[var(--color-bg-inset)] p-4">
              <p className="text-2xs text-[var(--color-fg-muted)]">
                {t('settings.remote.scanHint')}
              </p>
              {/* The SVG is built by our own Rust from a URL we constructed —
                  no user input reaches it. */}
              <div
                className="[&>svg]:size-56 [&>svg]:rounded-[var(--radius-control)]"
                aria-label={t('settings.remote.qrAlt')}
                role="img"
                dangerouslySetInnerHTML={{ __html: lan.pairing.qrSvg }}
              />
              <code className="text-2xs break-all text-[var(--color-fg-muted)]">
                {lan.pairing.url}
              </code>
              <p className="text-2xs text-[var(--color-status-warn)]">
                {t('settings.remote.onceOnly')}
              </p>
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  lan.dismissPairing();
                }}
              >
                {t('settings.remote.dismiss')}
              </Button>
            </div>
          )}
        </SettingsSection>
      )}

      {tokens.length > 0 && (
        <SettingsSection title={t('settings.remote.pairedTitle')}>
          <ul className="divide-y divide-[var(--color-border-subtle)]">
            {tokens.map((token) => (
              <li key={token.prefix} className="flex items-center justify-between gap-3 py-2">
                <div className="min-w-0">
                  <span className="block truncate text-sm">{token.label}</span>
                  <span className="text-2xs text-[var(--color-fg-muted)]">
                    {token.prefix}… ·{' '}
                    {token.scope === 'control'
                      ? t('settings.remote.scopeControl')
                      : t('settings.remote.scopeRead')}
                  </span>
                </div>
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={lan.busy}
                  onClick={() => {
                    void lan.revoke(token.prefix);
                  }}
                >
                  {t('settings.remote.revoke')}
                </Button>
              </li>
            ))}
          </ul>
          <div className="pt-2">
            <Button
              variant="danger"
              size="sm"
              disabled={lan.busy}
              onClick={() => {
                void lan.revokeAll();
              }}
            >
              {t('settings.remote.revokeAll')}
            </Button>
          </div>
        </SettingsSection>
      )}
    </>
  );
}
