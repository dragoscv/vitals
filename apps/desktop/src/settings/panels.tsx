import { Fragment, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { locales, type Locale } from '@vitals/i18n';
import type { HostInfo } from '@vitals/protocol';
import {
  Button,
  SegmentedControl,
  Select,
  Skeleton,
  Switch,
  cn,
  focusRing,
  formatBytes,
} from '@vitals/ui';

import { SHELL_NS } from '../shell/strings';
import { hasTauriHost } from '../shell/host';
import { openWindowsTaskManager, quitApp } from '../lib/settingsSync';
import { reportFailure } from '../lib/reportFailure';
import { useTheme } from '../theme/ThemeProvider';
import { accents, densities, themeModes, type Accent } from '../theme/types';
import { SettingsRow, SettingsSection } from './SettingsRow';
import { clearHistory, exportFlightRecording } from './historyActions';
import { retentionDayOptions, samplingRates } from './schema';
import { useSettings } from './store';
import { useHostInfo } from './useHostInfo';
import { useHistoryUsage } from './useHistoryUsage';
import { useTaskManagerReplacement } from './useTaskManagerReplacement';
import { useIsStoreBuild } from './useDistribution';
import { UpdateSection } from './UpdatePanel';

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

export function GeneralPanel() {
  const { t } = useTranslation(SHELL_NS);
  const settings = useSettings((state) => state.settings);
  const patch = useSettings((state) => state.patch);
  const taskManager = useTaskManagerReplacement();
  const storeBuild = useIsStoreBuild();

  // Three reasons the switch cannot move, each shown rather than implied:
  // the registry has not answered yet, a write is in flight, or another
  // tool owns the hook and we refuse to take it from them.
  const taskManagerLocked =
    taskManager.status === null || taskManager.busy || taskManager.status.replacedBy !== null;
  const taskManagerHint =
    taskManager.status?.replacedBy !== null && taskManager.status?.replacedBy !== undefined
      ? t('settings.general.replaceTaskManagerOwned', {
          debugger: taskManager.status.replacedBy,
        })
      : t('settings.general.replaceTaskManagerHint');

  return (
    <SettingsSection title={t('settings.general.title')}>
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

      {/* The Store package cannot write HKLM, so the hook would do nothing there. */}
      {!storeBuild && (
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
    </SettingsSection>
  );
}

export function PrivacyPanel() {
  const { t } = useTranslation();
  const storeBuild = useIsStoreBuild();

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
          <li>{t(storeBuild ? 'settings.privacy.updatesStore' : 'settings.privacy.updates')}</li>
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
  const storeBuild = useIsStoreBuild();

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

        {storeBuild ? (
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t('settings.about.update.store')}
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
