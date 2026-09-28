import { useTranslation } from 'react-i18next';

import { Button, SegmentedControl, Select, Switch } from '@vitals/ui';

import { SHELL_NS } from '../shell/strings';
import { SettingsRow, SettingsSection } from './SettingsRow';
import {
  sensitivities,
  soundFromKey,
  soundKey,
  systemSounds,
  tauriWatchdogApi,
  useWatchdog,
  type WatchdogApi,
  type WatchdogSensitivity,
} from './watchdog';

const volumeSteps = [20, 40, 60, 80, 100] as const;

/** The file name only: a full path does not fit, and the name is what people recognise. */
function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export function WatchdogPanel({ api = tauriWatchdogApi }: { readonly api?: WatchdogApi }) {
  const { t } = useTranslation(SHELL_NS);
  const { config, status, busy, error, update, test, pickFile } = useWatchdog(api);

  const unavailable = status !== null && !status.available;
  const locked = busy || status === null || unavailable;

  const sensitivityLabels: Readonly<Record<WatchdogSensitivity, string>> = {
    relaxed: t('settings.watchdog.relaxed'),
    normal: t('settings.watchdog.normal'),
    sensitive: t('settings.watchdog.sensitive'),
  };

  const fileOption =
    config.sound.kind === 'file'
      ? t('settings.watchdog.soundFileChosen', { name: fileName(config.sound.value) })
      : t('settings.watchdog.soundFile');

  const soundOptions = [
    { value: 'default', label: t('settings.watchdog.soundDefault') },
    { value: 'silent', label: t('settings.watchdog.soundSilent') },
    ...systemSounds.map((name) => ({
      value: `system:${name}`,
      label: t(`settings.watchdog.sounds.${name.replace('.', '_')}`),
    })),
    { value: 'file', label: fileOption },
  ];

  const stateText =
    status === null
      ? t('settings.watchdog.stateChecking')
      : unavailable
        ? t('settings.watchdog.stateMissing')
        : status.running
          ? t('settings.watchdog.stateRunning')
          : t('settings.watchdog.stateStopped');

  return (
    <SettingsSection title={t('settings.watchdog.title')}>
      <SettingsRow
        label={t('settings.watchdog.enable')}
        description={t('settings.watchdog.enableHint')}
      >
        {({ labelId, describedBy }) => (
          <Switch
            aria-labelledby={labelId}
            aria-describedby={describedBy}
            disabled={locked}
            checked={config.enabled && !unavailable}
            onCheckedChange={(value) => update({ enabled: value })}
          />
        )}
      </SettingsRow>

      <SettingsRow
        label={t('settings.watchdog.sensitivity')}
        description={t(`settings.watchdog.${config.sensitivity}Hint`)}
      >
        {() => (
          <SegmentedControl
            ariaLabel={t('settings.watchdog.sensitivity')}
            value={config.sensitivity}
            onValueChange={(value) => update({ sensitivity: value })}
            options={sensitivities.map((s) => ({
              value: s,
              label: sensitivityLabels[s],
              disabled: locked || !config.enabled,
            }))}
          />
        )}
      </SettingsRow>

      <SettingsRow
        label={t('settings.watchdog.sound')}
        description={t('settings.watchdog.soundHint')}
      >
        {({ labelId }) => (
          <div className="flex items-center gap-2">
            <Select
              ariaLabel={t('settings.watchdog.sound')}
              disabled={locked || !config.enabled}
              value={soundKey(config.sound)}
              options={soundOptions}
              onValueChange={(key) => {
                const sound = soundFromKey(key, config.sound);
                if (sound === null) pickFile();
                else update({ sound });
              }}
            />
            <Button
              aria-describedby={labelId}
              variant="secondary"
              size="sm"
              disabled={locked || !config.enabled}
              onClick={pickFile}
            >
              {t('settings.watchdog.browse')}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              disabled={locked || !config.enabled || config.sound.kind === 'silent'}
              onClick={test}
            >
              {t('settings.watchdog.test')}
            </Button>
          </div>
        )}
      </SettingsRow>

      {config.sound.kind === 'file' && (
        <SettingsRow label={t('settings.watchdog.volume')} description={config.sound.value}>
          {() => (
            <SegmentedControl
              ariaLabel={t('settings.watchdog.volume')}
              value={String(config.volume)}
              onValueChange={(value) => update({ volume: Number(value) })}
              options={volumeSteps.map((v) => ({
                value: String(v),
                label: `${v} %`,
                disabled: locked || !config.enabled,
              }))}
            />
          )}
        </SettingsRow>
      )}

      <p className="text-2xs text-[var(--color-fg-muted)]" role="status">
        {stateText}
      </p>
      {error !== null && (
        <p className="text-2xs text-[var(--color-danger-text)]" role="alert">
          {error}
        </p>
      )}
    </SettingsSection>
  );
}
