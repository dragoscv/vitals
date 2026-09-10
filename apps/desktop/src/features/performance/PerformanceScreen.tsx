/**
 * The Performance page.
 *
 * A rail of resources on the left, detail on the right — the shape Task
 * Manager established and got right, because it makes "which of my resources
 * is the problem" answerable by scanning one column.
 *
 * Composition only: the rail's contents come from `resources.ts`, each panel
 * owns its own layout, and history comes from the shared collector the
 * dashboard fills. That collector is a module singleton precisely so this page
 * has charts on arrival rather than starting from an empty buffer, even though
 * the dashboard was unmounted the moment the user navigated here.
 */

import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Checkbox, EmptyState, Meter, ScrollArea, Skeleton, cn, formatPercent } from '@vitals/ui';
import { PlugZap } from 'lucide-react';

import { useHistory } from '../dashboard/useHistory';
import { history as sharedHistory, type HistoryCollector } from '../dashboard/history';
import {
  createTauriSystemSource,
  NO_SAMPLER,
  useSystemSnapshot,
  type SystemSource,
} from '../dashboard/useSystemSnapshot';
import { CpuPanel } from './panels/CpuPanel';
import { DiskPanel, GpuPanel, NetworkPanel } from './panels/DevicePanels';
import { MemoryPanel } from './panels/MemoryPanel';
import { ThermalsPanel } from './panels/ThermalsPanel';
import { buildResourceList, resolveSelection, type ResourceEntry } from './resources';
import { PERFORMANCE_NS } from './strings';

export interface PerformanceScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly source?: SystemSource;
  readonly historyCollector?: HistoryCollector;
}

export function PerformanceScreen({
  source,
  historyCollector = sharedHistory,
}: PerformanceScreenProps = {}): React.JSX.Element {
  const { t, i18n } = useTranslation(PERFORMANCE_NS);
  const locale = i18n.language;

  const [fallbackSource] = useState<SystemSource | null>(() =>
    source === undefined
      ? createTauriSystemSource((frame) => {
          historyCollector.push(frame);
        })
      : null,
  );
  const active = source ?? fallbackSource;
  if (active === null) throw new Error('performance screen has no metrics source');

  const snapshot = useSystemSnapshot(active);
  const history = useHistory(historyCollector);

  const [showVirtual, setShowVirtual] = useState(false);
  // Held as an id, never an index: an index would silently move the user to a
  // different device the moment one above it was unplugged.
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const entries = useMemo(
    () => buildResourceList(snapshot.system, { showVirtualAdapters: showVirtual }),
    [snapshot.system, showVirtual],
  );
  const selected = useMemo(() => resolveSelection(entries, selectedId), [entries, selectedId]);

  if (snapshot.pending) return <PerformanceSkeleton />;

  if (snapshot.system === null) {
    return (
      <EmptyState
        icon={<PlugZap />}
        title={t('noData.title')}
        description={
          snapshot.error === null || snapshot.error === NO_SAMPLER
            ? t('noData.body')
            : snapshot.error
        }
      />
    );
  }

  return (
    <div className="flex flex-col gap-4">
      <header>
        <h2 className="text-lg font-semibold">{t('title')}</h2>
        <p className="text-2xs text-[var(--color-fg-muted)]">{t('subtitle')}</p>
      </header>

      <div className="flex min-h-0 flex-col gap-4 lg:flex-row">
        <nav aria-label={t('rail.label')} className="shrink-0 lg:w-56">
          <ScrollArea className="max-h-[60vh]">
            <ul className="flex gap-1.5 lg:flex-col">
              {entries.map((entry) => (
                <li key={entry.id} className="min-w-40 flex-1 lg:min-w-0">
                  <RailButton
                    entry={entry}
                    selected={entry.id === selected?.id}
                    locale={locale}
                    onSelect={() => {
                      setSelectedId(entry.id);
                    }}
                  />
                </li>
              ))}
            </ul>
          </ScrollArea>

          <label className="mt-2 flex items-center gap-2">
            <Checkbox
              checked={showVirtual}
              onCheckedChange={(next) => {
                setShowVirtual(next === true);
              }}
            />
            <span className="text-2xs text-[var(--color-fg-muted)]">{t('rail.showVirtual')}</span>
          </label>
        </nav>

        <section className="min-w-0 flex-1" aria-live="off">
          {selected === null ? null : (
            <Detail entry={selected} snapshot={snapshot} history={history} locale={locale} />
          )}
        </section>
      </div>
    </div>
  );
}

function RailButton({
  entry,
  selected,
  locale,
  onSelect,
}: {
  readonly entry: ResourceEntry;
  readonly selected: boolean;
  readonly locale: string;
  readonly onSelect: () => void;
}) {
  const { t } = useTranslation(PERFORMANCE_NS);
  const label = entry.name === '' ? t(`${entry.kind}.title`) : entry.name;

  return (
    <button
      type="button"
      onClick={onSelect}
      aria-current={selected ? 'true' : undefined}
      className={cn(
        'w-full rounded-md border px-2.5 py-2 text-left',
        selected
          ? 'border-[var(--color-accent-border)] bg-[var(--color-accent-subtle)]'
          : 'border-transparent hover:bg-[var(--color-bg-subtle)]',
      )}
    >
      <span className="flex items-baseline justify-between gap-2">
        <span className="truncate text-sm font-medium">{label}</span>
        {/* No reading, no number. Networks have no honest percentage and a
            GPU without counters is unmeasured — see `resources.ts`. */}
        {entry.utilization !== null && (
          <span className="tnum shrink-0 font-mono text-sm text-[var(--color-fg-muted)]">
            {entry.kind === 'thermals'
              ? `${Math.round(entry.utilization)}°`
              : formatPercent(entry.utilization, locale, 0)}
          </span>
        )}
      </span>
      {entry.detail !== null && (
        <span className="block truncate text-2xs text-[var(--color-fg-subtle)]">
          {entry.detail}
        </span>
      )}
      {entry.utilization !== null && entry.kind !== 'thermals' && (
        <Meter
          className="mt-1"
          label=""
          accessibleLabel={label}
          value={entry.utilization}
          valueText={formatPercent(entry.utilization, locale, 0)}
        />
      )}
    </button>
  );
}

function Detail({
  entry,
  snapshot,
  history,
  locale,
}: {
  readonly entry: ResourceEntry;
  readonly snapshot: ReturnType<typeof useSystemSnapshot>;
  readonly history: ReturnType<typeof useHistory>;
  readonly locale: string;
}): React.JSX.Element | null {
  const system = snapshot.system;
  if (system === null) return null;

  switch (entry.kind) {
    case 'cpu':
      return <CpuPanel system={system} history={history} locale={locale} />;
    case 'memory':
      return <MemoryPanel system={system} history={history} locale={locale} />;
    case 'thermals':
      return <ThermalsPanel system={system} locale={locale} />;
    case 'gpu': {
      const gpu = system.gpus.find((candidate) => candidate.id === entry.deviceId);
      // A device can disappear between the rail being built and this render —
      // unplugging an eGPU does exactly that. Rendering nothing for one frame
      // is correct; the rail rebuilds on the next.
      return gpu === undefined ? null : <GpuPanel gpu={gpu} history={history} locale={locale} />;
    }
    case 'disk': {
      const disk = system.disks.find((candidate) => candidate.id === entry.deviceId);
      return disk === undefined ? null : (
        <DiskPanel disk={disk} history={history} locale={locale} />
      );
    }
    case 'network': {
      const nic = system.networks.find((candidate) => candidate.id === entry.deviceId);
      return nic === undefined ? null : (
        <NetworkPanel nic={nic} history={history} locale={locale} />
      );
    }
    default: {
      const exhaustive: never = entry.kind;
      return exhaustive;
    }
  }
}

function PerformanceSkeleton(): React.JSX.Element {
  return (
    <div aria-busy="true" className="flex flex-col gap-4 lg:flex-row">
      <div className="flex shrink-0 flex-col gap-1.5 lg:w-56">
        {[0, 1, 2, 3].map((index) => (
          <Skeleton key={index} className="h-14" />
        ))}
      </div>
      <div className="flex-1 space-y-3">
        <Skeleton className="h-40" />
        <Skeleton className="h-24" />
      </div>
    </div>
  );
}
