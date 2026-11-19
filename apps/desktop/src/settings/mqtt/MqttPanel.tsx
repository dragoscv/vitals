import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Badge, Button, Input, Skeleton, Switch } from '@vitals/ui';

import { SHELL_NS } from '../../shell/strings';
import { SettingsRow, SettingsSection } from '../SettingsRow';
import { useSettings } from '../store';
import { checkBrokerAddress, type AddressProblem } from './address';
import type { MqttApi } from './api';
import { useMqtt } from './useMqtt';

export interface MqttPanelProps {
  /** Injectable so the panel is testable without a Tauri host. */
  readonly api?: MqttApi;
}

/** The sentence each refusal gets. `empty` is not a refusal — see below. */
const ADDRESS_MESSAGE: Readonly<Record<Exclude<AddressProblem, 'empty'>, string>> = {
  scheme: 'settings.mqtt.invalidScheme',
  host: 'settings.mqtt.invalidHost',
  credentials: 'settings.mqtt.invalidCredentials',
  malformed: 'settings.mqtt.invalidAddress',
};

/**
 * "Publish this computer's readings to a broker."
 *
 * A second half of Remote access: the LAN server is for a phone on the same
 * network, MQTT is for a broker that may be anywhere. The panel must make the
 * direction obvious — the computer dials out, the broker never dials in — and
 * must not claim to be publishing when the backend has said otherwise.
 */
export function MqttPanel({ api }: MqttPanelProps = {}) {
  const { t } = useTranslation(SHELL_NS);
  const settings = useSettings((state) => state.settings);
  const patch = useSettings((state) => state.patch);
  const mqtt = useMqtt(api);
  // Typed then saved, not saved as typed: a secret reaching the OS store on
  // every keystroke would be a vault write per character.
  const [password, setPassword] = useState('');

  const { mqtt: config } = settings;
  const check = checkBrokerAddress(config.url);
  // An empty field is a section being set up, not a mistake; only a filled-in
  // address that is wrong is shown as an error.
  const addressError =
    !check.ok && check.problem !== 'empty' ? t(ADDRESS_MESSAGE[check.problem]) : null;

  const setEnabled = (enabled: boolean) => {
    patch({ mqtt: { ...config, enabled } });
    if (!enabled) {
      void mqtt.stop();
      return;
    }
    // Refuse a bad address rather than opening a connection the user did not
    // describe; the sentence is already on the row above.
    if (!check.ok) return;
    void mqtt.start(config.url, config.username);
  };

  const reapply = async () => {
    // A password reaches the broker only on a fresh connection, so a change
    // is applied by restarting rather than by editing the running publisher.
    if (config.enabled && check.ok) await mqtt.start(config.url, config.username);
  };

  const savePassword = async () => {
    if (!(await mqtt.setPassword(password))) return;
    setPassword('');
    await reapply();
  };

  const clearPassword = async () => {
    if (await mqtt.setPassword(null)) await reapply();
  };

  const error = mqtt.error ?? mqtt.status?.error ?? null;

  return (
    <SettingsSection title={t('settings.mqtt.title')}>
      <p className="pb-1 text-2xs text-[var(--color-fg-muted)]">{t('settings.mqtt.explainer')}</p>

      <SettingsRow label={t('settings.mqtt.enable')} description={t('settings.mqtt.enableHint')}>
        {({ labelId, describedBy }) =>
          mqtt.pending ? (
            <Skeleton className="h-5 w-9" />
          ) : (
            <Switch
              aria-labelledby={labelId}
              aria-describedby={describedBy}
              checked={config.enabled}
              disabled={mqtt.busy}
              onCheckedChange={setEnabled}
            />
          )
        }
      </SettingsRow>

      {/* The description is the hint normally and the refusal when the address
          is wrong — the same shape the Task Manager row uses, so an invalid
          field is never silent and never needs a second element. */}
      <SettingsRow
        label={t('settings.mqtt.address')}
        description={addressError ?? t('settings.mqtt.addressHint')}
      >
        {({ labelId, describedBy }) => (
          <Input
            aria-labelledby={labelId}
            aria-describedby={describedBy}
            aria-invalid={addressError !== null || undefined}
            value={config.url}
            placeholder={t('settings.mqtt.addressPlaceholder')}
            onChange={(event) => {
              patch({ mqtt: { ...config, url: event.currentTarget.value } });
            }}
          />
        )}
      </SettingsRow>

      <SettingsRow
        label={t('settings.mqtt.username')}
        description={t('settings.mqtt.usernameHint')}
      >
        {({ labelId, describedBy }) => (
          <Input
            aria-labelledby={labelId}
            aria-describedby={describedBy}
            value={config.username}
            onChange={(event) => {
              patch({ mqtt: { ...config, username: event.currentTarget.value } });
            }}
          />
        )}
      </SettingsRow>

      <SettingsRow
        label={t('settings.mqtt.password')}
        description={t('settings.mqtt.passwordHint')}
      >
        {({ labelId, describedBy }) => (
          <div className="flex items-center gap-2">
            <Input
              type="password"
              aria-labelledby={labelId}
              aria-describedby={describedBy}
              value={password}
              placeholder={mqtt.status?.passwordSet === true ? t('settings.mqtt.passwordSet') : ''}
              onChange={(event) => setPassword(event.currentTarget.value)}
            />
            <Button
              variant="secondary"
              size="sm"
              disabled={password === '' || mqtt.busy}
              onClick={() => {
                void savePassword();
              }}
            >
              {t('settings.mqtt.savePassword')}
            </Button>
            {mqtt.status?.passwordSet === true && (
              <Button
                variant="ghost"
                size="sm"
                disabled={mqtt.busy}
                onClick={() => {
                  void clearPassword();
                }}
              >
                {t('settings.mqtt.removePassword')}
              </Button>
            )}
          </div>
        )}
      </SettingsRow>

      {mqtt.status?.running === true && (
        <SettingsRow label={t('settings.mqtt.status')}>
          {() => <Badge tone="ok">{t('settings.mqtt.publishing')}</Badge>}
        </SettingsRow>
      )}

      {error !== null && (
        <p
          role="alert"
          className="rounded-[var(--radius-control)] border border-[var(--color-status-danger)] bg-[var(--color-bg-inset)] p-2.5 text-2xs"
        >
          {error}
        </p>
      )}
    </SettingsSection>
  );
}
