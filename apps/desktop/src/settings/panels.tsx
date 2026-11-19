import { Fragment, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { locales, type Locale } from '@vitals/i18n';
import type { HostInfo } from '@vitals/protocol';
import type {
  Channel as AlertChannel,
  ChannelKind,
  Comparator,
  Rule as AlertRule,
  RuleMetric,
  Severity as RuleSeverity,
} from '@vitals/protocol';
import {
  Button,
  Input,
  SegmentedControl,
  Select,
  Skeleton,
  Switch,
  cn,
  focusRing,
  formatBytes,
} from '@vitals/ui';
import { Trash2 } from 'lucide-react';

import { SHELL_NS } from '../shell/strings';
import { hasTauriHost } from '../shell/host';
import { openWindowsTaskManager, quitApp } from '../lib/settingsSync';
import { reportFailure } from '../lib/reportFailure';
import { useTheme } from '../theme/ThemeProvider';
import { accents, densities, themeModes, type Accent } from '../theme/types';
import { SettingsRow, SettingsSection } from './SettingsRow';
import { clearHistory, exportFlightRecording } from './historyActions';
import { retentionDayOptions, samplingRates, trayMetrics } from './schema';
import { useSettings } from './store';
import { useHostInfo } from './useHostInfo';
import { useHistoryUsage } from './useHistoryUsage';
import { useTaskManagerReplacement } from './useTaskManagerReplacement';
import { useIsStoreBuild, useStore } from './useDistribution';
import { useNavVisibility } from '../shell/navigation';
import { runsOn } from '../shell/platform';
import { UpdateSection } from './UpdatePanel';

/** Mirrors `ChannelStatus` in `apps/desktop/src-tauri/src/channels.rs`. */
interface ChannelStatus {
  readonly lastOk: boolean | null;
  readonly lastStatus: number | null;
}

/**
 * A type alias rather than an inline generic: `scripts/check-drift.ps1` reads
 * the command name from an `invoke` call with a single-word type argument, and
 * a nested generic such as a `Record` slips past its pattern — which would
 * report the command as unreachable from the webview.
 */
type ChannelStatusMap = Record<string, ChannelStatus>;

/** The public documents. One place, so the app and the site cannot disagree. */
const SITE = 'https://vitals.dragoscatalin.ro';
const legalLinks = {
  privacyPolicy: `${SITE}/privacy/`,
  terms: `${SITE}/terms/`,
  licenses: 'https://github.com/dragoscv/vitals/blob/main/THIRD_PARTY_NOTICES.md',
} as const;

/**
 * Locale names are written in their own language, not translated.
 *
 * A user who has accidentally set the app to a language they cannot read must
 * still be able to find their way back — "Romanian" is useless to them if it
 * is rendered in Romanian.
 */
const localeNames: Readonly<Record<Locale, string>> = {
  en: 'English',
  ro: 'Română',
};

/**
 * i18n keys for the tray options, spelled out rather than assembled from the
 * value: a built key silently renders its own name the day a value is renamed.
 */
const trayMetricLabels = {
  off: 'settings.general.trayMetricOff',
  cpu: 'settings.general.trayMetricCpu',
  cpuTemperature: 'settings.general.trayMetricCpuTemperature',
  memory: 'settings.general.trayMetricMemory',
  network: 'settings.general.trayMetricNetwork',
} as const;

export function GeneralPanel() {
  const { t } = useTranslation(SHELL_NS);
  const settings = useSettings((state) => state.settings);
  const patch = useSettings((state) => state.patch);
  const taskManager = useTaskManagerReplacement();
  const storeBuild = useIsStoreBuild();
  // The platform comes from the visibility store the shell already fills from
  // `get_capabilities`, so the panel does not ask again.
  const platform = useNavVisibility((state) => state.platform);
  const windows = runsOn(platform, 'windows');

  // Three reasons the switch cannot move, each shown rather than implied:
  // the registry has not answered yet, a write is in flight, or another
  // tool owns the hook and we refuse to take it from them.
  const taskManagerLocked =
    taskManager.status === null || taskManager.busy || taskManager.status.replacedBy !== null;
  const replacedBy = taskManager.status?.replacedBy;
  const taskManagerBase =
    replacedBy !== null && replacedBy !== undefined
      ? t('settings.general.replaceTaskManagerOwned', { debugger: replacedBy })
      : t('settings.general.replaceTaskManagerHint');
  // While the replacement is on, this copy is per-machine — the only kind that
  // may own the hook — so the move offered in About would end it, and nothing
  // over there is visible from this row. Said here, where the setting lives.
  const taskManagerHint =
    taskManager.status?.enabled === true
      ? `${taskManagerBase} ${t('settings.general.replaceTaskManagerMoveHint')}`
      : taskManagerBase;

  return (
    <SettingsSection title={t('settings.general.title')}>
      {/* Windows only, and the label says so. The autostart command answers
          `Unsupported` elsewhere, so the switch would move and do nothing;
          the macOS path (SMAppService, S24-10) brings its own wording
          ("Open at login") with it rather than a Windows word on a Mac. */}
      {windows && (
        <SettingsRow
          label={t('settings.general.startWithWindows')}
          description={t('settings.general.startWithWindowsHint')}
        >
          {({ labelId, describedBy }) => (
            <Switch
              aria-labelledby={labelId}
              aria-describedby={describedBy}
              checked={settings.startWithWindows}
              onCheckedChange={(value) => patch({ startWithWindows: value })}
            />
          )}
        </SettingsRow>
      )}

      <SettingsRow label={t('settings.general.startMinimised')}>
        {({ labelId }) => (
          <Switch
            aria-labelledby={labelId}
            checked={settings.startMinimised}
            // Starting minimised is meaningless unless the app also starts,
            // so the option is inert rather than hidden — hiding it would
            // leave the user unable to see the setting they are looking for.
            disabled={!settings.startWithWindows}
            onCheckedChange={(value) => patch({ startMinimised: value })}
          />
        )}
      </SettingsRow>

      <SettingsRow
        label={t('settings.general.closeToTray')}
        description={t('settings.general.closeToTrayHint')}
      >
        {({ labelId, describedBy }) => (
          <Switch
            aria-labelledby={labelId}
            aria-describedby={describedBy}
            checked={settings.closeToTray}
            onCheckedChange={(value) => patch({ closeToTray: value })}
          />
        )}
      </SettingsRow>

      <SettingsRow
        label={t('settings.general.taskbarLoad')}
        description={t('settings.general.taskbarLoadHint')}
      >
        {({ labelId, describedBy }) => (
          <Switch
            aria-labelledby={labelId}
            aria-describedby={describedBy}
            checked={settings.taskbarLoad}
            onCheckedChange={(value) => patch({ taskbarLoad: value })}
          />
        )}
      </SettingsRow>

      {/* Windows only: the tray icon is a Windows notification-area concept,
          and on macOS the menu-bar item is a different control (S24-10). */}
      {windows && (
        <SettingsRow
          label={t('settings.general.trayMetric')}
          description={t('settings.general.trayMetricHint')}
        >
          {({ labelId }) => (
            <Select
              ariaLabel={t('settings.general.trayMetric')}
              aria-labelledby={labelId}
              value={settings.trayMetric}
              onValueChange={(value) => {
                const metric = trayMetrics.find((candidate) => candidate === value);
                if (metric) patch({ trayMetric: metric });
              }}
              options={trayMetrics.map((metric) => ({
                value: metric,
                label: t(trayMetricLabels[metric]),
              }))}
            />
          )}
        </SettingsRow>
      )}

      <SettingsRow
        label={t('settings.general.hudVisible')}
        description={t('settings.general.hudVisibleHint')}
      >
        {({ labelId, describedBy }) => (
          <Switch
            aria-labelledby={labelId}
            aria-describedby={describedBy}
            checked={settings.hudVisible}
            onCheckedChange={(value) => patch({ hudVisible: value })}
          />
        )}
      </SettingsRow>

      <SettingsRow
        label={t('settings.general.confirmEndTask')}
        description={t('settings.general.confirmEndTaskHint')}
      >
        {({ labelId, describedBy }) => (
          <Switch
            aria-labelledby={labelId}
            aria-describedby={describedBy}
            checked={settings.confirmEndTask}
            onCheckedChange={(value) => patch({ confirmEndTask: value })}
          />
        )}
      </SettingsRow>

      {/* Two reasons a hook here would do nothing: the Store package cannot
          write HKLM, and IFEO exists only on Windows. */}
      {!storeBuild && windows && (
        <SettingsRow
          label={t('settings.general.replaceTaskManager')}
          description={taskManager.error ?? taskManagerHint}
        >
          {({ labelId, describedBy }) => (
            <Switch
              aria-labelledby={labelId}
              aria-describedby={describedBy}
              aria-invalid={taskManager.error !== null || undefined}
              checked={taskManager.status?.enabled ?? false}
              disabled={taskManagerLocked}
              onCheckedChange={(value) => taskManager.set(value)}
            />
          )}
        </SettingsRow>
      )}

      {windows && (
        <SettingsRow
          label={t('settings.general.openTaskManager')}
          description={t(
            storeBuild
              ? 'settings.general.openTaskManagerHintStore'
              : 'settings.general.openTaskManagerHint',
          )}
        >
          {({ labelId }) => (
            <Button
              aria-labelledby={labelId}
              variant="secondary"
              disabled={!hasTauriHost()}
              onClick={() => {
                void reportFailure(openWindowsTaskManager(), t('settings.general.openTaskManager'));
              }}
            >
              {t('settings.general.openTaskManager')}
            </Button>
          )}
        </SettingsRow>
      )}

      <SettingsRow label={t('settings.general.quit')} description={t('settings.general.quitHint')}>
        {({ labelId }) => (
          <Button
            aria-labelledby={labelId}
            variant="secondary"
            onClick={() => {
              void reportFailure(quitApp(), t('settings.general.quit'));
            }}
          >
            {t('settings.general.quit')}
          </Button>
        )}
      </SettingsRow>
    </SettingsSection>
  );
}

export function AppearancePanel() {
  const { t } = useTranslation();
  const { theme, setTheme } = useTheme();
  const settings = useSettings((state) => state.settings);
  const patch = useSettings((state) => state.patch);

  const modeLabels = {
    light: t('settings.appearance.themeLight'),
    dark: t('settings.appearance.themeDark'),
    system: t('settings.appearance.themeSystem'),
  } as const;

  const densityLabels = {
    compact: t('settings.appearance.densityCompact'),
    default: t('settings.appearance.densityDefault'),
    comfortable: t('settings.appearance.densityComfortable'),
  } as const;

  return (
    <>
      <SettingsSection title={t('settings.appearance.title')}>
        <SettingsRow label={t('settings.appearance.theme')}>
          {() => (
            <SegmentedControl
              ariaLabel={t('settings.appearance.theme')}
              value={theme.mode}
              onValueChange={(mode) => setTheme({ mode })}
              options={themeModes.map((mode) => ({ value: mode, label: modeLabels[mode] }))}
            />
          )}
        </SettingsRow>

        <SettingsRow label={t('settings.appearance.accent')}>
          {({ labelId }) => (
            <AccentPicker
              labelId={labelId}
              value={theme.accent}
              onChange={(accent) => setTheme({ accent })}
            />
          )}
        </SettingsRow>

        <SettingsRow label={t('settings.appearance.density')}>
          {({ labelId }) => (
            <Select
              ariaLabel={t('settings.appearance.density')}
              aria-labelledby={labelId}
              value={theme.density}
              onValueChange={(value) => {
                const density = densities.find((candidate) => candidate === value);
                if (density) setTheme({ density });
              }}
              options={densities.map((density) => ({
                value: density,
                label: densityLabels[density],
              }))}
            />
          )}
        </SettingsRow>

        <SettingsRow label={t('settings.appearance.reduceMotion')}>
          {({ labelId }) => (
            <Switch
              aria-labelledby={labelId}
              // `null` means follow the OS, and there is no third switch
              // position, so an unset value presents as off. Turning it on is
              // an explicit override; turning it off returns to following the
              // system rather than forcing motion on.
              checked={theme.reduceMotion === true}
              onCheckedChange={(value) => setTheme({ reduceMotion: value ? true : null })}
            />
          )}
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title={t('settings.language.title')}>
        <SettingsRow label={t('settings.language.language')}>
          {({ labelId }) => (
            <Select
              ariaLabel={t('settings.language.language')}
              aria-labelledby={labelId}
              value={settings.locale}
              onValueChange={(value) => {
                const locale = locales.find((candidate) => candidate === value);
                if (locale) patch({ locale });
              }}
              options={locales.map((locale) => ({ value: locale, label: localeNames[locale] }))}
            />
          )}
        </SettingsRow>
      </SettingsSection>
    </>
  );
}

function AccentPicker({
  labelId,
  value,
  onChange,
}: {
  readonly labelId: string;
  readonly value: Accent;
  readonly onChange: (accent: Accent) => void;
}) {
  const { t } = useTranslation();

  /*
   * A radiogroup, not ten buttons. Colour is the only distinguishing feature,
   * so each swatch needs a name (`aria-label`) and the group needs a single
   * tab stop with arrow keys inside it — otherwise a keyboard user tabs
   * through ten identically-announced controls to reach the next setting.
   *
   * The selected swatch also gains a ring, because a "selected colour"
   * indicated only by colour is unusable to the exact people most likely to
   * be changing it.
   */
  return (
    <div role="radiogroup" aria-labelledby={labelId} className="flex flex-wrap gap-1.5">
      {accents.map((accent) => {
        const selected = accent === value;
        return (
          <button
            key={accent}
            type="button"
            role="radio"
            aria-checked={selected}
            // Named "Accent colour: teal". The colour itself carries no
            // accessible information, so without the swatch name every option
            // in the group is announced identically.
            aria-label={`${t('settings.appearance.accent')}: ${accent}`}
            tabIndex={selected ? 0 : -1}
            data-accent={accent}
            onClick={() => onChange(accent)}
            onKeyDown={(event) => {
              const delta =
                event.key === 'ArrowRight' || event.key === 'ArrowDown'
                  ? 1
                  : event.key === 'ArrowLeft' || event.key === 'ArrowUp'
                    ? -1
                    : 0;
              if (delta === 0) return;
              event.preventDefault();
              const index = accents.indexOf(accent);
              const next = accents[(index + delta + accents.length) % accents.length];
              if (next) onChange(next);
            }}
            className={cn(
              'size-6 rounded-full border-2',
              selected
                ? 'border-[var(--color-fg-default)]'
                : 'border-transparent hover:border-[var(--color-border-strong)]',
              focusRing,
            )}
          >
            {/* The swatch reads its colour from the same `[data-accent]`
                selectors the whole theme uses, so a new accent needs no
                change here at all. */}
            <span className="block size-full rounded-full bg-[var(--color-accent)]" />
          </button>
        );
      })}
    </div>
  );
}

export function SamplingPanel() {
  const { t } = useTranslation();
  const { t: ts } = useTranslation(SHELL_NS);
  const settings = useSettings((state) => state.settings);
  const patch = useSettings((state) => state.patch);
  const [usageRevision, setUsageRevision] = useState(0);
  const [clearing, setClearing] = useState(false);
  const [exporting, setExporting] = useState(false);
  const { usage, pending } = useHistoryUsage(usageRevision);

  const rateLabels = {
    fast: ts('settings.sampling.rateFast'),
    normal: ts('settings.sampling.rateNormal'),
    slow: ts('settings.sampling.rateSlow'),
  } as const;

  return (
    <>
      <SettingsSection title={t('settings.sampling.title')}>
        <SettingsRow label={t('settings.sampling.rate')}>
          {({ labelId }) => (
            <Select
              ariaLabel={t('settings.sampling.rate')}
              aria-labelledby={labelId}
              value={settings.samplingRate}
              onValueChange={(value) => {
                const rate = samplingRates.find((candidate) => candidate === value);
                if (rate) patch({ samplingRate: rate });
              }}
              options={samplingRates.map((rate) => ({ value: rate, label: rateLabels[rate] }))}
            />
          )}
        </SettingsRow>

        <SettingsRow
          label={t('settings.sampling.backgroundThrottle')}
          description={t('settings.sampling.backgroundThrottleHint')}
        >
          {({ labelId, describedBy }) => (
            <Switch
              aria-labelledby={labelId}
              aria-describedby={describedBy}
              checked={settings.throttleWhenHidden}
              onCheckedChange={(value) => patch({ throttleWhenHidden: value })}
            />
          )}
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title={t('settings.history.title')}>
        <SettingsRow
          label={t('settings.history.enable')}
          description={t('settings.history.enableHint')}
        >
          {({ labelId, describedBy }) => (
            <Switch
              aria-labelledby={labelId}
              aria-describedby={describedBy}
              checked={settings.historyEnabled}
              onCheckedChange={(value) => patch({ historyEnabled: value })}
            />
          )}
        </SettingsRow>

        <SettingsRow label={t('settings.history.retention')}>
          {({ labelId }) => (
            <Select
              ariaLabel={t('settings.history.retention')}
              aria-labelledby={labelId}
              disabled={!settings.historyEnabled}
              value={String(settings.retentionDays)}
              onValueChange={(value) => patch({ retentionDays: Number(value) })}
              options={retentionDayOptions.map((days) => ({
                value: String(days),
                label: ts('settings.history.retentionDays', { count: days }),
              }))}
            />
          )}
        </SettingsRow>

        <SettingsRow
          label={t('settings.history.diskUsageLabel')}
          description={ts('settings.history.diskUsageHint')}
        >
          {() =>
            pending ? (
              <Skeleton className="h-4 w-24" />
            ) : (
              <span className="text-sm tabular-nums">
                {usage === null
                  ? ts('settings.history.noData')
                  : t('settings.history.diskUsage', { size: formatBytes(usage.bytes) })}
              </span>
            )
          }
        </SettingsRow>

        <SettingsRow
          label={t('settings.history.clear')}
          description={ts('settings.history.clearHint')}
        >
          {({ labelId }) => (
            <Button
              aria-labelledby={labelId}
              variant="danger"
              disabled={clearing || usage === null || usage.bytes === 0}
              onClick={() => {
                setClearing(true);
                void reportFailure(clearHistory(), t('settings.history.clear')).finally(() => {
                  setClearing(false);
                  // Bumping the revision is what refreshes the figure above;
                  // a stale "12 MB" beside a button that just deleted it
                  // reads as the button having failed.
                  setUsageRevision((n) => n + 1);
                });
              }}
            >
              {t('settings.history.clear')}
            </Button>
          )}
        </SettingsRow>
      </SettingsSection>

      <SettingsSection title={ts('settings.flight.title')}>
        <SettingsRow label={ts('settings.flight.export')} description={ts('settings.flight.hint')}>
          {({ labelId }) => (
            <Button
              aria-labelledby={labelId}
              disabled={exporting}
              onClick={() => {
                setExporting(true);
                void reportFailure(exportFlightRecording(), ts('settings.flight.export')).finally(
                  () => {
                    setExporting(false);
                  },
                );
              }}
            >
              {exporting ? ts('settings.flight.exporting') : ts('settings.flight.save')}
            </Button>
          )}
        </SettingsRow>
      </SettingsSection>
    </>
  );
}

export function NotificationsPanel() {
  const { t } = useTranslation(SHELL_NS);
  const settings = useSettings((state) => state.settings);
  const patch = useSettings((state) => state.patch);

  const alerts = [
    { key: 'notifyHighCpu', label: t('settings.notifications.highCpu') },
    { key: 'notifyHighMemory', label: t('settings.notifications.highMemory') },
    { key: 'notifyThermal', label: t('settings.notifications.thermal') },
  ] as const;

  return (
    <SettingsSection title={t('settings.notifications.title')}>
      <SettingsRow
        label={t('settings.notifications.enable')}
        description={t('settings.notifications.enableHint')}
      >
        {({ labelId, describedBy }) => (
          <Switch
            aria-labelledby={labelId}
            aria-describedby={describedBy}
            checked={settings.notificationsEnabled}
            onCheckedChange={(value) => patch({ notificationsEnabled: value })}
          />
        )}
      </SettingsRow>

      {alerts.map((alert) => (
        <SettingsRow key={alert.key} label={alert.label}>
          {({ labelId }) => (
            <Switch
              aria-labelledby={labelId}
              disabled={!settings.notificationsEnabled}
              checked={settings[alert.key]}
              onCheckedChange={(value) => patch({ [alert.key]: value })}
            />
          )}
        </SettingsRow>
      ))}

      <AlertRulesSection />
      <ChannelsSection />
    </SettingsSection>
  );
}

/**
 * Where alerts go beyond this machine.
 *
 * Every field saves as it is typed, so a half-written URL is a normal state
 * rather than an error: the Rust side treats a channel with no usable URL as
 * inert, and the test button is how the user finds out it works.
 *
 * The status line beside each channel comes from the backend, so it says what
 * actually happened on the last send — not what the browser hoped.
 */
function ChannelsSection() {
  const { t } = useTranslation(SHELL_NS);
  const channels = useSettings((state) => state.settings.notificationChannels);
  const patch = useSettings((state) => state.patch);
  const [testing, setTesting] = useState<string | null>(null);
  const [result, setResult] = useState<{ id: string; message: string | null } | null>(null);
  // What each destination last did, read once when the panel opens and again
  // after a test. Read on demand rather than polled: nothing here changes
  // while the user is looking at it except by their own click.
  const [lastResult, setLastResult] = useState<ChannelStatusMap>({});

  /**
   * The last result each channel returned, or `null` when there is no host to
   * ask — a browser build of the screens, where the panel simply works
   * without it rather than showing every channel as failed.
   */
  const readStatus = async (): Promise<ChannelStatusMap | null> => {
    if (!hasTauriHost()) return null;
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      return await invoke<ChannelStatusMap>('alert_channel_status');
    } catch {
      return null;
    }
  };

  // `.then` rather than an `async` body, and no `setLastResult` outside the
  // callback: React's lint rejects a state write reachable synchronously from
  // an effect, and the guard stops a reply from a panel the user has already
  // left.
  useEffect(() => {
    let live = true;
    void readStatus().then((status) => {
      if (live && status) setLastResult(status);
    });
    return () => {
      live = false;
    };
  }, []);

  /** Re-reads after a test, so the row shows what just happened. */
  const refreshStatus = async () => {
    const status = await readStatus();
    if (status) setLastResult(status);
  };

  const update = (index: number, next: Partial<AlertChannel>) => {
    patch({
      notificationChannels: channels.map((channel, i) =>
        i === index ? { ...channel, ...next } : channel,
      ),
    });
  };

  const add = () => {
    patch({
      notificationChannels: [
        ...channels,
        {
          id: `channel-${channels.length}-${channels.reduce((n, c) => n + c.id.length, 0)}`,
          name: t('settings.notifications.newChannel'),
          kind: 'webhook',
          url: '',
          enabled: false,
          secret: null,
          minSeverity: 'warning',
        },
      ],
    });
  };

  const test = async (id: string) => {
    setTesting(id);
    setResult(null);
    try {
      const { invoke } = await import('@tauri-apps/api/core');
      await invoke('test_alert_channel', { id });
      setResult({ id, message: null });
    } catch (error) {
      setResult({ id, message: error instanceof Error ? error.message : String(error) });
    } finally {
      setTesting(null);
      void refreshStatus();
    }
  };

  return (
    <>
      <div className="flex items-center justify-between pt-4">
        <div>
          <p className="text-sm font-medium">{t('settings.notifications.channels')}</p>
          <p className="text-xs text-[var(--color-fg-muted)]">
            {t('settings.notifications.channelsHint')}
          </p>
        </div>
        <Button variant="secondary" size="sm" onClick={add}>
          {t('settings.notifications.addChannel')}
        </Button>
      </div>

      {channels.map((channel, index) => (
        <div
          key={channel.id}
          className="grid gap-2 border-t border-[var(--color-border-subtle)] pt-3"
        >
          <div className="flex items-center gap-2">
            <Switch
              aria-label={t('settings.notifications.channelEnabled')}
              checked={channel.enabled}
              onCheckedChange={(value) => update(index, { enabled: value })}
            />
            <Input
              ariaLabel={t('settings.notifications.channelName')}
              value={channel.name}
              onChange={(event) => update(index, { name: event.target.value })}
            />
            <Button
              variant="secondary"
              size="sm"
              aria-label={t('settings.notifications.removeChannel')}
              onClick={() =>
                patch({ notificationChannels: channels.filter((_, i) => i !== index) })
              }
            >
              <Trash2 aria-hidden className="size-4" />
            </Button>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Select
              ariaLabel={t('settings.notifications.channelKind')}
              value={channel.kind}
              onValueChange={(value) => {
                const kind = channelKindValues.find((candidate) => candidate === value);
                if (kind) update(index, { kind });
              }}
              options={channelKindValues.map((kind) => ({
                value: kind,
                label: t(channelKindLabels[kind]),
              }))}
            />
            <Select
              ariaLabel={t('settings.notifications.channelMinSeverity')}
              value={channel.minSeverity}
              onValueChange={(value) => {
                const severity = ruleSeverityValues.find((candidate) => candidate === value);
                if (severity) update(index, { minSeverity: severity });
              }}
              options={ruleSeverityValues.map((severity) => ({
                value: severity,
                label: t(ruleSeverityLabels[severity]),
              }))}
            />
            <Button
              variant="secondary"
              size="sm"
              disabled={testing === channel.id}
              onClick={() => void test(channel.id)}
            >
              {testing === channel.id
                ? t('settings.notifications.testing')
                : t('settings.notifications.test')}
            </Button>
          </div>
          <Input
            ariaLabel={t('settings.notifications.channelUrl')}
            placeholder={channelKindExamples[channel.kind]}
            value={channel.url}
            onChange={(event) => update(index, { url: event.target.value })}
          />
          {channelUsesSecret[channel.kind] && (
            <Input
              type="password"
              ariaLabel={t('settings.notifications.channelSecret')}
              value={channel.secret ?? ''}
              onChange={(event) =>
                update(index, { secret: event.target.value === '' ? null : event.target.value })
              }
            />
          )}
          {result?.id === channel.id && (
            <p
              role="status"
              className={`text-xs ${result.message === null ? 'text-[var(--color-success-text)]' : 'text-[var(--color-danger-text)]'}`}
            >
              {result.message ?? t('settings.notifications.testOk')}
            </p>
          )}
          {result?.id !== channel.id && lastResult[channel.id]?.lastOk === false && (
            <p role="status" className="text-xs text-[var(--color-danger-text)]">
              {lastResult[channel.id]?.lastStatus === null
                ? t('settings.notifications.lastUnreachable')
                : t('settings.notifications.lastFailed', {
                    status: lastResult[channel.id]?.lastStatus ?? 0,
                  })}
            </p>
          )}
        </div>
      ))}
    </>
  );
}

const channelKindValues = [
  'webhook',
  'ntfy',
  'discord',
  'homeAssistant',
] as const satisfies readonly ChannelKind[];

const channelKindLabels = {
  webhook: 'settings.notifications.kindWebhook',
  ntfy: 'settings.notifications.kindNtfy',
  discord: 'settings.notifications.kindDiscord',
  homeAssistant: 'settings.notifications.kindHomeAssistant',
} as const;

/** Mirrors `ChannelKind::example_url` in Rust: what the user is expected to
 * paste, so the field does not need a sentence to explain itself. */
const channelKindExamples = {
  webhook: 'https://example.com/hooks/vitals',
  ntfy: 'https://ntfy.sh/my-vitals-topic',
  discord: 'https://discord.com/api/webhooks/…',
  homeAssistant: 'https://home.example.com:8123/api/events/vitals_alert',
} as const;

/** Mirrors `ChannelKind::uses_secret`: Discord's credential is its URL. */
const channelUsesSecret = {
  webhook: true,
  ntfy: true,
  discord: false,
  homeAssistant: true,
} as const;

/**
 * The user's own rules: a reading, a side of a threshold, and how long it has
 * to hold.
 *
 * Edited in place rather than in a dialog because a rule is three short
 * decisions and a dialog for that is more ceremony than the thing it edits.
 * The engine receives these exactly as they are stored — the settings type is
 * the wire type — so there is no translation step that could drift.
 */
/**
 * A number from a text field, or `null` while the user is mid-edit.
 *
 * The empty string is the case that matters and the one that reads as safe:
 * `Number('')` is **`0`**, not `NaN`, so a cleared threshold would silently
 * become zero — and a rule that says "CPU above 0 %" fires the moment it is
 * saved. Rejecting the empty field explicitly, rather than trusting a
 * finiteness check to catch it, is the whole point of this function.
 */
function typedNumber(raw: string): number | null {
  if (raw.trim() === '') return null;
  const value = Number(raw);
  return Number.isFinite(value) ? value : null;
}

function AlertRulesSection() {
  const { t } = useTranslation(SHELL_NS);
  const rules = useSettings((state) => state.settings.alertRules);
  const patch = useSettings((state) => state.patch);

  const update = (index: number, next: Partial<AlertRule>) => {
    patch({ alertRules: rules.map((rule, i) => (i === index ? { ...rule, ...next } : rule)) });
  };

  return (
    <>
      <div className="flex items-center justify-between pt-4">
        <div>
          <p className="text-sm font-medium">{t('settings.notifications.rules')}</p>
          <p className="text-xs text-[var(--color-fg-muted)]">
            {t('settings.notifications.rulesHint')}
          </p>
        </div>
        <Button
          variant="secondary"
          size="sm"
          onClick={() =>
            patch({
              alertRules: [
                ...rules,
                {
                  // Unique without a clock: a counter over what is already
                  // there cannot repeat, and `Date.now()` in a render path is
                  // what the compiler rejects anyway.
                  id: `rule-${rules.length}-${rules.reduce((n, r) => n + r.id.length, 0)}`,
                  name: t('settings.notifications.newRule'),
                  metric: 'cpuPercent',
                  comparator: 'above',
                  threshold: 90,
                  sustainSecs: 30,
                  cooldownSecs: 300,
                  severity: 'warning',
                  // Every channel, which is what a rule written before
                  // channels existed behaves as.
                  channels: [],
                },
              ],
            })
          }
        >
          {t('settings.notifications.addRule')}
        </Button>
      </div>

      {rules.map((rule, index) => (
        <div key={rule.id} className="grid gap-2 border-t border-[var(--color-border-subtle)] pt-3">
          <div className="flex items-center gap-2">
            <Input
              ariaLabel={t('settings.notifications.ruleName')}
              value={rule.name}
              onChange={(event) => update(index, { name: event.target.value })}
            />
            <Button
              variant="secondary"
              size="sm"
              aria-label={t('settings.notifications.removeRule')}
              onClick={() => patch({ alertRules: rules.filter((_, i) => i !== index) })}
            >
              <Trash2 aria-hidden className="size-4" />
            </Button>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Select
              ariaLabel={t('settings.notifications.ruleMetric')}
              value={rule.metric}
              onValueChange={(value) => {
                const metric = ruleMetricValues.find((candidate) => candidate === value);
                if (metric) update(index, { metric });
              }}
              options={ruleMetricValues.map((metric) => ({
                value: metric,
                label: t(ruleMetricLabels[metric]),
              }))}
            />
            <Select
              ariaLabel={t('settings.notifications.ruleComparator')}
              value={rule.comparator}
              onValueChange={(value) => {
                const comparator = comparatorValues.find((candidate) => candidate === value);
                if (comparator) update(index, { comparator });
              }}
              options={comparatorValues.map((comparator) => ({
                value: comparator,
                label: t(comparatorLabels[comparator]),
              }))}
            />
            <Input
              type="number"
              ariaLabel={t('settings.notifications.ruleThreshold')}
              value={String(rule.threshold)}
              onChange={(event) => {
                const next = typedNumber(event.target.value);
                // A field being retyped is not an instruction to change
                // anything. See `typedNumber` for why the empty string is
                // the dangerous case rather than an unlikely one.
                if (next !== null) update(index, { threshold: next });
              }}
            />
            <Input
              type="number"
              ariaLabel={t('settings.notifications.ruleSustain')}
              value={String(rule.sustainSecs)}
              onChange={(event) => {
                const next = typedNumber(event.target.value);
                if (next !== null) update(index, { sustainSecs: Math.max(0, next) });
              }}
            />
            <Select
              ariaLabel={t('settings.notifications.ruleSeverity')}
              value={rule.severity}
              onValueChange={(value) => {
                const severity = ruleSeverityValues.find((candidate) => candidate === value);
                if (severity) update(index, { severity });
              }}
              options={ruleSeverityValues.map((severity) => ({
                value: severity,
                label: t(ruleSeverityLabels[severity]),
              }))}
            />
          </div>
        </div>
      ))}
    </>
  );
}

const ruleMetricValues = [
  'cpuPercent',
  'cpuTemperature',
  'memoryPercent',
  'diskActivePercent',
  'batteryPercent',
  'networkErrorsPerSec',
] as const satisfies readonly RuleMetric[];

/** Spelled out rather than assembled: a built key renders its own name the
 * day a value is renamed. */
const ruleMetricLabels = {
  cpuPercent: 'settings.notifications.metricCpuPercent',
  cpuTemperature: 'settings.notifications.metricCpuTemperature',
  memoryPercent: 'settings.notifications.metricMemoryPercent',
  diskActivePercent: 'settings.notifications.metricDiskActivePercent',
  batteryPercent: 'settings.notifications.metricBatteryPercent',
  networkErrorsPerSec: 'settings.notifications.metricNetworkErrorsPerSec',
} as const;

const comparatorValues = ['above', 'below'] as const satisfies readonly Comparator[];
const comparatorLabels = {
  above: 'settings.notifications.comparatorAbove',
  below: 'settings.notifications.comparatorBelow',
} as const;

const ruleSeverityValues = [
  'info',
  'warning',
  'critical',
] as const satisfies readonly RuleSeverity[];
const ruleSeverityLabels = {
  info: 'settings.notifications.severityInfo',
  warning: 'settings.notifications.severityWarning',
  critical: 'settings.notifications.severityCritical',
} as const;

export function PrivacyPanel() {
  const { t } = useTranslation();
  const store = useStore();

  // No switches here. Vitals has no telemetry, no crash reporting and no
  // online reputation lookups, so there is nothing to opt out of. The one
  // automatic request — the update check — has its switch beside the update
  // controls in About, and this panel says so.
  return (
    <SettingsSection title={t('settings.privacy.title')}>
      <div className="space-y-2 py-2 text-sm">
        <p className="font-medium">{t('settings.privacy.noTelemetry')}</p>
        <ul className="list-disc space-y-1 pl-5 text-2xs text-[var(--color-fg-muted)]">
          <li>{t('settings.privacy.localOnly')}</li>
          <li>{t('settings.privacy.noCrashReports')}</li>
          <li>{t('settings.privacy.noLookups')}</li>
          <li>
            {t(
              store === 'macAppStore'
                ? 'settings.privacy.updatesMacAppStore'
                : store === 'microsoft'
                  ? 'settings.privacy.updatesStore'
                  : 'settings.privacy.updates',
            )}
          </li>
          <li>{t('settings.privacy.lanServer')}</li>
        </ul>
        <Button
          variant="ghost"
          onClick={() =>
            void reportFailure(openExternal(legalLinks.privacyPolicy), t('settings.privacy.policy'))
          }
        >
          {t('settings.privacy.policy')}
        </Button>
      </div>
    </SettingsSection>
  );
}

export function AboutPanel({ version }: { readonly version: string }) {
  const { t } = useTranslation();
  const { info, pending } = useHostInfo();
  const autoUpdate = useSettings((state) => state.settings.autoUpdate);
  const patch = useSettings((state) => state.patch);
  const store = useStore();

  return (
    <SettingsSection title={t('settings.about.title')}>
      <div className="space-y-2 py-2">
        <p className="text-sm font-medium">{t('settings.about.version', { version })}</p>

        {/* Machine facts live here because this is where someone goes to
            file a bug, and "what is this computer" is the first thing any
            report needs. Copying them is one button rather than a hunt
            through five Windows dialogs. */}
        {pending ? (
          <Skeleton className="h-24 w-full" />
        ) : (
          info !== null && <HostFacts info={info} />
        )}

        {store !== null ? (
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t(
              store === 'macAppStore'
                ? 'settings.about.update.macAppStore'
                : 'settings.about.update.store',
            )}
          </p>
        ) : (
          <>
            <UpdateSection />

            <SettingsRow
              label={t('settings.about.update.auto')}
              description={t('settings.about.update.autoHint')}
            >
              {({ labelId, describedBy }) => (
                <Switch
                  aria-labelledby={labelId}
                  aria-describedby={describedBy}
                  checked={autoUpdate}
                  onCheckedChange={(value) => patch({ autoUpdate: value })}
                />
              )}
            </SettingsRow>
          </>
        )}

        <p className="text-sm">{t('settings.about.contribute')}</p>
        <p className="text-2xs text-[var(--color-fg-muted)]">
          {t('settings.about.contributeBody')}
        </p>
        <div className="flex flex-wrap gap-2 pt-1">
          <Button
            onClick={() =>
              void reportFailure(
                openExternal('https://github.com/dragoscv/vitals/issues'),
                t('settings.about.openIssues'),
              )
            }
          >
            {t('settings.about.openIssues')}
          </Button>
          <Button
            variant="ghost"
            onClick={() =>
              void reportFailure(
                openExternal('https://github.com/dragoscv/vitals'),
                t('settings.about.viewSource'),
              )
            }
          >
            {t('settings.about.viewSource')}
          </Button>
        </div>

        <p className="pt-2 text-sm font-medium">{t('settings.about.legal')}</p>
        <div className="flex flex-wrap gap-2">
          {(Object.keys(legalLinks) as (keyof typeof legalLinks)[]).map((key) => (
            <Button
              key={key}
              variant="ghost"
              onClick={() =>
                void reportFailure(openExternal(legalLinks[key]), t(`settings.about.${key}`))
              }
            >
              {t(`settings.about.${key}`)}
            </Button>
          ))}
        </div>
      </div>
    </SettingsSection>
  );
}

/**
 * The machine facts, plus a button that copies them.
 *
 * Formatted as plain `label: value` lines rather than a table, because the
 * copy button produces exactly what is on screen and that text has to paste
 * legibly into a GitHub issue.
 */
function HostFacts({ info }: { readonly info: HostInfo }) {
  const { t } = useTranslation();
  const { i18n } = useTranslation();

  const rows = useMemo(() => {
    const cores =
      info.physicalCores === info.logicalCores
        ? t('settings.about.coresUniform', { count: info.logicalCores })
        : t('settings.about.coresSplit', {
            physical: info.physicalCores,
            logical: info.logicalCores,
          });

    // Only meaningful on a hybrid part. On a uniform machine the backend
    // sends null rather than a list of identical entries, so there is
    // nothing to summarise and the row is omitted entirely.
    const hybrid =
      info.coreTopology === null
        ? null
        : t('settings.about.hybrid', {
            performance: info.coreTopology.filter((core) => core === 'performance').length,
            efficiency: info.coreTopology.filter((core) => core !== 'performance').length,
          });

    return [
      { label: t('settings.about.machine'), value: info.hostname },
      {
        label: t('settings.about.os'),
        value: `${info.osName} ${info.osVersion} (${t('settings.about.build', { build: info.kernelVersion })})`,
      },
      { label: t('settings.about.processor'), value: info.cpuModel },
      { label: t('settings.about.cores'), value: cores },
      ...(hybrid === null ? [] : [{ label: t('settings.about.coreTypes'), value: hybrid }]),
      { label: t('settings.about.memory'), value: formatBytes(info.totalMemory, i18n.language) },
      ...(info.motherboard === null
        ? []
        : [{ label: t('settings.about.motherboard'), value: info.motherboard }]),
      ...(info.biosVersion === null
        ? []
        : [{ label: t('settings.about.bios'), value: info.biosVersion }]),
    ];
  }, [info, t, i18n.language]);

  return (
    <div className="rounded-md border border-[var(--color-border-subtle)] p-2.5">
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-1">
        {rows.map((row) => (
          <Fragment key={row.label}>
            <dt className="text-2xs text-[var(--color-fg-muted)]">{row.label}</dt>
            <dd className="truncate font-mono text-2xs">{row.value}</dd>
          </Fragment>
        ))}
      </dl>

      <Button
        variant="ghost"
        size="sm"
        className="mt-2"
        onClick={() => {
          void globalThis.navigator?.clipboard?.writeText(
            rows.map((row) => `${row.label}: ${row.value}`).join('\n'),
          );
        }}
      >
        {t('settings.about.copyFacts')}
      </Button>
    </div>
  );
}

/**
 * Opens a URL in the user's browser.
 *
 * Through the opener plugin rather than `window.open`, which inside a WebView
 * would either be blocked by the CSP or load the page *inside the app* — a
 * webview that can be navigated to an arbitrary remote origin is the one
 * mistake that turns a local monitor into a remote-code-execution surface.
 */
async function openExternal(url: string): Promise<void> {
  if (!hasTauriHost()) return;
  const { openUrl } = await import('@tauri-apps/plugin-opener');
  await openUrl(url);
}
