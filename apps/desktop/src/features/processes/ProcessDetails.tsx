/**
 * The detail panel for the focused row.
 *
 * Reads from the live snapshot rather than from a copy taken at selection
 * time, so the numbers here and the numbers in the table can never disagree —
 * two different readings of the same process on one screen is the kind of
 * detail that makes people stop trusting a monitor.
 */

import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  EmptyState,
  Switch,
  formatBytes,
  formatCount,
  formatPercent,
  formatThroughput,
  formatUptime,
} from '@vitals/ui';

import {
  displayName,
  type HandleInfo,
  type HostInfo,
  type ModuleInfo,
  type Process,
} from '@vitals/protocol';

import { errorMessage, type ProcessActionsApi } from './actions';
import { affinityPresets, type AffinityPreset } from './affinity';
import { UNKNOWN } from './constants';
import { flagKeys, type ProcessRow } from './model';
import { PROCESSES_NS, fallback } from './strings';

/**
 * The most rows either lazy list will render.
 *
 * A browser holds several thousand handles. The list is not virtualised
 * because the panel is a 288 px sidebar with its own scroll, and a
 * virtualiser inside a virtualised table's sibling fights it for the wheel.
 * A hard cap with an honest "showing N of M" is the smaller design.
 */
export const LAZY_LIST_CAP = 200;

export interface ProcessDetailsProps {
  readonly row: ProcessRow | null;
  readonly locale: string;
  readonly actions: ProcessActionsApi;
  /**
   * Static machine facts, for the affinity presets. `null` while loading or
   * on a platform with no backend — presets that need the topology are then
   * omitted rather than guessed.
   */
  readonly host: HostInfo | null;
  /** Full executable path, when the detail fetch has provided one. */
  readonly executablePath: string | null;
  readonly onFailure: (message: string) => void;
}

export function ProcessDetails({
  row,
  locale,
  actions,
  host,
  executablePath,
  onFailure,
}: ProcessDetailsProps): React.JSX.Element {
  const { t } = useTranslation();

  if (row === null) {
    return (
      <aside
        className="w-72 shrink-0 border-l border-[var(--color-border-subtle)] p-4"
        aria-label={t('process.detail.title', fallback('process.detail.title'))}
      >
        <EmptyState
          title={t('process.detail.title', fallback('process.detail.title'))}
          description={t('process.detail.none', fallback('process.detail.none'))}
        />
      </aside>
    );
  }

  const p = row.process;
  const flags = flagKeys(p.flags);

  return (
    <aside
      className="w-72 shrink-0 space-y-4 overflow-auto border-l border-[var(--color-border-subtle)] p-4"
      aria-label={t('process.detail.title', fallback('process.detail.title'))}
    >
      <header className="space-y-1">
        <h2 className="truncate text-sm font-semibold" title={displayName(p)}>
          {displayName(p)}
        </h2>
        <p className="text-2xs text-[var(--color-fg-muted)]">
          {displayName(p) !== p.name && <span className="font-mono">{p.name} · </span>}
          {t('process.pid')} {p.key.pid} ·{' '}
          {t(`process.kind.${p.kind}`, fallback(`process.kind.${p.kind}` as never))}
        </p>
      </header>

      <Section title={t('process.detail.identity', fallback('process.detail.identity'))}>
        <Field label={t('process.user')} value={p.user ?? UNKNOWN} />
        <Field label={t('process.status')} value={t(`process.state.${p.state}`)} />
        <Field label={t('process.uptime')} value={formatUptime(p.uptimeSecs)} />
        <Field
          label={t('process.integrity')}
          value={
            p.integrity === null
              ? UNKNOWN
              : t(`process.integrityLevel.${p.integrity}`, { defaultValue: p.integrity })
          }
          hidden={p.integrity === null}
        />
      </Section>

      <Section title={t('process.detail.resources', fallback('process.detail.resources'))}>
        <Field
          label={t('process.column.cpu', fallback('process.column.cpu'))}
          value={formatPercent(p.cpu, locale)}
        />
        <Field
          label={t('process.column.memory', fallback('process.column.memory'))}
          value={formatBytes(p.memoryPrivate, locale)}
        />
        <Field label={t('process.memoryInUse')} value={formatBytes(p.memoryWorkingSet, locale)} />
        <Field
          label={t('process.column.disk', fallback('process.column.disk'))}
          value={formatThroughput(p.diskRead + p.diskWrite, locale)}
        />
        {/* Same as GPU below: unmeasured, not zero. Per-process network needs
            an ETW session the unelevated app cannot start. */}
        <Field
          label={t('process.column.network', fallback('process.column.network'))}
          value={
            p.netRx === null || p.netTx === null
              ? UNKNOWN
              : formatThroughput(p.netRx + p.netTx, locale)
          }
        />
        {/* Absent GPU telemetry renders as an em-dash. Showing 0% would be a
            measurement we did not take. */}
        <Field
          label={t('process.column.gpu', fallback('process.column.gpu'))}
          value={p.gpu === null ? UNKNOWN : formatPercent(p.gpu, locale)}
        />
        <Field label={t('process.threads')} value={formatCount(p.threadCount, locale)} />
        <Field
          label={t('process.handles')}
          value={p.handleCount === null ? UNKNOWN : formatCount(p.handleCount, locale)}
        />
      </Section>

      <EfficiencySection process={p} actions={actions} onFailure={onFailure} />

      <AffinitySection process={p} actions={actions} host={host} onFailure={onFailure} />

      <FileSection path={executablePath} actions={actions} onFailure={onFailure} />

      {/* Keyed on the process so an expanded Handles list for chrome.exe is
          not shown, stale, under the next row the user clicks. */}
      <LazyList
        key={`handles-${row.id}`}
        kind="handles"
        process={p}
        fetch={(target) => actions.getHandles(target)}
        render={(handle: HandleInfo) => (
          <>
            <span className="text-[var(--color-fg-muted)]">
              {handle.kind ?? t(`${PROCESSES_NS}:detail.handles.untyped`)}
            </span>{' '}
            <span className="break-all">
              {handle.name ?? t(`${PROCESSES_NS}:detail.handles.unnamed`)}
            </span>
          </>
        )}
        keyOf={(handle: HandleInfo) => handle.value}
        locale={locale}
      />

      <LazyList
        key={`modules-${row.id}`}
        kind="modules"
        process={p}
        fetch={(target) => actions.getModules(target)}
        render={(module: ModuleInfo) => (
          <>
            <span title={module.path ?? undefined}>{module.name}</span>{' '}
            <span className="text-[var(--color-fg-muted)]">{formatBytes(module.size, locale)}</span>
          </>
        )}
        keyOf={(module: ModuleInfo) => module.baseAddress}
        locale={locale}
      />

      {flags.length > 0 && (
        <Section title={t('process.detail.flags', fallback('process.detail.flags'))}>
          <div className="flex flex-wrap gap-1">
            {flags.map((flag) => (
              <Badge
                key={flag}
                tone={flag === 'critical' || flag === 'signatureBroken' ? 'danger' : 'neutral'}
              >
                {t(`process.flag.${flag}`, fallback(`process.flag.${flag}` as never))}
              </Badge>
            ))}
          </div>
        </Section>
      )}
    </aside>
  );
}

/**
 * The efficiency-mode switch.
 *
 * Three states, not two. `null` — the backend could not read it, because the
 * process denied us a handle — renders as the unavailable label with a
 * disabled switch. Rendering it as "off" would offer to turn on a throttle
 * the same denial will refuse to set, and a switch that fails every time it
 * is touched teaches the user the whole panel is decorative.
 */
function EfficiencySection({
  process,
  actions,
  onFailure,
}: {
  readonly process: Process;
  readonly actions: ProcessActionsApi;
  readonly onFailure: (message: string) => void;
}): React.JSX.Element {
  const { t } = useTranslation(PROCESSES_NS);
  // `undefined` = not yet asked; `null` = asked, unreadable.
  const [mode, setMode] = useState<boolean | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);

  // The live snapshot replaces every row object each tick. The read below
  // must fire once per *process*, not once per tick — a handle open per
  // second for the selected row is the churn the sampler exists to avoid —
  // so the effect depends on the identity fields, and reads the current
  // object through a ref written after render.
  const latest = useRef(process);
  useEffect(() => {
    latest.current = process;
  }, [process]);

  const { pid, startTime } = process.key;
  useEffect(() => {
    let live = true;
    void actions
      .getEfficiencyMode(latest.current)
      .then((value) => {
        if (live) setMode(value);
      })
      .catch(() => {
        // A vanished process or a platform without the backend: both are
        // "unknown", and unknown is what the switch already renders.
        if (live) setMode(null);
      });
    return () => {
      live = false;
    };
  }, [pid, startTime, actions]);

  const unreadable = mode === null;
  const pending = mode === undefined;

  const toggle = (enabled: boolean): void => {
    setBusy(true);
    void actions
      .setEfficiencyMode(process, enabled)
      .then(() => actions.getEfficiencyMode(process))
      .then((value) => setMode(value))
      .catch((error: unknown) => {
        onFailure(t('detail.efficiency.failed', { message: errorMessage(error) }));
      })
      .finally(() => setBusy(false));
  };

  return (
    <Section title={t('detail.efficiency.label')}>
      <Switch
        label={
          <span className="text-2xs">
            {unreadable || pending
              ? unreadable
                ? t('detail.efficiency.unavailable')
                : UNKNOWN
              : mode
                ? t('detail.efficiency.on')
                : t('detail.efficiency.off')}
          </span>
        }
        description={t('detail.efficiency.description')}
        checked={mode === true}
        disabled={unreadable || pending || busy}
        onCheckedChange={toggle}
        data-testid="efficiency-switch"
      />
    </Section>
  );
}

/**
 * Affinity presets, only when the machine can express them.
 *
 * Nothing is rendered without host info: the presets need the logical core
 * count at minimum, and the interesting ones need the topology. A section
 * that says "Run on" and offers nothing is a broken-looking section.
 */
function AffinitySection({
  process,
  actions,
  host,
  onFailure,
}: {
  readonly process: Process;
  readonly actions: ProcessActionsApi;
  readonly host: HostInfo | null;
  readonly onFailure: (message: string) => void;
}): React.JSX.Element | null {
  const { t } = useTranslation(PROCESSES_NS);
  const presets = affinityPresets(host);
  if (presets.length === 0) return null;

  const apply = (preset: AffinityPreset): void => {
    void actions.setAffinity(process, preset.mask).catch((error: unknown) => {
      onFailure(t('detail.affinity.failed', { name: process.name, message: errorMessage(error) }));
    });
  };

  return (
    <Section title={t('detail.affinity.title')}>
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('detail.affinity.hint')}</p>
      <div className="flex flex-wrap gap-1" data-testid="affinity-presets">
        {presets.map((preset) => (
          <Button key={preset.id} size="sm" variant="secondary" onClick={() => apply(preset)}>
            {t(`detail.affinity.preset.${preset.id}`, { cores: preset.cores })}
          </Button>
        ))}
      </div>
    </Section>
  );
}

/**
 * "Open file location" and "Properties".
 *
 * Enabled only when the path is known. The backend refuses a path that does
 * not exist, but the reason to disable here is different: an enabled button
 * that always fails for protected processes would make the user try it
 * three times before concluding it is broken. Disabled with a reason is
 * honest on the first look.
 */
function FileSection({
  path,
  actions,
  onFailure,
}: {
  readonly path: string | null;
  readonly actions: ProcessActionsApi;
  readonly onFailure: (message: string) => void;
}): React.JSX.Element {
  const { t } = useTranslation();
  const { t: tp } = useTranslation(PROCESSES_NS);
  const known = path !== null;
  const reason = known ? undefined : tp('detail.file.unknownPath');

  return (
    <Section title={t('process.action.openLocation')}>
      <div className="flex flex-wrap gap-1">
        <Button
          size="sm"
          variant="secondary"
          disabled={!known}
          {...(reason !== undefined && { title: reason })}
          onClick={() => {
            if (path === null) return;
            void actions.openFileLocation(path).catch((error: unknown) => {
              onFailure(tp('detail.file.openFailed', { message: errorMessage(error) }));
            });
          }}
        >
          {t('process.action.openLocation')}
        </Button>
        <Button
          size="sm"
          variant="secondary"
          disabled={!known}
          {...(reason !== undefined && { title: reason })}
          onClick={() => {
            if (path === null) return;
            void actions.showFileProperties(path).catch((error: unknown) => {
              onFailure(tp('detail.file.propertiesFailed', { message: errorMessage(error) }));
            });
          }}
        >
          {t('process.action.properties')}
        </Button>
      </div>
    </Section>
  );
}

type LazyState<T> =
  | { readonly status: 'collapsed' }
  | { readonly status: 'loading' }
  | { readonly status: 'loaded'; readonly items: readonly T[] }
  | { readonly status: 'failed'; readonly message: string };

/**
 * A section that fetches its contents the first time it is expanded.
 *
 * On demand, never eagerly: the handle backend walks the whole system handle
 * table — megabytes on a busy machine, hundreds of milliseconds to name the
 * objects — and the module backend probes the target's address space. Doing
 * either for every row the user arrows past would make the detail panel the
 * most expensive thing on the screen. The fetch runs once per expansion and
 * the result is held until the row changes (the parent keys this component
 * on the row id).
 */
function LazyList<T>({
  kind,
  process,
  fetch,
  render,
  keyOf,
  locale,
}: {
  readonly kind: 'handles' | 'modules';
  readonly process: Process;
  readonly fetch: (process: Process) => Promise<readonly T[]>;
  readonly render: (item: T) => React.ReactNode;
  readonly keyOf: (item: T) => string | number;
  readonly locale: string;
}): React.JSX.Element {
  const { t } = useTranslation(PROCESSES_NS);
  const [state, setState] = useState<LazyState<T>>({ status: 'collapsed' });

  const expanded = state.status !== 'collapsed';

  const toggle = (): void => {
    if (expanded) {
      setState({ status: 'collapsed' });
      return;
    }
    setState({ status: 'loading' });
    void fetch(process)
      .then((items) => setState({ status: 'loaded', items }))
      .catch((error: unknown) => setState({ status: 'failed', message: errorMessage(error) }));
  };

  const count = state.status === 'loaded' ? formatCount(state.items.length, locale) : null;
  const title =
    count === null ? t(`detail.${kind}.title`) : `${t(`detail.${kind}.title`)} · ${count}`;

  return (
    <section className="space-y-1.5">
      <h3 className="text-2xs font-medium tracking-wide text-[var(--color-fg-subtle)] uppercase">
        <button
          type="button"
          className="flex w-full items-center justify-between text-left hover:text-[var(--color-fg-default)]"
          aria-expanded={expanded}
          aria-label={t(`detail.${kind}.${expanded ? 'collapse' : 'expand'}`)}
          onClick={toggle}
          data-testid={`${kind}-toggle`}
        >
          <span>{title}</span>
          <span aria-hidden="true">{expanded ? '▾' : '▸'}</span>
        </button>
      </h3>
      {state.status === 'loading' && (
        <p className="text-2xs text-[var(--color-fg-muted)]" role="status">
          {t(`detail.${kind}.loading`)}
        </p>
      )}
      {state.status === 'failed' && (
        <p className="text-2xs text-[var(--color-status-danger)]" role="alert">
          {state.message}
        </p>
      )}
      {state.status === 'loaded' &&
        (state.items.length === 0 ? (
          <p className="text-2xs text-[var(--color-fg-muted)]">{t(`detail.${kind}.empty`)}</p>
        ) : (
          <>
            <ul className="max-h-64 space-y-0.5 overflow-auto font-mono text-2xs">
              {state.items.slice(0, LAZY_LIST_CAP).map((item) => (
                <li key={keyOf(item)} className="truncate">
                  {render(item)}
                </li>
              ))}
            </ul>
            {state.items.length > LAZY_LIST_CAP && (
              <p className="text-2xs text-[var(--color-fg-muted)]">
                {t(`detail.${kind}.capped`, {
                  shown: formatCount(LAZY_LIST_CAP, locale),
                  total: formatCount(state.items.length, locale),
                })}
              </p>
            )}
          </>
        ))}
    </section>
  );
}

function Section({
  title,
  children,
}: {
  readonly title: string;
  readonly children: React.ReactNode;
}): React.JSX.Element {
  return (
    <section className="space-y-1.5">
      <h3 className="text-2xs font-medium tracking-wide text-[var(--color-fg-subtle)] uppercase">
        {title}
      </h3>
      <dl className="space-y-1">{children}</dl>
    </section>
  );
}

function Field({
  label,
  value,
  hidden = false,
}: {
  readonly label: string;
  readonly value: string;
  readonly hidden?: boolean;
}): React.JSX.Element | null {
  if (hidden) return null;
  return (
    <div className="flex items-baseline justify-between gap-2 text-2xs">
      <dt className="text-[var(--color-fg-muted)]">{label}</dt>
      <dd className="font-mono text-[var(--color-fg-default)] tabular-nums">{value}</dd>
    </div>
  );
}
