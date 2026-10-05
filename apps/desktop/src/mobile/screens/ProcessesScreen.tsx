import { useVirtualizer } from '@tanstack/react-virtual';
import { useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { ControlRequest } from '@vitals/client';
import { displayName, matchesProcessName, type Process } from '@vitals/protocol';
import {
  EmptyState,
  SearchInput,
  SegmentedControl,
  Skeleton,
  formatBytes,
  formatPercent,
} from '@vitals/ui';

import { ActionSheet, type ActionOutcome } from '../components/ActionSheet';
import type { LiveState } from '../lib/live';
import type { Pairing } from '../lib/pairing';

export interface ProcessesScreenProps {
  readonly pairing: Pairing | undefined;
  readonly live: LiveState;
  readonly control: (request: ControlRequest) => Promise<void>;
  readonly onReadOnlyLearned: (id: string) => void;
}

type SortKey = 'cpu' | 'memory';

/** Tall enough for a 44 px touch target with breathing room. */
const ROW_HEIGHT = 56;

/** The process list for one PC, sorted, searchable and virtualised. */
export function ProcessesScreen({
  pairing,
  live,
  control,
  onReadOnlyLearned,
}: ProcessesScreenProps) {
  const { t, i18n } = useTranslation();
  const locale = i18n.language;
  const [sort, setSort] = useState<SortKey>('cpu');
  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<Process | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  const rows = useMemo(() => {
    const all = live.snapshot === null ? [] : Array.from(live.snapshot.processes.values());
    const needle = query.trim().toLowerCase();
    const filtered = needle === '' ? all : all.filter((p) => matchesProcessName(p, needle));
    return filtered.sort((a, b) =>
      sort === 'cpu' ? b.cpu - a.cpu : b.memoryPrivate - a.memoryPrivate,
    );
  }, [live.snapshot, query, sort]);

  // Same reasoning as the desktop table: the virtualiser's own output is a
  // handful of numbers per scroll frame and nothing memoised depends on it.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
    initialRect: { width: 360, height: 640 },
    // A 0 height is "not laid out yet", never "no room" — happy-dom reports 0
    // always, and a phone briefly does during rotation.
    observeElementRect: (instance, cb) => {
      const element = instance.scrollElement;
      if (element === null || element === undefined) return undefined;
      const measure = (): void => {
        const rect = element.getBoundingClientRect();
        cb({
          width: rect.width > 0 ? rect.width : 360,
          height: rect.height > 0 ? rect.height : 640,
        });
      };
      measure();
      const observer = new ResizeObserver(measure);
      observer.observe(element);
      return () => observer.disconnect();
    },
    getItemKey: (index) => {
      const p = rows[index];
      return p === undefined ? index : `${p.key.pid}:${p.key.startTime}`;
    },
  });

  if (pairing === undefined) {
    return (
      <EmptyState
        className="flex-1"
        title={t('mobile.processes.pickMachine')}
        description={t('mobile.processes.pickMachineBody')}
      />
    );
  }

  const loading = live.snapshot === null;

  function settled(outcome: ActionOutcome): void {
    if (outcome.readOnly && pairing !== undefined) onReadOnlyLearned(pairing.id);
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="space-y-2 px-4 pt-3 pb-2">
        <div className="flex items-center justify-between gap-2">
          <h1 className="truncate text-lg font-semibold">{pairing.name}</h1>
          <SegmentedControl<SortKey>
            value={sort}
            onValueChange={setSort}
            ariaLabel={t('mobile.processes.sortBy')}
            options={[
              { value: 'cpu', label: t('mobile.processes.sortCpu') },
              { value: 'memory', label: t('mobile.processes.sortMemory') },
            ]}
          />
        </div>
        <SearchInput
          value={query}
          onValueChange={setQuery}
          ariaLabel={t('mobile.processes.search')}
          placeholder={t('mobile.processes.search')}
          clearLabel={t('mobile.action.cancel')}
          inputMode="search"
          resultsAnnouncement={t('mobile.processes.count', { count: rows.length })}
        />
      </div>

      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto" aria-busy={loading}>
        {loading ? (
          <ul className="space-y-px px-4">
            {Array.from({ length: 10 }, (_, i) => (
              <li key={i} className="flex items-center gap-3 py-3">
                <Skeleton className="h-4 flex-1" />
                <Skeleton className="h-4 w-12" />
                <Skeleton className="h-4 w-16" />
              </li>
            ))}
          </ul>
        ) : rows.length === 0 ? (
          <EmptyState
            title={t('mobile.processes.none')}
            description={t('mobile.processes.noneBody')}
          />
        ) : (
          <ul
            className="relative w-full"
            style={{ height: virtualizer.getTotalSize() }}
            aria-label={t('mobile.processes.title')}
          >
            {virtualizer.getVirtualItems().map((item) => {
              const p = rows[item.index];
              if (p === undefined) return null;
              return (
                <li
                  key={item.key}
                  className="absolute inset-x-0 top-0"
                  style={{ height: item.size, transform: `translateY(${item.start}px)` }}
                >
                  <button
                    type="button"
                    onClick={() => setSelected(p)}
                    className="flex h-full w-full items-center gap-3 border-b border-[var(--color-border-subtle)] px-4 text-left active:bg-[var(--color-bg-subtle)]"
                  >
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium">{displayName(p)}</span>
                      <span className="tnum block text-2xs text-[var(--color-fg-muted)]">
                        PID {p.key.pid}
                      </span>
                    </span>
                    <span className="tnum w-14 shrink-0 text-right text-sm">
                      {formatPercent(p.cpu, locale, 0)}
                    </span>
                    <span className="tnum w-20 shrink-0 text-right text-sm text-[var(--color-fg-muted)]">
                      {formatBytes(p.memoryPrivate, locale)}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>

      {selected !== null && (
        <ActionSheet
          process={selected}
          control={control}
          readOnly={pairing.readOnly}
          onClose={() => setSelected(null)}
          onSettled={settled}
        />
      )}
    </div>
  );
}
