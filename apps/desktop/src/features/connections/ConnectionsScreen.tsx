/**
 * Network connections.
 *
 * Composition only — grouping and filtering live in `model.ts`, polling in
 * `useConnections.ts`, so both are exercised without a DOM.
 *
 * The screen's one editorial decision: it groups by application and shows
 * counts, rather than presenting the flat socket table. `netstat -ano` output
 * is a thousand rows that answer nothing, because the question is never "what
 * is this socket" but "what is this program talking to".
 */

import { ChevronDown, ChevronRight, Globe, RefreshCw, Wifi } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Process } from '@vitals/protocol';
import { Badge, Button, EmptyState, SearchInput, SegmentedControl, Skeleton, cn } from '@vitals/ui';

import { ExportButton } from '../../components/ExportButton';
import type { ExportColumn } from '../../lib/export';
import { oneOf, useUrlState } from '../../lib/useUrlState';
import {
  useSystemSnapshot,
  createTauriSystemSource,
  type SystemSource,
} from '../dashboard/useSystemSnapshot';
import {
  applySearch,
  connectionFilters,
  connectionId,
  groupByApp,
  isExternal,
  isPublicListener,
  type ConnectionFilter,
  type ConnectionGroup,
  type ConnectionRow,
} from './model';
import { CONNECTIONS_NS } from './strings';
import { NO_HOST, useConnections, type ConnectionsReader } from './useConnections';

export interface ConnectionsScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly reader?: ConnectionsReader;
  /** Supplies process names; the socket table only carries PIDs. */
  readonly processSource?: SystemSource;
}

export function ConnectionsScreen({
  reader,
  processSource,
}: ConnectionsScreenProps = {}): React.JSX.Element {
  const { t } = useTranslation(CONNECTIONS_NS);

  const state = useConnections(reader);

  const [defaultSource] = useState<SystemSource | null>(() =>
    processSource === undefined ? createTauriSystemSource() : null,
  );
  const processes = useSystemSnapshot(processSource ?? defaultSource ?? emptySource());

  const [view, patchView] = useUrlState<{ q: string; filter: ConnectionFilter }>(
    'network',
    { q: '', filter: 'all' },
    { filter: oneOf(connectionFilters) },
  );
  const query = view.q;
  const filter = view.filter;
  const setQuery = (q: string): void => {
    patchView({ q });
  };
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());

  // PID to name, built once per frame rather than searched per row: the socket
  // table can hold thousands of entries and a linear scan of the process map
  // for each would be quadratic on exactly the machines that have most rows.
  const namesByPid = useMemo(() => {
    const map = new Map<number, string>();
    for (const process of processes.processes.values() as Iterable<Process>) {
      map.set(process.key.pid, process.name);
    }
    return map;
  }, [processes.processes]);

  const groups = useMemo(() => {
    const rows: ConnectionRow[] = (state.snapshot?.connections ?? []).map((connection) => ({
      ...connection,
      id: connectionId(connection),
    }));

    return groupByApp(rows, {
      nameFor: (pid) => namesByPid.get(pid) ?? null,
      unknownLabel: t('unknownApp'),
    });
  }, [state.snapshot, namesByPid, t]);

  const visible = useMemo(() => applySearch(groups, filter, query), [groups, filter, query]);

  // The export is the flat socket list, one row per socket with its owning
  // application in the first column. The grouping is a reading aid; a CSV
  // consumer wants a table, and the application name is what makes each row
  // meaningful on its own.
  type SocketExportRow = ConnectionRow & { readonly app: string };
  const exportRows = useMemo(
    (): readonly SocketExportRow[] =>
      visible.flatMap((group) => group.rows.map((row) => ({ ...row, app: group.name }))),
    [visible],
  );
  const exportColumns = useMemo(
    (): readonly ExportColumn<SocketExportRow>[] => [
      { id: 'app', header: t('column.app'), value: (row) => row.app },
      { id: 'protocol', header: t('column.protocol'), value: (row) => row.protocol },
      { id: 'localAddress', header: t('column.local'), value: (row) => row.localAddress },
      { id: 'localPort', header: t('column.localPort'), value: (row) => row.localPort },
      { id: 'remoteAddress', header: t('column.remote'), value: (row) => row.remoteAddress },
      { id: 'remotePort', header: t('column.remotePort'), value: (row) => row.remotePort },
      { id: 'state', header: t('column.state'), value: (row) => row.state },
      { id: 'ownerPid', header: t('column.pid'), value: (row) => row.ownerPid },
    ],
    [t],
  );

  if (state.pending) return <ConnectionsSkeleton />;

  if (state.error === NO_HOST) {
    return <EmptyState icon={<Wifi />} title={t('noHost.title')} description={t('noHost.body')} />;
  }

  return (
    <div className="screen">
      <header className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{t('title')}</h2>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('subtitle')}</p>
        </div>
        <Button variant="ghost" size="sm" onClick={state.refresh}>
          <RefreshCw aria-hidden className="size-4" />
          {t('refresh')}
        </Button>
      </header>

      {/* A failure after the first success keeps the list and says it is stale.
          Blanking a list the user is reading is worse than showing old data
          that is labelled as old. */}
      {state.error !== null && state.error !== NO_HOST && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {t('stale', { message: state.error })}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <SearchInput
          value={query}
          onValueChange={setQuery}
          placeholder={t('search')}
          aria-label={t('search')}
          clearLabel={t('clear')}
          className="min-w-56 flex-1"
        />
        <SegmentedControl
          value={filter}
          ariaLabel={t('filterLabel')}
          onValueChange={(next) => {
            patchView({ filter: next });
          }}
          options={connectionFilters.map((id) => ({ value: id, label: t(`filter.${id}`) }))}
        />
        <ExportButton name="connections" rows={exportRows} columns={exportColumns} />
      </div>

      {visible.length === 0 ? (
        <EmptyState title={t('empty.title')} description={t('empty.body')} />
      ) : (
        <div className="screen-scroll">
          <ul className="flex flex-col gap-1.5">
            {visible.map((group) => (
              <GroupRow
                key={group.name}
                group={group}
                expanded={expanded.has(group.name)}
                onToggle={() => {
                  setExpanded((current) => {
                    const next = new Set(current);
                    if (next.has(group.name)) next.delete(group.name);
                    else next.add(group.name);
                    return next;
                  });
                }}
              />
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function GroupRow({
  group,
  expanded,
  onToggle,
}: {
  readonly group: ConnectionGroup;
  readonly expanded: boolean;
  readonly onToggle: () => void;
}) {
  const { t } = useTranslation(CONNECTIONS_NS);
  const Chevron = expanded ? ChevronDown : ChevronRight;

  return (
    <li className="rounded-md border border-[var(--color-border-subtle)]">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={expanded}
        className="flex w-full items-center gap-2 px-2.5 py-2 text-left hover:bg-[var(--color-bg-subtle)]"
      >
        <Chevron aria-hidden className="size-4 shrink-0 text-[var(--color-fg-muted)]" />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-sm font-medium">{group.name}</span>
          <span className="flex flex-wrap gap-x-3 text-2xs text-[var(--color-fg-muted)]">
            <span>{t('summary.connections', { count: group.rows.length })}</span>
            {group.established > 0 && (
              <span>{t('summary.established', { count: group.established })}</span>
            )}
            {group.listening > 0 && (
              <span>{t('summary.listening', { count: group.listening })}</span>
            )}
            {group.remoteHosts > 0 && (
              <span>{t('summary.hosts', { count: group.remoteHosts })}</span>
            )}
          </span>
        </span>
        {/* An observation, not an accusation. This module has no threat
            intelligence, and a false positive on a system process teaches the
            user to ignore the badge — or worse, to kill something
            load-bearing. So it states a fact and lets them judge.

            `title` rather than the `Tooltip` component: Tooltip requires a
            `TooltipProvider` ancestor, which only `AppShell` supplies. A
            screen that throws unless it is mounted inside one particular
            parent is a screen that crashes the moment it is rendered anywhere
            else — which is exactly what happened here, for every user with a
            public listener. The native attribute has no such dependency. */}
        {group.publicListeners > 0 && (
          <Badge
            tone="info"
            title={t('summary.publicListenerHint')}
            icon={<Globe aria-hidden className="size-3" />}
          >
            {t('summary.publicListener')}
          </Badge>
        )}
      </button>

      {expanded && <SocketTable rows={group.rows} />}
    </li>
  );
}

function SocketTable({ rows }: { readonly rows: readonly ConnectionRow[] }) {
  const { t } = useTranslation(CONNECTIONS_NS);

  return (
    <div className="overflow-x-auto border-t border-[var(--color-border-subtle)]">
      <table className="w-full text-left">
        <thead>
          <tr className="text-2xs text-[var(--color-fg-muted)]">
            <th scope="col" className="px-2.5 py-1 font-normal">
              {t('column.protocol')}
            </th>
            <th scope="col" className="px-2.5 py-1 font-normal">
              {t('column.local')}
            </th>
            <th scope="col" className="px-2.5 py-1 font-normal">
              {t('column.remote')}
            </th>
            <th scope="col" className="px-2.5 py-1 font-normal">
              {t('column.state')}
            </th>
            <th scope="col" className="px-2.5 py-1 font-normal">
              {t('column.pid')}
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.id} className="border-t border-[var(--color-border-subtle)]">
              <td className="px-2.5 py-1 font-mono text-2xs uppercase">{row.protocol}</td>
              <td className="px-2.5 py-1 font-mono text-2xs">
                {row.localAddress}:{row.localPort}
                {isPublicListener(row) && (
                  <Globe
                    aria-hidden
                    className="ml-1 inline size-3 text-[var(--color-status-info)]"
                  />
                )}
              </td>
              <td
                className={cn(
                  'px-2.5 py-1 font-mono text-2xs',
                  isExternal(row.remoteAddress) && 'text-[var(--color-fg-default)]',
                )}
              >
                {row.remoteAddress === null ? '—' : `${row.remoteAddress}:${row.remotePort ?? ''}`}
              </td>
              <td className="px-2.5 py-1 text-2xs">{t(`state.${row.state}`)}</td>
              <td className="tnum px-2.5 py-1 font-mono text-2xs">{row.ownerPid ?? '—'}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ConnectionsSkeleton(): React.JSX.Element {
  return (
    <div aria-busy="true" className="space-y-2">
      <Skeleton className="h-7 w-64" />
      {[0, 1, 2, 3, 4].map((index) => (
        <Skeleton key={index} className="h-12" />
      ))}
    </div>
  );
}

/** Used when no process source is available; yields no names, never throws. */
function emptySource(): SystemSource {
  return {
    subscribe: () => () => undefined,
    current: () => ({
      system: null,
      processes: new Map(),
      seq: 0,
      timestampMs: 0,
      error: null,
      pending: false,
    }),
  };
}
