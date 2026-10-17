/**
 * The welcome card: the three things most people want from Vitals first,
 * offered rather than done for them.
 *
 * The freeze watchdog used to switch itself on at the first launch and add
 * itself to sign-in without asking. It is opt-in now (ADR-0032, PRIVACY.md),
 * and this card is where the question is asked. Each choice is one press
 * that does exactly what its label says; Task Manager replacement opens the
 * setting rather than flipping a machine-wide registry value from here,
 * because Windows asks for permission and the user should see why.
 *
 * Shown until dismissed, and the dismissal is a setting like any other, so it
 * survives a restart and does not come back on every launch.
 */

import { Gauge, ShieldCheck, Stethoscope, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button, Card, CardBody, IconButton } from '@vitals/ui';

import { OPEN_SETTINGS_EVENT } from '../../shell/navigation';
import { hasTauriHost } from '../../shell/host';
import { useSettings } from '../../settings/store';
import { useIsStoreBuild } from '../../settings/useDistribution';
import {
  tauriApi as tauriTaskManagerApi,
  useTaskManagerReplacement,
  type TaskManagerReplacementApi,
} from '../../settings/useTaskManagerReplacement';
import {
  tauriWatchdogApi,
  turnOnWatchdog,
  type WatchdogApi,
  type WatchdogStatus,
} from '../../settings/watchdog';
import { DASHBOARD_NS } from './strings';

export interface FirstRunCardProps {
  readonly onDiagnose: () => void;
  /** Injectable so tests need no Tauri host. */
  readonly watchdogApi?: WatchdogApi;
  readonly taskManagerApi?: TaskManagerReplacementApi;
}

type WatchdogOffer =
  | { readonly state: 'unknown' }
  | { readonly state: 'missing' }
  | { readonly state: 'off'; readonly busy: boolean; readonly error: string | null }
  | { readonly state: 'on' };

function describe(failure: unknown): string {
  if (failure instanceof Error) return failure.message;
  if (typeof failure === 'object' && failure !== null && 'message' in failure) {
    const { message } = failure;
    if (typeof message === 'string') return message;
  }
  return String(failure);
}

/**
 * The gate. Split from the card so that once it is dismissed — which is
 * every launch after the first for most people — the dashboard does not
 * ask the registry and the watchdog for answers nobody will see.
 *
 * Before hydration the saved dismissal is not known yet; showing the card
 * and then removing it would be a flash on every launch for everyone who
 * already dismissed it.
 */
export function FirstRunCard(props: FirstRunCardProps): React.JSX.Element | null {
  const hydrated = useSettings((state) => state.hydrated);
  const dismissed = useSettings((state) => state.settings.firstRunDismissed);
  if (!hydrated || dismissed) return null;
  return <FirstRunCardBody {...props} />;
}

function FirstRunCardBody({
  onDiagnose,
  watchdogApi = tauriWatchdogApi,
  taskManagerApi = tauriTaskManagerApi,
}: FirstRunCardProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const patch = useSettings((state) => state.patch);
  const storeBuild = useIsStoreBuild();
  const taskManager = useTaskManagerReplacement(taskManagerApi);
  // Without a host there is nothing to ask, so the answer is known up front.
  const [watchdog, setWatchdog] = useState<WatchdogOffer>(() =>
    hasTauriHost() ? { state: 'unknown' } : { state: 'missing' },
  );

  useEffect(() => {
    if (!hasTauriHost()) return;
    let live = true;
    watchdogApi
      .status()
      .then((status: WatchdogStatus) => {
        if (!live) return;
        if (!status.available) setWatchdog({ state: 'missing' });
        else if (status.registered) setWatchdog({ state: 'on' });
        else setWatchdog({ state: 'off', busy: false, error: null });
      })
      // An unanswered status hides the offer: a button that may not work is
      // worse than one fewer choice on a card the user can ignore.
      .catch(() => {
        if (live) setWatchdog({ state: 'missing' });
      });
    return () => {
      live = false;
    };
  }, [watchdogApi]);

  const enableWatchdog = (): void => {
    setWatchdog({ state: 'off', busy: true, error: null });
    turnOnWatchdog(watchdogApi)
      .then((status) => {
        setWatchdog(
          status.registered ? { state: 'on' } : { state: 'off', busy: false, error: null },
        );
      })
      .catch((failure: unknown) => {
        setWatchdog({ state: 'off', busy: false, error: describe(failure) });
      });
  };

  // Only where the hook can work: a direct install (the Store build cannot
  // touch Task Manager), a host to ask, an answer, not already on, and no
  // other tool owning it.
  const offerTaskManager =
    hasTauriHost() &&
    !storeBuild &&
    taskManager.status !== null &&
    !taskManager.status.enabled &&
    taskManager.status.replacedBy === null;

  const headingId = 'first-run-title';

  return (
    <Card role="region" aria-labelledby={headingId} className="shrink-0">
      <CardBody className="flex flex-col gap-3 pt-3">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0">
            <h3 id={headingId} className="text-sm font-semibold">
              {t('firstRun.title')}
            </h3>
            <p className="text-2xs text-[var(--color-fg-muted)]">{t('firstRun.body')}</p>
          </div>
          <IconButton
            label={t('firstRun.dismissLabel')}
            icon={<X />}
            variant="ghost"
            // Full size: this is the only way to dismiss, so the 24 px
            // exemption for row actions does not apply.
            size="md"
            onClick={() => {
              patch({ firstRunDismissed: true });
            }}
          />
        </div>

        <ul className="flex flex-col gap-2.5">
          <li className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <Button variant="secondary" size="sm" onClick={onDiagnose}>
              <Stethoscope aria-hidden className="size-4" />
              {t('firstRun.diagnose')}
            </Button>
            <span className="text-2xs text-[var(--color-fg-muted)]">
              {t('firstRun.diagnoseHint')}
            </span>
          </li>

          {watchdog.state === 'off' && (
            <li className="flex flex-col gap-1">
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                <Button
                  variant="secondary"
                  size="sm"
                  loading={watchdog.busy}
                  loadingLabel={t('firstRun.watchdog')}
                  aria-describedby="first-run-watchdog-hint"
                  onClick={enableWatchdog}
                >
                  <ShieldCheck aria-hidden className="size-4" />
                  {t('firstRun.watchdog')}
                </Button>
                <span
                  id="first-run-watchdog-hint"
                  className="text-2xs text-[var(--color-fg-muted)]"
                >
                  {t('firstRun.watchdogHint')}
                </span>
              </div>
              {watchdog.error !== null && (
                <p role="alert" className="text-2xs text-[var(--color-danger-text)]">
                  {t('firstRun.watchdogFailed', { reason: watchdog.error })}
                </p>
              )}
            </li>
          )}
          {watchdog.state === 'on' && (
            <li role="status" className="flex items-center gap-2 text-2xs">
              <ShieldCheck aria-hidden className="size-4 text-[var(--color-ok-text)]" />
              {t('firstRun.watchdogOn')}
            </li>
          )}

          {offerTaskManager && (
            <li className="flex flex-wrap items-center gap-x-3 gap-y-1">
              <Button
                variant="ghost"
                size="sm"
                aria-describedby="first-run-taskmgr-hint"
                onClick={() => {
                  window.dispatchEvent(new Event(OPEN_SETTINGS_EVENT));
                }}
              >
                <Gauge aria-hidden className="size-4" />
                {t('firstRun.taskManager')}
              </Button>
              <span id="first-run-taskmgr-hint" className="text-2xs text-[var(--color-fg-muted)]">
                {t('firstRun.taskManagerHint')}
              </span>
            </li>
          )}
        </ul>
      </CardBody>
    </Card>
  );
}
