import { AlertTriangle, CheckCircle2, Info, XCircle } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import {
  busiestNic,
  headlineTemperature,
  primaryGpu,
  type Alert,
  type Severity,
  type SystemMetrics,
} from '@vitals/protocol';
import {
  Badge,
  Button,
  Card,
  Skeleton,
  cn,
  formatBytes,
  formatPercent,
  formatTemperature,
  formatThroughput,
} from '@vitals/ui';

import {
  formatAgo,
  readingAgeMs,
  STALE_AFTER_MS,
  useNow,
  type ConnectionState,
  type LiveState,
} from '../lib/live';
import type { Pairing } from '../lib/pairing';
import { MOBILE_ALERTS_NS } from '../strings';
import { Sparkline } from './Sparkline';

export interface MachineCardProps {
  readonly pairing: Pairing;
  readonly live: LiveState;
  readonly onOpen: (id: string) => void;
  /** Forgets the dead pairing so a fresh code can be scanned. */
  readonly onPairAgain?: (id: string) => void;
}

const TONE: Record<ConnectionState, 'ok' | 'warn' | 'danger' | 'neutral'> = {
  live: 'ok',
  connecting: 'neutral',
  reconnecting: 'warn',
  unreachable: 'danger',
  // A revoked key is not the network's fault and will not fix itself, so it
  // reads with the same urgency as an unreachable PC but a different action.
  unauthorised: 'danger',
};

const SEVERITY_ICON: Readonly<Record<Severity, typeof Info>> = {
  critical: XCircle,
  warning: AlertTriangle,
  info: Info,
};

const SEVERITY_TONE: Readonly<Record<Severity, string>> = {
  critical: 'text-[var(--color-danger-text)]',
  warning: 'text-[var(--color-warn-text)]',
  info: 'text-[var(--color-fg-muted)]',
};

/** One paired PC at a glance. The whole card is the tap target. */
export function MachineCard({ pairing, live, onOpen, onPairAgain }: MachineCardProps) {
  const { t, i18n } = useTranslation();
  const locale = i18n.language;
  const system = live.snapshot?.system;
  const now = useNow();

  // gpus[0] was wrong: the kernel enumerates a virtual display first on
  // this machine, so the card showed a dash beside a GPU at 17 %. The shared
  // selectors pick the busiest MEASURED adapter and are tested once.
  const gpu = system === undefined ? undefined : (primaryGpu(system) ?? undefined);
  const nic = system === undefined ? undefined : busiestNic(system);
  const temperature = system === undefined ? null : headlineTemperature(system);
  // A reading that has gone quiet is the last one received, not a live view,
  // so an older-than-2×-tick figure is labelled rather than passed off as
  // current. Suppressed while unauthorised: the panel below already explains
  // why nothing is arriving.
  const age = readingAgeMs(live, now);
  const stale = live.state !== 'unauthorised' && age !== null && age >= STALE_AFTER_MS;

  return (
    <Card className="overflow-hidden">
      <button
        type="button"
        onClick={() => onOpen(pairing.id)}
        aria-label={t('mobile.machines.openProcesses', { name: pairing.name })}
        className="block min-h-[44px] w-full p-4 text-left active:bg-[var(--color-bg-subtle)]"
      >
        <div className="flex items-center justify-between gap-3">
          <h2 className="truncate text-base font-semibold">{pairing.name}</h2>
          <Badge tone={TONE[live.state]}>{t(`mobile.connection.${live.state}`)}</Badge>
        </div>

        {stale && (
          <p className="mt-2 text-2xs text-[var(--color-warn-text)]">
            {t('mobile.stale', { ago: formatAgo(age) })}
          </p>
        )}

        {live.state === 'unauthorised' ? null : system === undefined ? (
          <div className="mt-4 space-y-3" aria-busy="true">
            <Skeleton className="h-8 w-full" />
            <Skeleton className="h-8 w-full" />
            <p className="text-2xs text-[var(--color-fg-muted)]">{t('mobile.machines.waiting')}</p>
          </div>
        ) : (
          <div className="mt-3 space-y-3">
            <Trace
              label={t('mobile.machines.cpu')}
              value={formatPercent(system.cpu.total, locale, 0)}
              history={live.cpuHistory}
              stroke="var(--color-chart-cpu)"
            />
            <Trace
              label={t('mobile.machines.memory')}
              value={t('mobile.machines.memoryOf', {
                // A total of zero is "not reported", never "no memory", so the
                // line reads "— of —" rather than a false "0 B of 0 B".
                used: formatBytes(memoryReported(system) ? system.memory.used : null, locale),
                total: formatBytes(memoryReported(system) ? system.memory.total : null, locale),
              })}
              history={live.memoryHistory}
              stroke="var(--color-chart-memory)"
            />
            {/* Only what this machine has. The em-dash rule is for a reading
                that exists but was not measured this tick; a PC with no GPU
                has no GPU row, and a card that is one-third dashes reads as
                broken to anyone who does not know the rule. */}
            <dl
              className={cn(
                'grid gap-2 text-2xs',
                facts(gpu !== undefined, nic !== undefined, temperature !== null),
              )}
            >
              {gpu !== undefined && (
                <Fact
                  label={t('mobile.machines.gpu')}
                  value={formatPercent(gpu.utilization, locale, 0)}
                />
              )}
              {nic !== undefined && (
                <Fact
                  label={t('mobile.machines.network')}
                  value={`↓${formatThroughput(nic.rx, locale)} ↑${formatThroughput(nic.tx, locale)}`}
                />
              )}
              {temperature !== null && (
                <Fact
                  label={t('mobile.machines.temperature')}
                  value={formatTemperature(temperature, locale)}
                />
              )}
            </dl>
          </div>
        )}
      </button>
      {/* Outside the button: a revoked key needs its own action, and a button
          inside a button is invalid HTML browsers repair by dropping the
          inner one. */}
      {live.state === 'unauthorised' && onPairAgain !== undefined && (
        <div className="border-t border-[var(--color-border-subtle)] p-4">
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('mobile.pairExpiredBody')}</p>
          <Button className="mt-3" variant="primary" onClick={() => onPairAgain(pairing.id)}>
            {t('mobile.pairAgain')}
          </Button>
        </div>
      )}
      {/* Outside the button: an alert row can itself be a tap target, and a
          button inside a button is invalid HTML that browsers repair by
          dropping the inner one. */}
      {live.alerts !== null && (
        <AlertsStrip alerts={live.alerts} onProcesses={() => onOpen(pairing.id)} />
      )}
    </Card>
  );
}

/** `total > 0` is the only signal the phone has that memory was measured at all. */
function memoryReported(system: SystemMetrics): boolean {
  return system.memory.total > 0;
}

/**
 * What is wrong with this PC, or the one line saying nothing is.
 *
 * `title` is an i18n key from the server (`alert.<kind>.title`) with the
 * numbers already rounded in `values`, so the phone and the desktop cannot
 * disagree about a percentage. The severity is spoken as text beside the
 * icon: colour alone is invisible to a screen reader and to a third of
 * colour-blind users on a red/amber pair.
 */
function AlertsStrip({
  alerts,
  onProcesses,
}: {
  readonly alerts: readonly Alert[];
  readonly onProcesses: () => void;
}) {
  const { t } = useTranslation(MOBILE_ALERTS_NS);

  if (alerts.length === 0) {
    return (
      <p className="flex items-center gap-2 border-t border-[var(--color-border-subtle)] px-4 py-2 text-2xs text-[var(--color-fg-muted)]">
        <CheckCircle2 aria-hidden className="size-3.5 shrink-0 text-[var(--color-ok-text)]" />
        <span>{t('alert.none')}</span>
      </p>
    );
  }

  return (
    <ul
      // Polite, and only because this list changes rarely — the meters above
      // update every second and would make a live region unusable.
      aria-live="polite"
      className="border-t border-[var(--color-border-subtle)]"
    >
      {alerts.map((alert) => {
        const Icon = SEVERITY_ICON[alert.severity];
        const body = (
          <>
            <Icon aria-hidden className={cn('size-4 shrink-0', SEVERITY_TONE[alert.severity])} />
            <span className="min-w-0 flex-1 truncate text-left">
              {t(alert.title, alert.values)}
            </span>
            <span className={cn('shrink-0 text-2xs font-medium', SEVERITY_TONE[alert.severity])}>
              {t(`alert.severity.${alert.severity}`)}
            </span>
          </>
        );
        const className = 'flex min-h-[44px] w-full items-center gap-2 px-4 py-2 text-sm';
        return (
          // Keyed on kind *and* subject: two disks can be nearly full at once.
          <li key={`${alert.kind}:${alert.subject}`}>
            {alert.route === 'processes' ? (
              <button
                type="button"
                onClick={onProcesses}
                className={cn(className, 'active:bg-[var(--color-bg-subtle)]')}
              >
                {body}
              </button>
            ) : (
              <div className={className}>{body}</div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

/** Column count matching the facts actually shown, so one fact is not a third-width sliver. */
function facts(...present: readonly boolean[]): string {
  const n = present.filter(Boolean).length;
  return n <= 1 ? 'grid-cols-1' : n === 2 ? 'grid-cols-2' : 'grid-cols-3';
}

function Trace({
  label,
  value,
  history,
  stroke,
}: {
  readonly label: string;
  readonly value: string;
  readonly history: readonly number[];
  readonly stroke: string;
}) {
  return (
    <div className="flex items-center gap-3">
      <div className="w-20 shrink-0">
        <div className="text-2xs text-[var(--color-fg-muted)]">{label}</div>
        <div className="tnum text-sm font-medium">{value}</div>
      </div>
      <Sparkline values={history} stroke={stroke} className="h-7 min-w-0 flex-1" />
    </div>
  );
}

function Fact({ label, value }: { readonly label: string; readonly value: string }) {
  return (
    <div
      className={cn(
        'min-w-0 rounded-[var(--radius-control)] bg-[var(--color-bg-inset)] px-2 py-1.5',
      )}
    >
      <dt className="truncate text-[var(--color-fg-muted)]">{label}</dt>
      <dd className="tnum truncate font-medium">{value}</dd>
    </div>
  );
}
