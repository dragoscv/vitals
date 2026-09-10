/**
 * Startup apps and Services.
 *
 * One component behind two routes, because they share a fetch: a service set
 * to start automatically IS a startup item, and the Rust inventory already
 * resolves that rather than making each screen re-derive it. Splitting them
 * would mean reading the SCM twice.
 *
 * # The undercount is stated, not hidden
 *
 * Unelevated, some scheduled task definitions are ACL'd to SYSTEM and some
 * service configurations cannot be queried. Every other tool either omits
 * those silently or guesses. This screen prints how many it could not read and
 * says the totals are a floor — because a startup list that quietly omits
 * three entries is worse than one that admits it, especially when the user is
 * counting them to decide what to disable.
 */

import { RefreshCw, ShieldAlert } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Badge, Button, EmptyState, SearchInput, SegmentedControl, Skeleton, cn } from '@vitals/ui';

import {
  countServices,
  countStartup,
  filterServices,
  filterStartup,
  isMachineWide,
  labelFor,
  serviceFilters,
  sortServices,
  sortStartup,
  startupFilters,
  type ServiceEntry,
  type ServiceFilter,
  type StartupEntry,
  type StartupFilter,
} from './model';
import { STARTUP_NS } from './strings';
import { NO_HOST, useStartup, type StartupReader } from './useStartup';

export interface StartupScreenProps {
  /** `services` asks the backend for start types, which costs an SCM call each. */
  readonly mode: 'startup' | 'services';
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly reader?: StartupReader;
}

export function StartupScreen({ mode, reader }: StartupScreenProps): React.JSX.Element {
  const { t } = useTranslation(STARTUP_NS);
  const state = useStartup(mode === 'services', reader);

  const [query, setQuery] = useState('');
  const [startupFilter, setStartupFilter] = useState<StartupFilter>('all');
  const [serviceFilter, setServiceFilter] = useState<ServiceFilter>('all');

  const snapshot = state.snapshot;

  const startupRows = useMemo(
    () => sortStartup(filterStartup(snapshot?.entries ?? [], startupFilter, query)),
    [snapshot, startupFilter, query],
  );
  const serviceRows = useMemo(
    () => sortServices(filterServices(snapshot?.services ?? [], serviceFilter, query)),
    [snapshot, serviceFilter, query],
  );

  if (state.pending) return <StartupSkeleton />;

  if (state.error === NO_HOST) {
    return (
      <EmptyState icon={<ShieldAlert />} title={t('noHost.title')} description={t('noHost.body')} />
    );
  }

  const isServices = mode === 'services';
  const rows = isServices ? serviceRows : startupRows;

  return (
    <div className="flex flex-col gap-4">
      <header className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{t(`${mode}.title`)}</h2>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t(`${mode}.subtitle`)}</p>
        </div>
        {/* The spin is the only signal that a re-read is happening. Screens
            stay mounted now, so returning to this tab refreshes underneath a
            list that is already drawn — without this the data would change
            under the user with no explanation. */}
        <Button variant="ghost" size="sm" onClick={state.refresh} disabled={state.refreshing}>
          <RefreshCw aria-hidden className={cn('size-4', state.refreshing && 'animate-spin')} />
          {state.refreshing ? t('refreshing') : t('refresh')}
        </Button>
      </header>

      {state.error !== null && state.error !== NO_HOST && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {t('stale', { message: state.error })}
        </p>
      )}

      <Summary snapshot={snapshot} mode={mode} />

      <div className="flex flex-wrap items-center gap-2">
        <SearchInput
          value={query}
          onValueChange={setQuery}
          placeholder={t(`${mode}.search`)}
          aria-label={t(`${mode}.search`)}
          clearLabel={t(`${mode}.clear`)}
          className="min-w-56 flex-1"
        />
        <SegmentedControl
          value={isServices ? serviceFilter : startupFilter}
          ariaLabel={t(`${mode}.filterLabel`)}
          onValueChange={(next) => {
            if (isServices) setServiceFilter(next as ServiceFilter);
            else setStartupFilter(next as StartupFilter);
          }}
          options={(isServices ? serviceFilters : startupFilters).map((id) => ({
            value: id,
            label: t(`filter.${id}`),
          }))}
        />
      </div>

      {rows.length === 0 ? (
        <EmptyState title={t('empty.title')} description={t('empty.body')} />
      ) : isServices ? (
        <ServiceTable rows={serviceRows} />
      ) : (
        <StartupTable rows={startupRows} />
      )}
    </div>
  );
}

function Summary({
  snapshot,
  mode,
}: {
  readonly snapshot: ReturnType<typeof useStartup>['snapshot'];
  readonly mode: 'startup' | 'services';
}) {
  const { t } = useTranslation(STARTUP_NS);
  if (snapshot === null) return null;

  const startup = countStartup(snapshot.entries);
  const services = countServices(snapshot.services);

  // Every figure that could be wrong in the "too low" direction is named, with
  // the reason. A list that quietly omits three entries is worse than one that
  // admits it, especially when the user is counting to decide what to disable.
  const caveats =
    mode === 'services'
      ? [
          services.unknownStartType > 0
            ? t('counts.unknownStartType', { count: services.unknownStartType })
            : null,
        ]
      : [
          startup.unknown > 0 ? t('counts.unknownState', { count: startup.unknown }) : null,
          snapshot.unreadableTasks > 0
            ? t('counts.unreadableTasks', { count: snapshot.unreadableTasks })
            : null,
        ];

  const shown = caveats.filter((caveat): caveat is string => caveat !== null);

  return (
    <div>
      <p className="text-sm">
        {mode === 'services'
          ? `${t('counts.services', { running: services.running, total: services.total })} · ${t('counts.automatic', { count: services.automatic })}`
          : t('counts.startup', { enabled: startup.enabled, total: startup.total })}
      </p>
      {shown.length > 0 && (
        <p className="mt-0.5 text-2xs text-[var(--color-fg-muted)]">
          {shown.join(' · ')} — {t('counts.undercountHint')}
        </p>
      )}
    </div>
  );
}

function StartupTable({ rows }: { readonly rows: readonly StartupEntry[] }) {
  const { t } = useTranslation(STARTUP_NS);

  return (
    <div className="overflow-x-auto rounded-md border border-[var(--color-border-subtle)]">
      <table className="w-full text-left">
        <thead>
          <tr className="text-2xs text-[var(--color-fg-muted)]">
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.name')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.source')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.state')}
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((entry) => (
            <tr
              key={`${entry.source}:${entry.name}`}
              className="border-t border-[var(--color-border-subtle)]"
            >
              <td className="px-2.5 py-1.5">
                <span className="block truncate text-sm">{labelFor(entry)}</span>
                {entry.command !== null && (
                  <span className="block truncate font-mono text-2xs text-[var(--color-fg-subtle)]">
                    {entry.command}
                  </span>
                )}
              </td>
              <td className="px-2.5 py-1.5 text-2xs">
                {t(`source.${entry.source}`)}
                {isMachineWide(entry) && (
                  <Badge tone="neutral" title={t('allUsersHint')} className="ml-1.5">
                    {t('allUsers')}
                  </Badge>
                )}
              </td>
              <td className="px-2.5 py-1.5 text-2xs">
                <Badge
                  tone={
                    entry.state === 'enabled'
                      ? 'ok'
                      : entry.state === 'disabled'
                        ? 'neutral'
                        : 'warn'
                  }
                >
                  {t(`state.${entry.state}`)}
                </Badge>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ServiceTable({ rows }: { readonly rows: readonly ServiceEntry[] }) {
  const { t } = useTranslation(STARTUP_NS);

  return (
    <div className="overflow-x-auto rounded-md border border-[var(--color-border-subtle)]">
      <table className="w-full text-left">
        <thead>
          <tr className="text-2xs text-[var(--color-fg-muted)]">
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.name')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.state')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.startType')}
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((service) => (
            <tr key={service.name} className="border-t border-[var(--color-border-subtle)]">
              <td className="px-2.5 py-1.5">
                <span className="block truncate text-sm">{labelFor(service)}</span>
                <span className="block truncate font-mono text-2xs text-[var(--color-fg-subtle)]">
                  {service.name}
                </span>
              </td>
              <td className="px-2.5 py-1.5 text-2xs">
                <Badge tone={service.state === 'running' ? 'ok' : 'neutral'}>
                  {t(`serviceState.${service.state}`)}
                </Badge>
                {/* Grouped services share one process, so their CPU and memory
                    cannot be attributed individually. Saying so beats showing
                    the host's whole footprint against each of a dozen
                    services, which is what Task Manager appears to do. */}
                {service.svchostGroup !== null && (
                  <Badge tone="info" title={t('sharedHint')} className="ml-1.5">
                    {t('shared')}
                  </Badge>
                )}
              </td>
              <td className="px-2.5 py-1.5 text-2xs">
                <span
                  className={
                    service.startType === 'unknown' ? 'text-[var(--color-status-warn)]' : undefined
                  }
                >
                  {t(`startType.${service.startType}`)}
                </span>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function StartupSkeleton(): React.JSX.Element {
  return (
    <div aria-busy="true" className="space-y-2">
      <Skeleton className="h-7 w-64" />
      <Skeleton className="h-9 w-full" />
      {[0, 1, 2, 3, 4, 5].map((index) => (
        <Skeleton key={index} className="h-10" />
      ))}
    </div>
  );
}
