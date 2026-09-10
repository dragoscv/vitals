import { useTranslation } from 'react-i18next';

import {
  Badge,
  Card,
  Skeleton,
  cn,
  formatBytes,
  formatPercent,
  formatTemperature,
  formatThroughput,
} from '@vitals/ui';

import type { ConnectionState, LiveState } from '../lib/live';
import type { Pairing } from '../lib/pairing';
import { Sparkline } from './Sparkline';

export interface MachineCardProps {
  readonly pairing: Pairing;
  readonly live: LiveState;
  readonly onOpen: (id: string) => void;
}

const TONE: Record<ConnectionState, 'ok' | 'warn' | 'danger' | 'neutral'> = {
  live: 'ok',
  connecting: 'neutral',
  reconnecting: 'warn',
  unreachable: 'danger',
};

/** One paired PC at a glance. The whole card is the tap target. */
export function MachineCard({ pairing, live, onOpen }: MachineCardProps) {
  const { t, i18n } = useTranslation();
  const locale = i18n.language;
  const system = live.snapshot?.system;

  const gpu = system?.gpus[0];
  const busiestNic = system?.networks.reduce<(typeof system.networks)[number] | undefined>(
    (best, nic) => (best === undefined || nic.rx + nic.tx > best.rx + best.tx ? nic : best),
    undefined,
  );
  const temperature = system?.cpu.temperature ?? gpu?.temperature ?? null;

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

        {system === undefined ? (
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
                used: formatBytes(system.memory.used, locale),
                total: formatBytes(system.memory.total, locale),
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
                facts(gpu !== undefined, busiestNic !== undefined, temperature !== null),
              )}
            >
              {gpu !== undefined && (
                <Fact
                  label={t('mobile.machines.gpu')}
                  value={formatPercent(gpu.utilization, locale, 0)}
                />
              )}
              {busiestNic !== undefined && (
                <Fact
                  label={t('mobile.machines.network')}
                  value={`↓${formatThroughput(busiestNic.rx, locale)} ↑${formatThroughput(busiestNic.tx, locale)}`}
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
    </Card>
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
