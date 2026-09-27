/**
 * The verdict screen.
 *
 * One sentence, the applications behind it, the last minute of the metric it
 * names, and a Copy button. Deliberately not a second dashboard: the person
 * opening this has already looked at the meters and still does not know what
 * to do. The answer is a name they can act on, or a clear "nothing is wrong"
 * so they stop suspecting the hardware.
 *
 * The verdict is asked for once, when the dialog opens, against the frame the
 * dashboard already holds. It does not refresh while open — a sentence that
 * rewrites itself every second is unreadable, and the chart underneath is
 * live, which conveys "still happening" without churning the words.
 */

import { AlertTriangle, CheckCircle2, ClipboardCheck, Copy, Info, XCircle } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { TimeSeriesChart } from '@vitals/charts';
import type { Diagnosis, Process, Severity, SystemMetrics } from '@vitals/protocol';
import { Button, DialogContent, DialogRoot, formatPercent, Spinner } from '@vitals/ui';

import { alertCause, alertTitle } from '../alerts/alertText';
import type { MetricHistory } from '../dashboard/history';
import { DASHBOARD_NS } from '../dashboard/strings';
import { useThemeColors } from '../dashboard/widgets/useThemeColors';
import { formatContributorValue, formatReport, isPercentSubsystem, seriesFor } from './report';
import type { DiagnosisSource } from './source';

const ICON: Readonly<Record<Severity, typeof Info>> = {
  critical: XCircle,
  warning: AlertTriangle,
  info: Info,
};

const TONE: Readonly<Record<Severity, string>> = {
  critical: 'text-[var(--color-status-danger)]',
  warning: 'text-[var(--color-status-warn)]',
  info: 'text-[var(--color-fg-muted)]',
};

/** How long the button reads "Copied" before reverting. */
export const COPIED_FEEDBACK_MS = 1600;

type Status =
  | { readonly kind: 'working' }
  | { readonly kind: 'failed' }
  | { readonly kind: 'ready'; readonly diagnosis: Diagnosis; readonly at: Date };

export interface DiagnosisDialogProps {
  readonly open: boolean;
  readonly onOpenChange: (open: boolean) => void;
  readonly source: DiagnosisSource;
  readonly system: SystemMetrics | null;
  readonly processes: ReadonlyMap<string, Process>;
  readonly history: MetricHistory;
  readonly locale: string;
  /** Injected so a test can assert on the text without a real clipboard. */
  readonly writeClipboard?: (text: string) => Promise<void>;
}

export function DiagnosisDialog({
  open,
  onOpenChange,
  source,
  system,
  processes,
  history,
  locale,
  writeClipboard = defaultWriteClipboard,
}: DiagnosisDialogProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);

  return (
    <DialogRoot open={open} onOpenChange={onOpenChange}>
      {/* Mounted only while open, so every opening starts from a fresh
          request rather than showing last week's verdict for a frame. */}
      {open && (
        <Body
          source={source}
          system={system}
          processes={processes}
          history={history}
          locale={locale}
          writeClipboard={writeClipboard}
          closeLabel={t('diagnosis.close')}
        />
      )}
    </DialogRoot>
  );
}

function Body({
  source,
  system,
  processes,
  history,
  locale,
  writeClipboard,
  closeLabel,
}: {
  readonly source: DiagnosisSource;
  readonly system: SystemMetrics | null;
  readonly processes: ReadonlyMap<string, Process>;
  readonly history: MetricHistory;
  readonly locale: string;
  readonly writeClipboard: (text: string) => Promise<void>;
  readonly closeLabel: string;
}): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  // No frame means no question can be asked; that is known at mount.
  const [status, setStatus] = useState<Status>(() =>
    system === null ? { kind: 'failed' } : { kind: 'working' },
  );
  const [copied, setCopied] = useState(false);

  // Asked once, at mount. `system`/`processes` are intentionally not in the
  // dependency list: they change every second and the verdict must not.
  useEffect(() => {
    if (system === null) return;
    let cancelled = false;
    source
      .diagnose(system, [...processes.values()])
      .then((diagnosis) => {
        if (!cancelled) setStatus({ kind: 'ready', diagnosis, at: new Date() });
      })
      .catch(() => {
        if (!cancelled) setStatus({ kind: 'failed' });
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- see comment above
  }, [source]);

  useEffect(() => {
    if (!copied) return;
    const handle = setTimeout(() => {
      setCopied(false);
    }, COPIED_FEEDBACK_MS);
    return () => {
      clearTimeout(handle);
    };
  }, [copied]);

  const copy = (): void => {
    if (status.kind !== 'ready') return;
    const text = formatReport(status.diagnosis, {
      t,
      locale,
      system,
      generatedAt: status.at,
    });
    writeClipboard(text)
      .then(() => {
        setCopied(true);
      })
      .catch(() => {
        // The clipboard API refuses when the document is not focused (seen
        // live when the window was driven from outside). Saying "Copied"
        // would be a lie; leaving the button as it was invites a retry.
        setCopied(false);
      });
  };

  return (
    <DialogContent
      size="lg"
      title={t('diagnosis.title')}
      description={t('diagnosis.description')}
      closeLabel={closeLabel}
      footer={
        <Button
          variant="ghost"
          size="sm"
          onClick={copy}
          disabled={status.kind !== 'ready'}
          aria-live="polite"
        >
          {copied ? (
            <ClipboardCheck aria-hidden className="size-4" />
          ) : (
            <Copy aria-hidden className="size-4" />
          )}
          {copied ? t('diagnosis.copied') : t('diagnosis.copy')}
        </Button>
      }
    >
      {status.kind === 'working' && (
        <div className="flex items-center gap-2 py-6 text-sm text-[var(--color-fg-muted)]">
          <Spinner />
          {t('diagnosis.working')}
        </div>
      )}
      {status.kind === 'failed' && (
        <p role="alert" className="py-4 text-sm">
          {t('diagnosis.failed')}
        </p>
      )}
      {status.kind === 'ready' && (
        <Verdict diagnosis={status.diagnosis} history={history} locale={locale} />
      )}
    </DialogContent>
  );
}

function Verdict({
  diagnosis,
  history,
  locale,
}: {
  readonly diagnosis: Diagnosis;
  readonly history: MetricHistory;
  readonly locale: string;
}): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const colors = useThemeColors();
  const verdict = diagnosis.verdict;

  if (verdict === null) {
    return (
      <div className="flex flex-col gap-4">
        <div className="flex items-start gap-3">
          <CheckCircle2
            aria-hidden
            className="mt-0.5 size-5 shrink-0 text-[var(--color-status-ok)]"
          />
          <div>
            <p className="text-base font-semibold">{t('diagnosis.healthy.title')}</p>
            <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
              {t('diagnosis.healthy.body')}
            </p>
          </div>
        </div>
        <Also alerts={diagnosis.also} />
      </div>
    );
  }

  const Icon = ICON[verdict.alert.severity];
  const series = seriesFor(verdict.subsystem, history);
  const scale = isPercentSubsystem(verdict.subsystem) ? { min: 0, max: 100 } : undefined;

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-start gap-3">
        <Icon aria-hidden className={`mt-0.5 size-5 shrink-0 ${TONE[verdict.alert.severity]}`} />
        <div className="min-w-0">
          <p className="text-base font-semibold">{alertTitle(verdict.alert, t)}</p>
          <p className="mt-1 text-sm text-[var(--color-fg-muted)]">
            {alertCause(verdict.alert, t)}
          </p>
        </div>
      </div>

      <section aria-labelledby="diagnosis-responsible">
        <h3
          id="diagnosis-responsible"
          className="mb-1.5 text-2xs font-medium tracking-wide text-[var(--color-fg-muted)] uppercase"
        >
          {t('diagnosis.responsible')}
        </h3>
        {verdict.contributors.length > 0 ? (
          <ol className="flex flex-col gap-1.5">
            {verdict.contributors.map((c) => (
              <li
                key={`${c.name}:${String(c.pid)}`}
                className="flex items-baseline justify-between gap-3 text-sm"
              >
                <span className="min-w-0 truncate">
                  <span className="font-medium">{c.name}</span>
                  <span className="ml-1.5 text-2xs text-[var(--color-fg-muted)]">PID {c.pid}</span>
                </span>
                <span className="shrink-0 tabular-nums">
                  <span className="font-medium">
                    {t('diagnosis.share', { percent: formatPercent(c.share, locale, 0) })}
                  </span>
                  <span className="ml-1.5 text-2xs text-[var(--color-fg-muted)]">
                    {formatContributorValue(verdict.subsystem, c, locale)}
                  </span>
                </span>
              </li>
            ))}
          </ol>
        ) : (
          <p className="text-sm text-[var(--color-fg-muted)]">
            {verdict.diffuse ? t('diagnosis.diffuse') : t('diagnosis.noProcessData')}
          </p>
        )}
      </section>

      {series !== null && (
        <section aria-labelledby="diagnosis-chart">
          <h3
            id="diagnosis-chart"
            className="mb-1.5 text-2xs font-medium tracking-wide text-[var(--color-fg-muted)] uppercase"
          >
            {t('diagnosis.lastMinute')}
          </h3>
          <TimeSeriesChart
            series={[{ buffer: series, color: colors.accent, fillOpacity: 0.18, headDot: true }]}
            revision={history.revision}
            {...(scale !== undefined && { scale })}
            className="h-24 w-full"
            ariaLabel={t('diagnosis.lastMinute')}
          />
        </section>
      )}

      <Also alerts={diagnosis.also} />
    </div>
  );
}

function Also({ alerts }: { readonly alerts: Diagnosis['also'] }): React.JSX.Element | null {
  const { t } = useTranslation(DASHBOARD_NS);
  if (alerts.length === 0) return null;
  return (
    <section aria-labelledby="diagnosis-also">
      <h3
        id="diagnosis-also"
        className="mb-1.5 text-2xs font-medium tracking-wide text-[var(--color-fg-muted)] uppercase"
      >
        {t('diagnosis.also')}
      </h3>
      <ul className="flex flex-col gap-1.5">
        {alerts.map((alert) => {
          const Icon = ICON[alert.severity];
          return (
            <li key={`${alert.kind}:${alert.subject}`} className="flex items-start gap-2 text-sm">
              <Icon aria-hidden className={`mt-0.5 size-4 shrink-0 ${TONE[alert.severity]}`} />
              <span>
                <span className="font-medium">{alertTitle(alert, t)}</span>{' '}
                <span className="text-[var(--color-fg-muted)]">{alertCause(alert, t)}</span>
              </span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}

function defaultWriteClipboard(text: string): Promise<void> {
  const clipboard = globalThis.navigator?.clipboard;
  if (clipboard === undefined) return Promise.reject(new Error('clipboard unavailable'));
  return clipboard.writeText(text);
}
