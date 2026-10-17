/**
 * The dashboard's front door: one line saying how the machine is doing, and
 * the button for the question most people open a system monitor to ask.
 *
 * The verdict is the Rust alert engine's output and nothing else. No
 * thresholds here and no numbers of our own: "all clear" means the engine
 * checked and found nothing, and an alert is shown by the title the engine
 * chose. A strip that computed its own verdict would sooner or later
 * disagree with the Attention widget beside it.
 *
 * Static on purpose: nothing here animates, so there is nothing for reduced
 * motion to turn off. The verdict changes rarely, which is also why it can
 * be a polite live region while the meters below cannot.
 */

import { AlertTriangle, CheckCircle2, Info, Loader2, Stethoscope, XCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import type { Alert, Severity } from '@vitals/protocol';
import { Button, cn } from '@vitals/ui';

import { alertTitle } from '../alerts/alertText';
import { DASHBOARD_NS } from './strings';

const RANK: Readonly<Record<Severity, number>> = { critical: 0, warning: 1, info: 2 };

const ICON: Readonly<Record<Severity, typeof Info>> = {
  critical: XCircle,
  warning: AlertTriangle,
  info: Info,
};

const TONE: Readonly<Record<Severity, string>> = {
  critical: 'text-[var(--color-danger-text)]',
  warning: 'text-[var(--color-warn-text)]',
  info: 'text-[var(--color-fg-muted)]',
};

/** The alert the strip names: the most severe, then the oldest. */
export function topAlert(alerts: readonly Alert[]): Alert | null {
  let top: Alert | null = null;
  for (const alert of alerts) {
    if (
      top === null ||
      RANK[alert.severity] < RANK[top.severity] ||
      (RANK[alert.severity] === RANK[top.severity] && alert.sinceSample < top.sinceSample)
    ) {
      top = alert;
    }
  }
  return top;
}

export interface HealthStripProps {
  readonly alerts: readonly Alert[];
  /** False until the first frame: no verdict is given about a machine not yet measured. */
  readonly measured: boolean;
  readonly onDiagnose: () => void;
}

export function HealthStrip({ alerts, measured, onDiagnose }: HealthStripProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const top = measured ? topAlert(alerts) : null;

  const Icon = !measured ? Loader2 : top === null ? CheckCircle2 : ICON[top.severity];
  const tone = !measured
    ? 'text-[var(--color-fg-muted)]'
    : top === null
      ? 'text-[var(--color-ok-text)]'
      : TONE[top.severity];
  const verdict = !measured
    ? t('health.waiting')
    : top === null
      ? t('health.clear')
      : t('health.attention', { title: alertTitle(top, t) });

  return (
    <section
      aria-label={t('health.label')}
      className="flex shrink-0 flex-wrap items-center gap-3 rounded-[var(--radius-widget)] border border-[var(--color-border-subtle)] bg-[var(--color-bg-raised)] px-4 py-2.5"
    >
      <p role="status" aria-live="polite" className="flex min-w-0 flex-1 items-center gap-2.5">
        <Icon aria-hidden className={cn('size-4 shrink-0', tone)} />
        <span className="text-sm font-medium">{verdict}</span>
      </p>
      <Button variant="primary" size="md" disabled={!measured} onClick={onDiagnose}>
        <Stethoscope aria-hidden className="size-4" />
        {t('diagnosis.ask')}
      </Button>
    </section>
  );
}
