/**
 * The attention panel.
 *
 * Renders what the Rust engine decided, and nothing more — no thresholds or
 * severity logic here. The rules live in `crates/vitals-core/src/alerts.rs`
 * so that one evaluation feeds this panel, the tray, a Windows toast and the
 * LAN API rather than four that can disagree.
 *
 * # Saying "nothing is wrong" is the point
 *
 * The empty state is not a fallback, it is the widget's most common and most
 * valuable output. A user who opens a system monitor because the machine feels
 * slow needs to be told that CPU, memory, disk, temperature and network were
 * all checked and are fine — that is what redirects them from guessing at
 * hardware to looking at a specific application. A blank panel says only that
 * the tool has nothing to say.
 */

import { AlertTriangle, CheckCircle2, Info, XCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import type { Alert, Severity } from '@vitals/protocol';
import { Button, cn } from '@vitals/ui';

import type { RouteId } from '../../../shell/navigation';
import { alertCause, alertTitle } from '../../alerts/alertText';
import { DASHBOARD_NS } from '../strings';

const ICON: Readonly<Record<Severity, typeof Info>> = {
  critical: XCircle,
  warning: AlertTriangle,
  info: Info,
};

const TONE: Readonly<Record<Severity, string>> = {
  critical: 'text-[var(--color-danger-fg)]',
  warning: 'text-[var(--color-warning-fg)]',
  info: 'text-[var(--color-fg-muted)]',
};

export interface AlertsWidgetProps {
  readonly alerts: readonly Alert[];
  readonly onNavigate: (route: RouteId) => void;
}

export function AlertsWidget({ alerts, onNavigate }: AlertsWidgetProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);

  if (alerts.length === 0) {
    return (
      <div className="flex items-start gap-2.5">
        <CheckCircle2
          aria-hidden
          className="mt-0.5 size-4 shrink-0 text-[var(--color-success-fg)]"
        />
        <div>
          <p className="text-sm font-medium">{t('alert.none')}</p>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('alert.noneBody')}</p>
        </div>
      </div>
    );
  }

  return (
    <ul
      className="flex flex-col gap-2.5"
      // Polite, and only because this list changes rarely — unlike the meters,
      // which update every second and would make a live region unusable.
      aria-live="polite"
    >
      {alerts.map((alert) => {
        const Icon = ICON[alert.severity];
        return (
          // Keyed on kind *and* subject: two disks can be nearly full at once,
          // and keying on kind alone would collapse them into one row that
          // flickers between drive letters.
          <li key={`${alert.kind}:${alert.subject}`} className="flex items-start gap-2.5">
            <Icon aria-hidden className={cn('mt-0.5 size-4 shrink-0', TONE[alert.severity])} />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium">{alertTitle(alert, t)}</p>
              {/* The cause, not a restatement of the number above it. This is
                  the sentence that makes the alert actionable. */}
              <p className="text-2xs text-[var(--color-fg-muted)]">{alertCause(alert, t)}</p>
            </div>
            {alert.route !== null && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  onNavigate(alert.route as RouteId);
                }}
              >
                {t('alert.investigate')}
              </Button>
            )}
          </li>
        );
      })}
    </ul>
  );
}
