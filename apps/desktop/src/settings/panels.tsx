import { useTranslation } from 'react-i18next';

import { locales, type Locale } from '@vitals/i18n';
import { Button, SegmentedControl, Select, Switch, cn, focusRing } from '@vitals/ui';

import { SHELL_NS } from '../shell/strings';
import { hasTauriHost } from '../shell/host';
import { useTheme } from '../theme/ThemeProvider';
import { accents, densities, surfaces, themeModes, type Accent } from '../theme/types';
import { SettingsRow, SettingsSection } from './SettingsRow';
import { retentionDayOptions, samplingRates } from './schema';
import { useSettings } from './store';

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

  const surfaceLabels = {
    solid: t('settings.appearance.surfaceSolid'),
    mica: t('settings.appearance.surfaceMica'),
    acrylic: t('settings.appearance.surfaceAcrylic'),
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

        <SettingsRow label={t('settings.appearance.surface')}>
          {() => (
            <SegmentedControl
              ariaLabel={t('settings.appearance.surface')}
              value={theme.surface}
              onValueChange={(surface) => setTheme({ surface })}
              options={surfaces.map((surface) => ({
                value: surface,
                label: surfaceLabels[surface],
              }))}
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
              options={densities.map((density) => ({ value: density, label: density }))}
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
  const settings = useSettings((state) => state.settings);
  const patch = useSettings((state) => state.patch);

  return (
    <SettingsSection title={t('settings.privacy.title')}>
      <SettingsRow
        label={t('settings.privacy.crashReports')}
        description={t('settings.privacy.crashReportsHint')}
      >
        {({ labelId, describedBy }) => (
          <Switch
            aria-labelledby={labelId}
            aria-describedby={describedBy}
            checked={settings.crashReports}
            onCheckedChange={(value) => patch({ crashReports: value })}
          />
        )}
      </SettingsRow>

      <SettingsRow label={t('settings.privacy.usageData')}>
        {({ labelId }) => (
          <Switch
            aria-labelledby={labelId}
            checked={settings.usageData}
            onCheckedChange={(value) => patch({ usageData: value })}
          />
        )}
      </SettingsRow>

      <SettingsRow
        label={t('settings.privacy.reputationLookups')}
        description={t('settings.privacy.reputationLookupsHint')}
      >
        {({ labelId, describedBy }) => (
          <Switch
            aria-labelledby={labelId}
            aria-describedby={describedBy}
            checked={settings.reputationLookups}
            onCheckedChange={(value) => patch({ reputationLookups: value })}
          />
        )}
      </SettingsRow>
    </SettingsSection>
  );
}

export function AdvancedPanel() {
  const { t } = useTranslation();
  const settings = useSettings((state) => state.settings);
  const patch = useSettings((state) => state.patch);

  return (
    <SettingsSection title={t('settings.advanced.title')}>
      {/*
       * `role="alert"` is deliberately not used: the warning is present from
       * the moment the tab opens rather than appearing in response to an
       * action, and an alert fired on render interrupts whatever the screen
       * reader was saying. It is a described-by on the switch instead, so it
       * is read at the point the decision is actually made.
       */}
      <p
        id="advanced-warning"
        className="text-2xs rounded-[var(--radius-control)] border border-[var(--color-status-danger)] bg-[var(--color-bg-inset)] p-2.5 text-[var(--color-fg-default)]"
      >
        {t('settings.advanced.warning')}
      </p>

      <SettingsRow label={t('settings.advanced.enable')}>
        {({ labelId }) => (
          <Switch
            aria-labelledby={labelId}
            aria-describedby="advanced-warning"
            checked={settings.advancedEnabled}
            onCheckedChange={(value) => patch({ advancedEnabled: value })}
          />
        )}
      </SettingsRow>
    </SettingsSection>
  );
}

export function AboutPanel({ version }: { readonly version: string }) {
  const { t } = useTranslation();

  return (
    <SettingsSection title={t('settings.about.title')}>
      <div className="space-y-2 py-2">
        <p className="text-sm font-medium">{t('settings.about.version', { version })}</p>
        <p className="text-sm">{t('settings.about.contribute')}</p>
        <p className="text-2xs text-[var(--color-fg-muted)]">
          {t('settings.about.contributeBody')}
        </p>
        <div className="flex flex-wrap gap-2 pt-1">
          <Button onClick={() => void openExternal('https://github.com/dragoscv/vitals/issues')}>
            {t('settings.about.openIssues')}
          </Button>
          <Button
            variant="ghost"
            onClick={() => void openExternal('https://github.com/dragoscv/vitals')}
          >
            {t('settings.about.viewSource')}
          </Button>
        </div>
      </div>
    </SettingsSection>
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
