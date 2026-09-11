/**
 * The History widget: CPU, memory and GPU over the last hour to week, read
 * from the on-disk store rather than the live feed.
 *
 * Every other chart on the dashboard shows the last three minutes. This one
 * answers the other question a slow computer raises — "was it like this
 * yesterday too?" — which the live buffers cannot, because they are gone the
 * moment the window closes.
 */

import { Database, DatabaseZap, Settings } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { TimeSeriesChart, type Series } from '@vitals/charts';
import { Button, EmptyState, SegmentedControl, Skeleton } from '@vitals/ui';

import { DASHBOARD_NS } from '../strings';
import {
  historyRanges,
  useMachineHistory,
  type HistoryRange,
  type HistorySource,
} from './useMachineHistory';
import { useThemeColors } from './useThemeColors';

export interface HistoryWidgetProps {
  /** The `historyEnabled` setting. Off means the store is not being written. */
  readonly recording: boolean;
  /** Opens the Settings dialog, where recording is switched on. */
  readonly onOpenSettings: () => void;
  /** Injectable so tests need no Tauri host. */
  readonly source?: HistorySource;
}

export function HistoryWidget({
  recording,
  onOpenSettings,
  source,
}: HistoryWidgetProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const colors = useThemeColors();
  const [range, setRange] = useState<HistoryRange>('h24');
  const history = useMachineHistory(range, recording, source);

  if (!recording) {
    return (
      <EmptyState
        icon={<Database />}
        title={t('history.off.title')}
        description={t('history.off.body')}
        action={
          <Button variant="secondary" size="sm" onClick={onOpenSettings}>
            <Settings aria-hidden className="size-4" />
            {t('history.off.action')}
          </Button>
        }
      />
    );
  }

  const series: readonly Series[] = [
    { buffer: history.series.cpu, color: colors.accent, fillOpacity: 0.12 },
    { buffer: history.series.memory, color: colors.warning, lineWidth: 1 },
    { buffer: history.series.gpu, color: colors.danger, lineWidth: 1 },
  ];

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <SegmentedControl
          size="sm"
          value={range}
          ariaLabel={t('history.rangeLabel')}
          onValueChange={setRange}
          options={historyRanges.map((id) => ({ value: id, label: t(`history.range.${id}`) }))}
        />
        <Legend
          items={[
            { key: 'cpu', label: t('history.cpu'), color: colors.accent },
            { key: 'memory', label: t('history.memory'), color: colors.warning },
            { key: 'gpu', label: t('history.gpu'), color: colors.danger },
          ]}
        />
      </div>

      {history.error !== null && (
        <p role="alert" className="text-2xs text-[var(--color-danger-fg)]">
          {t('history.failed', { reason: history.error })}
        </p>
      )}

      {history.pending && history.series.count === 0 ? (
        <Skeleton className="h-32 w-full" />
      ) : history.series.count === 0 ? (
        history.error === null && (
          <EmptyState
            icon={<DatabaseZap />}
            title={t('history.empty.title')}
            description={t('history.empty.body')}
          />
        )
      ) : (
        <TimeSeriesChart
          series={series}
          revision={history.revision}
          scale={{ min: 0, max: 100 }}
          className="h-32 w-full"
          ariaLabel={t('widget.history.title')}
        />
      )}
    </div>
  );
}

function Legend({
  items,
}: {
  readonly items: readonly { key: string; label: string; color: string }[];
}): React.JSX.Element {
  return (
    <ul className="flex items-center gap-3 text-2xs text-[var(--color-fg-muted)]">
      {items.map((item) => (
        <li key={item.key} className="flex items-center gap-1">
          <span
            aria-hidden
            className="inline-block size-2 rounded-full"
            style={{ backgroundColor: item.color }}
          />
          {item.label}
        </li>
      ))}
    </ul>
  );
}
