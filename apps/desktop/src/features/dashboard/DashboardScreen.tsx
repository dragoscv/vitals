/**
 * The dashboard.
 *
 * Composition only. The layout model is in `widgets.ts`, persistence in
 * `useLayout.ts`, the alert rules in `alerts.ts`, and history collection in
 * `history.ts` — each of which is exercised without a DOM, which is the reason
 * this file is wiring and a switch statement.
 *
 * # One subscription, not one per widget
 *
 * Every widget reads from a single snapshot passed down as a prop. The
 * tempting alternative — each widget subscribing to the sampler itself — gives
 * twelve independent re-render roots on a 1 Hz feed and twelve copies of the
 * ~600-entry process map per second. The saving from finer-grained updates is
 * imaginary here because all twelve widgets change on the same tick anyway.
 */

import { LayoutGrid, PlugZap, Plus, RotateCcw, Settings2 } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
  EmptyState,
  Skeleton,
} from '@vitals/ui';

import type { RouteId } from '../../shell/navigation';
import { evaluateAlerts } from './alerts';
import { history as sharedHistory, type HistoryCollector } from './history';
import { DASHBOARD_NS } from './strings';
import { useHistory } from './useHistory';
import { useLayout, type LayoutBackend } from './useLayout';
import {
  createTauriSystemSource,
  NO_SAMPLER,
  useSystemSnapshot,
  type SystemSource,
} from './useSystemSnapshot';
import { AlertsWidget } from './widgets/AlertsWidget';
import {
  BatteryWidget,
  GpuWidget,
  StorageWidget,
  SystemWidget,
  ThermalsWidget,
  TopCpuWidget,
  TopMemoryWidget,
} from './widgets/DetailWidgets';
import {
  CpuWidget,
  DiskWidget,
  MemoryWidget,
  NetworkWidget,
  type WidgetBodyProps,
} from './widgets/ResourceWidgets';
import { WidgetFrame } from './WidgetFrame';
import {
  availableToAdd,
  capabilitiesFrom,
  visibleLayout,
  widgetById,
  type WidgetId,
} from './widgets';

export interface DashboardScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly source?: SystemSource;
  readonly layoutBackend?: LayoutBackend;
  readonly historyCollector?: HistoryCollector;
  /**
   * `| undefined` explicitly, because `exactOptionalPropertyTypes` is on: a
   * caller forwarding an optional handler passes `undefined` as a real value,
   * and without this the compiler rejects the forward rather than the bug.
   */
  readonly onNavigate?: ((route: RouteId) => void) | undefined;
}

export function DashboardScreen({
  source,
  layoutBackend,
  historyCollector = sharedHistory,
  onNavigate,
}: DashboardScreenProps = {}): React.JSX.Element {
  const { t, i18n } = useTranslation(DASHBOARD_NS);
  const locale = i18n.language;

  // Created once. A ref written during render would be a correctness hazard
  // the compiler rightly rejects; a lazy state initialiser is the supported
  // way to hold a non-reactive singleton.
  const [fallbackSource] = useState<SystemSource | null>(() =>
    source === undefined
      ? createTauriSystemSource((frame) => {
          historyCollector.push(frame);
        })
      : null,
  );
  const active = source ?? fallbackSource;
  if (active === null) throw new Error('dashboard has no metrics source');

  const snapshot = useSystemSnapshot(active);
  const historyState = useHistory(historyCollector);
  const layoutController = useLayout(layoutBackend);
  const [editing, setEditing] = useState(false);

  const capabilities = useMemo(() => capabilitiesFrom(snapshot.system), [snapshot.system]);
  const placements = useMemo(
    () => visibleLayout(layoutController.layout, capabilities),
    [layoutController.layout, capabilities],
  );
  const addable = useMemo(
    () => availableToAdd(layoutController.layout, capabilities),
    [layoutController.layout, capabilities],
  );

  const alerts = useMemo(
    () => evaluateAlerts({ system: snapshot.system, history: historyState }),
    [snapshot.system, historyState],
  );

  const navigate = (route: RouteId): void => {
    onNavigate?.(route);
  };

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-wrap items-center justify-between gap-2">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{t('title')}</h2>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('subtitle')}</p>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {editing && (
            <>
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="ghost" size="sm" disabled={addable.length === 0}>
                    <Plus aria-hidden className="size-4" />
                    {t('layout.add')}
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuLabel>{t('layout.addTitle')}</DropdownMenuLabel>
                  <DropdownMenuSeparator />
                  {addable.map((widget) => (
                    <DropdownMenuItem
                      key={widget.id}
                      onSelect={() => {
                        layoutController.add(widget.id);
                      }}
                    >
                      {t(widget.titleKey)}
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
              <Button variant="ghost" size="sm" onClick={layoutController.reset}>
                <RotateCcw aria-hidden className="size-4" />
                {t('layout.reset')}
              </Button>
            </>
          )}
          <Button
            variant={editing ? 'primary' : 'ghost'}
            size="sm"
            onClick={() => {
              setEditing((value) => !value);
            }}
          >
            {editing ? (
              <Settings2 aria-hidden className="size-4" />
            ) : (
              <LayoutGrid aria-hidden className="size-4" />
            )}
            {editing ? t('layout.done') : t('layout.edit')}
          </Button>
        </div>
      </header>

      {!layoutController.hydrated || snapshot.pending ? (
        <DashboardSkeleton />
      ) : snapshot.system === null ? (
        /*
         * No frame ever arrived. Distinct from the skeleton state, which is
         * bounded by a timeout in the source precisely so this branch can be
         * reached — a loading state that cannot resolve makes a broken app
         * look busy, which is how the splash-screen hang went unnoticed.
         */
        <EmptyState
          icon={<PlugZap />}
          title={t('noData.title')}
          // Falls back to the generic explanation rather than omitting the
          // description: a title alone leaves the user with nothing to act on.
          description={
            snapshot.error === null || snapshot.error === NO_SAMPLER
              ? t('noData.noSampler')
              : snapshot.error
          }
        />
      ) : placements.length === 0 ? (
        <EmptyState
          icon={<LayoutGrid />}
          title={t('layout.empty')}
          description={t('layout.emptyBody')}
          action={
            <Button
              onClick={() => {
                setEditing(true);
              }}
            >
              {t('layout.add')}
            </Button>
          }
        />
      ) : (
        <div
          // Column count comes from the viewport, never from saved state, so a
          // layout stored on an ultrawide is still correct on a laptop. See
          // the note in `widgets.ts` on why placements are a list, not a grid.
          className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3"
        >
          {placements.map((placement, index) => {
            const definition = widgetById.get(placement.id);
            if (definition === undefined) return null;

            return (
              <WidgetFrame
                key={placement.id}
                definition={definition}
                size={placement.size}
                editing={editing}
                canMoveUp={index > 0}
                canMoveDown={index < placements.length - 1}
                onMove={(direction) => {
                  layoutController.move(placement.id, direction);
                }}
                onResize={(size) => {
                  layoutController.resize(placement.id, size);
                }}
                onRemove={() => {
                  layoutController.remove(placement.id);
                }}
              >
                <WidgetBody
                  id={placement.id}
                  snapshot={snapshot}
                  history={historyState}
                  locale={locale}
                  alerts={alerts}
                  onNavigate={navigate}
                />
              </WidgetFrame>
            );
          })}
        </div>
      )}
    </div>
  );
}

/**
 * Dispatches a widget id to its body.
 *
 * A switch rather than a lookup table so that adding a `WidgetId` without a
 * body is a compile error at the `never` case, not a blank card discovered by
 * a user.
 */
function WidgetBody({
  id,
  snapshot,
  history,
  locale,
  alerts,
  onNavigate,
}: {
  readonly id: WidgetId;
  readonly snapshot: ReturnType<typeof useSystemSnapshot>;
  readonly history: WidgetBodyProps['history'];
  readonly locale: string;
  readonly alerts: ReturnType<typeof evaluateAlerts>;
  readonly onNavigate: (route: RouteId) => void;
}): React.JSX.Element | null {
  const system = snapshot.system;

  if (id === 'alerts') return <AlertsWidget alerts={alerts} onNavigate={onNavigate} />;
  // Every other widget needs a frame. Rendering them against `null` would mean
  // twelve independent null checks, each of which is a chance to display a
  // zero that was never measured.
  if (system === null) return null;

  const body: WidgetBodyProps = { system, history, locale };
  const detail = { system, locale, timestampMs: snapshot.timestampMs };
  const top = {
    processes: snapshot.processes,
    locale,
    onSelect: () => {
      onNavigate('processes');
    },
  };

  switch (id) {
    case 'cpu':
      return <CpuWidget {...body} />;
    case 'memory':
      return <MemoryWidget {...body} />;
    case 'disk':
      return <DiskWidget {...body} />;
    case 'network':
      return <NetworkWidget {...body} />;
    case 'gpu':
      return <GpuWidget {...detail} />;
    case 'thermals':
      return <ThermalsWidget {...detail} />;
    case 'battery':
      return <BatteryWidget {...detail} />;
    case 'topCpu':
      return <TopCpuWidget {...top} />;
    case 'topMemory':
      return <TopMemoryWidget {...top} />;
    case 'uptime':
      return <SystemWidget {...detail} />;
    case 'storage':
      return <StorageWidget {...detail} />;
    default: {
      const exhaustive: never = id;
      return exhaustive;
    }
  }
}

/**
 * Shown until both the layout and the first frame have arrived.
 *
 * Card-shaped, not a spinner: these reserve the space the widgets will occupy
 * so nothing jumps when data lands. On a screen that updates every second,
 * layout shift is the difference between readable and nauseating.
 */
function DashboardSkeleton(): React.JSX.Element {
  return (
    <div aria-busy="true" className="grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {[0, 1, 2, 3, 4, 5].map((index) => (
        <Skeleton key={index} className="h-52" />
      ))}
    </div>
  );
}
