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

import { MoreHorizontal, RefreshCw, ShieldAlert } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  Checkbox,
  ContextMenu,
  ContextMenuTrigger,
  DialogContent,
  DialogRoot,
  DropdownMenu,
  DropdownMenuTrigger,
  EmptyState,
  IconButton,
  SearchInput,
  SegmentedControl,
  Skeleton,
  cn,
  focusRing,
  formatBytes,
} from '@vitals/ui';

import { ExportButton } from '../../components/ExportButton';
import { useRowMenu } from '../../lib/useRowMenu';
import { errorMessage, isCommandError } from '../../lib/commandError';
import type { ExportColumn } from '../../lib/export';
import { oneOf, useUrlState } from '../../lib/useUrlState';
import {
  countServices,
  countStartup,
  filterServices,
  filterStartup,
  formatCpuSeconds,
  hideMicrosoft,
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
import {
  RowContextMenu,
  RowDropdownMenu,
  serviceKey,
  startupKey,
  useRowMenus,
  type ActionRequest,
  type RowMenuModel,
} from './RowMenu';
import { tauriStartupActions, type StartupActions } from './startupActions';
import { STARTUP_NS } from './strings';
import { NO_HOST, useStartup, type StartupReader } from './useStartup';

const microsoftModes = ['hide', 'show'] as const;
type MicrosoftMode = (typeof microsoftModes)[number];

export interface StartupScreenProps {
  /** `services` asks the backend for start types, which costs an SCM call each. */
  readonly mode: 'startup' | 'services';
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly reader?: StartupReader;
  /** Injectable for the same reason, and because every call changes the machine. */
  readonly actions?: StartupActions;
}

export function StartupScreen({
  mode,
  reader,
  actions = tauriStartupActions,
}: StartupScreenProps): React.JSX.Element {
  const { t, i18n } = useTranslation(STARTUP_NS);
  const locale = i18n.language;
  const state = useStartup(mode === 'services', reader);

  // One component, two routes, so the fragment is keyed on the mode: a query
  // typed into Services must not reappear when the user opens Startup.
  // Microsoft entries are hidden by default, as msconfig does: on a stock
  // install they are most of the list, and they are rarely what slows a
  // machine down. The default writes no fragment, so only "show" is recorded.
  const [view, patchView] = useUrlState<{
    q: string;
    startup: StartupFilter;
    service: ServiceFilter;
    ms: MicrosoftMode;
  }>(
    mode,
    { q: '', startup: 'all', service: 'all', ms: 'hide' },
    {
      startup: oneOf(startupFilters),
      service: oneOf(serviceFilters),
      ms: oneOf(microsoftModes),
    },
  );
  const query = view.q;
  const startupFilter = view.startup;
  const serviceFilter = view.service;
  const hidingMicrosoft = view.ms === 'hide';
  const setQuery = (q: string): void => {
    patchView({ q });
  };

  const snapshot = state.snapshot;

  // The Microsoft filter runs before the export sees the rows, so a saved
  // file holds exactly what was on screen rather than a longer list the user
  // never looked at.
  const startupView = useMemo(() => {
    const shown = hideMicrosoft(
      filterStartup(snapshot?.entries ?? [], startupFilter, query),
      hidingMicrosoft,
    );
    return { rows: sortStartup(shown.rows), hidden: shown.hidden };
  }, [snapshot, startupFilter, query, hidingMicrosoft]);
  const serviceView = useMemo(() => {
    const shown = hideMicrosoft(
      filterServices(snapshot?.services ?? [], serviceFilter, query),
      hidingMicrosoft,
    );
    return { rows: sortServices(shown.rows), hidden: shown.hidden };
  }, [snapshot, serviceFilter, query, hidingMicrosoft]);
  const startupRows = startupView.rows;
  const serviceRows = serviceView.rows;

  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [confirming, setConfirming] = useState<ActionRequest | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [failure, setFailure] = useState<string | null>(null);

  const report = (cause: unknown): void => {
    // A refusal is the user pressing No on UAC, or Windows saying no: its own
    // message is the whole story, and prefixing "That did not work" would
    // blame the app for a choice the user made.
    const message = errorMessage(cause);
    setFailure(
      isCommandError(cause) && cause.kind === 'refused' ? message : t('failed', { message }),
    );
  };

  const run = (request: ActionRequest, confirmed: boolean): void => {
    setConfirming(null);
    setNotice(null);
    setFailure(null);
    setBusyKey(request.key);
    void request
      .run(confirmed)
      .then(() => {
        setNotice(t(`done.${request.verb}`, { name: request.name }));
        // The list is not polled, so without this the row would keep showing
        // the state the user just changed.
        state.refresh();
      })
      .catch(report)
      .finally(() => {
        setBusyKey(null);
      });
  };

  const menus = useRowMenus({
    actions,
    busyKey,
    request: (request) => {
      // Only a change that makes less run, on something the backend says
      // matters, is worth interrupting for. Asking before every Enable would
      // train the user to click through the one dialog that counts.
      const risky = request.risk === 'degrades' || request.risk === 'systemCritical';
      if (request.reduces && risky) setConfirming(request);
      else run(request, false);
    },
    attempt: (work) => {
      setFailure(null);
      void work().catch(report);
    },
  });

  const startupColumns = useMemo(
    (): readonly ExportColumn<StartupEntry>[] => [
      { id: 'name', header: t('column.name'), value: (entry) => labelFor(entry) },
      { id: 'publisher', header: t('column.publisher'), value: (entry) => entry.publisher },
      { id: 'source', header: t('column.source'), value: (entry) => entry.source },
      { id: 'state', header: t('column.state'), value: (entry) => entry.state },
      { id: 'command', header: t('column.command'), value: (entry) => entry.command },
      { id: 'imagePath', header: t('column.path'), value: (entry) => entry.imagePath },
      { id: 'pid', header: t('column.pid'), value: (entry) => entry.pid },
      // Raw units in the export, formatted ones on screen: a spreadsheet
      // sorts 4213 correctly and "4.2 s" not at all.
      {
        id: 'impactCpuMs',
        header: t('column.impactCpu'),
        value: (entry) => entry.impact?.cpuMs ?? null,
      },
      {
        id: 'impactDiskBytes',
        header: t('column.impactDisk'),
        value: (entry) => entry.impact?.diskBytes ?? null,
      },
    ],
    [t],
  );
  const serviceColumns = useMemo(
    (): readonly ExportColumn<ServiceEntry>[] => [
      { id: 'displayName', header: t('column.name'), value: (service) => labelFor(service) },
      { id: 'name', header: t('column.serviceName'), value: (service) => service.name },
      { id: 'state', header: t('column.state'), value: (service) => service.state },
      { id: 'startType', header: t('column.startType'), value: (service) => service.startType },
      { id: 'pid', header: t('column.pid'), value: (service) => service.pid },
      { id: 'binaryPath', header: t('column.path'), value: (service) => service.binaryPath },
      {
        id: 'svchostGroup',
        header: t('column.sharedGroup'),
        value: (service) => service.svchostGroup,
      },
    ],
    [t],
  );

  if (state.pending) return <StartupSkeleton />;

  if (state.error === NO_HOST) {
    return (
      <EmptyState icon={<ShieldAlert />} title={t('noHost.title')} description={t('noHost.body')} />
    );
  }

  const isServices = mode === 'services';
  const rows = isServices ? serviceRows : startupRows;
  const hiddenCount = isServices ? serviceView.hidden : startupView.hidden;

  return (
    <div className="screen">
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
            // The control's value type is the union of both filter sets, so
            // it is narrowed by the mode rather than by the type system.
            if (isServices) patchView({ service: next as ServiceFilter });
            else patchView({ startup: next as StartupFilter });
          }}
          options={(isServices ? serviceFilters : startupFilters).map((id) => ({
            value: id,
            label: t(`filter.${id}`),
          }))}
        />
        {isServices ? (
          <ExportButton name="services" rows={serviceRows} columns={serviceColumns} />
        ) : (
          <ExportButton name="startup" rows={startupRows} columns={startupColumns} />
        )}
      </div>

      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <Checkbox
          checked={hidingMicrosoft}
          onCheckedChange={(checked) => {
            patchView({ ms: checked === true ? 'hide' : 'show' });
          }}
          label={t(isServices ? 'microsoft.hideServices' : 'microsoft.hideStartup')}
        />
        {/* Said out loud so a shorter list is never mistaken for a scan
            that missed things. */}
        {hiddenCount > 0 && (
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t('microsoft.hidden', { count: hiddenCount })}
          </p>
        )}
      </div>

      {notice !== null && (
        <p role="status" className="text-2xs text-[var(--color-fg-muted)]">
          {notice}
        </p>
      )}
      {failure !== null && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {failure}
        </p>
      )}

      {rows.length === 0 ? (
        <EmptyState title={t('empty.title')} description={t('empty.body')} />
      ) : isServices ? (
        <ServiceTable rows={serviceRows} menuFor={menus.service} />
      ) : (
        <StartupTable rows={startupRows} locale={locale} menuFor={menus.startup} />
      )}

      {!isServices && snapshot !== null && (
        <ImpactCaption measuredAtMs={snapshot.impactMeasuredAtMs} locale={locale} />
      )}

      <DialogRoot
        open={confirming !== null}
        onOpenChange={(open) => {
          if (!open) setConfirming(null);
        }}
      >
        {confirming !== null && (
          <DialogContent
            title={t(confirmTitleKey(confirming.verb), { name: confirming.name })}
            closeLabel={t('confirm.close')}
          >
            <p className="text-sm">
              {t(
                confirming.risk === 'systemCritical'
                  ? 'confirm.body.systemCritical'
                  : 'confirm.body.degrades',
              )}
            </p>
            <div className="mt-4 flex justify-end gap-2">
              <Button
                variant="ghost"
                onClick={() => {
                  setConfirming(null);
                }}
              >
                {t('confirm.cancel')}
              </Button>
              <Button
                variant="danger"
                onClick={() => {
                  run(confirming, true);
                }}
              >
                {t('confirm.confirm')}
              </Button>
            </div>
          </DialogContent>
        )}
      </DialogRoot>
    </div>
  );
}

/**
 * Only reducing verbs reach the dialog, so the others map to the nearest
 * title rather than a key that would never be shown.
 */
function confirmTitleKey(verb: ActionRequest['verb']) {
  switch (verb) {
    case 'stop':
      return 'confirm.title.stop';
    case 'restart':
      return 'confirm.title.restart';
    case 'manual':
      return 'confirm.title.manual';
    case 'disabled':
      return 'confirm.title.disabled';
    default:
      return 'confirm.title.disable';
  }
}

/**
 * The trailing "⋯" cell. A right-click menu alone is invisible; this is the
 * affordance that tells a mouse user the row has actions at all.
 */
function ActionsCell({ menu }: { readonly menu: RowMenuModel }) {
  const { t } = useTranslation(STARTUP_NS);
  return (
    <td className="w-8 px-1 py-1.5 text-right">
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <IconButton
            size="sm"
            icon={<MoreHorizontal aria-hidden />}
            label={t('action.rowActions', { name: menu.label })}
          />
        </DropdownMenuTrigger>
        <RowDropdownMenu menu={menu} />
      </DropdownMenu>
    </td>
  );
}

function ActionsHeader() {
  const { t } = useTranslation(STARTUP_NS);
  return (
    <th scope="col" className="w-8 px-1 py-1.5">
      <span className="sr-only">{t('action.actions')}</span>
    </th>
  );
}

/** Focusable so Shift+F10 and the Menu key reach the row's context menu. */
const rowClass = cn('border-t border-[var(--color-border-subtle)]', focusRing);

/**
 * Says which boot the Startup cost column describes, or that none has been
 * measured. A column of dashes with no explanation reads as broken; a column
 * of dashes with "Vitals was not running at boot" reads as true.
 */
function ImpactCaption({
  measuredAtMs,
  locale,
}: {
  readonly measuredAtMs: number | null;
  readonly locale: string;
}) {
  const { t } = useTranslation(STARTUP_NS);
  if (measuredAtMs === null) {
    return <p className="text-2xs text-[var(--color-fg-muted)]">{t('impact.unmeasured')}</p>;
  }
  const date = new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(
    new Date(measuredAtMs),
  );
  return <p className="text-2xs text-[var(--color-fg-muted)]">{t('impact.measured', { date })}</p>;
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

function StartupTable({
  rows,
  locale,
  menuFor,
}: {
  readonly rows: readonly StartupEntry[];
  readonly locale: string;
  readonly menuFor: (entry: StartupEntry) => RowMenuModel;
}) {
  const { t } = useTranslation(STARTUP_NS);
  const rowMenu = useRowMenu();

  return (
    <div className="table-scroll" onKeyDown={rowMenu.onKeyDown}>
      <table className="w-full text-left">
        <thead>
          <tr className="text-2xs text-[var(--color-fg-muted)]">
            <th scope="col" className="cell-fill px-2.5 py-1.5 font-normal">
              {t('column.name')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.source')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.state')}
            </th>
            <th
              scope="col"
              className="px-2.5 py-1.5 text-right font-normal"
              title={t('impact.hint')}
            >
              {t('column.impact')}
            </th>
            <ActionsHeader />
          </tr>
        </thead>
        <tbody>
          {rows.map((entry) => {
            const menu = menuFor(entry);
            return (
              <ContextMenu key={startupKey(entry)} {...rowMenu.rootProps(startupKey(entry))}>
                <ContextMenuTrigger asChild>
                  <tr
                    tabIndex={0}
                    className={rowClass}
                    data-testid="startup-row"
                    onContextMenu={rowMenu.onContextMenu}
                  >
                    <td className="cell-fill px-2.5 py-1.5">
                      <span className="block truncate text-sm">{labelFor(entry)}</span>
                      {entry.command !== null && (
                        <span
                          className="block truncate font-mono text-2xs text-[var(--color-fg-subtle)]"
                          title={entry.command}
                        >
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
                    <td
                      className="px-2.5 py-1.5 text-right font-mono text-2xs tabular-nums"
                      data-testid="startup-impact"
                      {...(entry.impact === null && { title: t('impact.notSeen') })}
                    >
                      {/* Two figures, both honest: a null impact is an em dash in
                    each, never a zero. */}
                      <span className="block">
                        {formatCpuSeconds(entry.impact?.cpuMs ?? null, locale)}
                      </span>
                      <span className="block text-[var(--color-fg-subtle)]">
                        {formatBytes(entry.impact?.diskBytes ?? null, locale)}
                      </span>
                    </td>
                    <ActionsCell menu={menu} />
                  </tr>
                </ContextMenuTrigger>
                <RowContextMenu menu={menu} />
              </ContextMenu>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function ServiceTable({
  rows,
  menuFor,
}: {
  readonly rows: readonly ServiceEntry[];
  readonly menuFor: (service: ServiceEntry) => RowMenuModel;
}) {
  const { t } = useTranslation(STARTUP_NS);
  const rowMenu = useRowMenu();

  return (
    <div className="table-scroll" onKeyDown={rowMenu.onKeyDown}>
      <table className="w-full text-left">
        <thead>
          <tr className="text-2xs text-[var(--color-fg-muted)]">
            <th scope="col" className="cell-fill px-2.5 py-1.5 font-normal">
              {t('column.name')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.state')}
            </th>
            <th scope="col" className="px-2.5 py-1.5 font-normal">
              {t('column.startType')}
            </th>
            <ActionsHeader />
          </tr>
        </thead>
        <tbody>
          {rows.map((service) => {
            const menu = menuFor(service);
            return (
              <ContextMenu key={serviceKey(service)} {...rowMenu.rootProps(serviceKey(service))}>
                <ContextMenuTrigger asChild>
                  <tr
                    tabIndex={0}
                    className={rowClass}
                    data-testid="service-row"
                    onContextMenu={rowMenu.onContextMenu}
                  >
                    <td className="cell-fill px-2.5 py-1.5">
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
                          service.startType === 'unknown'
                            ? 'text-[var(--color-status-warn)]'
                            : undefined
                        }
                      >
                        {t(`startType.${service.startType}`)}
                      </span>
                    </td>
                    <ActionsCell menu={menu} />
                  </tr>
                </ContextMenuTrigger>
                <RowContextMenu menu={menu} />
              </ContextMenu>
            );
          })}
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
