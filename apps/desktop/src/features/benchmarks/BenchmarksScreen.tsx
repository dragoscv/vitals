/**
 * Benchmarks.
 *
 * # The conditions are the feature
 *
 * Anyone can print a number. The reason this screen exists is that a number
 * printed on its own is a lie of omission: the same machine scores half as
 * much on battery, less again while thermally throttled, and less still if
 * Windows Update happened to be installing during the run. So every result
 * carries the conditions it was taken under, and a result that failed the
 * conditions check is rendered as a warning with the specific cause named —
 * never as a dimmed row the user is left to interpret.
 *
 * # Nothing is hidden because it does not work
 *
 * Six of the ten benchmarks are not implemented. They are listed anyway, with
 * their checkbox disabled and the reason stated in text next to it. Hiding
 * them would make the feature look complete and leave the user wondering why
 * their disk never gets measured; offering an enabled control that fails on
 * click would be worse.
 *
 * # The warning comes before the run, not during it
 *
 * These take tens of seconds and make the machine unusable while they run.
 * The estimate and the consequence are stated next to the start button, so
 * the decision is made with the information rather than discovered after.
 */

import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  CheckSquare,
  Copy,
  Play,
  RefreshCw,
  Square,
} from 'lucide-react';
import { forwardRef, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  BrandMark,
  Button,
  Card,
  CardBody,
  CardHeader,
  CardTitle,
  Checkbox,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
  EmptyState,
  ProgressBar,
  Skeleton,
  cn,
  formatCount,
  formatPercent,
  formatTemperature,
} from '@vitals/ui';

import { useRowMenu, type RowMenu } from '../../lib/useRowMenu';
import {
  availableIds,
  byGroup,
  distrustReasons,
  estimatedSeconds,
  isTrustworthy,
  medianOf,
  spreadOf,
  type BenchmarkId,
  type BenchmarkInfo,
  type BenchmarkResultDto,
} from './model';
import { BENCHMARKS_NS } from './strings';
import { NO_HOST, useBenchmarks, type BenchmarksSource } from './useBenchmarks';

export interface BenchmarksScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly source?: BenchmarksSource;
}

export function BenchmarksScreen({ source }: BenchmarksScreenProps = {}): React.JSX.Element {
  const { t, i18n } = useTranslation(BENCHMARKS_NS);
  const locale = i18n.language;

  const state = useBenchmarks(source);
  const [deselected, setDeselected] = useState<ReadonlySet<BenchmarkId>>(new Set());
  const menu = useRowMenu();

  const runnable = useMemo(() => availableIds(state.infos), [state.infos]);
  // Tracked as an exclusion set rather than a selection: the listing arrives
  // after the first render, and a selection set initialised from an empty list
  // would start empty and stay empty unless it were resynchronised — which is
  // exactly the kind of effect-driven state this avoids needing.
  const selected = useMemo(
    () => new Set(runnable.filter((id) => !deselected.has(id))),
    [runnable, deselected],
  );

  const totalSeconds = useMemo(
    () => estimatedSeconds(state.infos, selected),
    [state.infos, selected],
  );

  if (state.pending) return <BenchmarksSkeleton />;

  if (state.listError === NO_HOST) {
    return (
      <EmptyState icon={<Activity />} title={t('noHost.title')} description={t('noHost.body')} />
    );
  }

  const toggle = (id: BenchmarkId, on: boolean): void => {
    setDeselected((previous) => {
      const next = new Set(previous);
      if (on) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const runningName = state.runningIds[0];

  // One menu for a test, wherever the test is drawn: the checkbox in the
  // selection and its result card offer the same actions, so neither place
  // is the only way to reach one of them.
  const menuFor = (id: BenchmarkId): TestMenuModel => {
    const testInfo = state.infos.find((candidate) => candidate.id === id);
    const available = testInfo?.available === true;
    const testResult = state.suite?.results.find((candidate) => candidate.id === id) ?? null;
    return {
      id,
      available,
      selected: selected.has(id),
      running: state.running,
      result:
        testResult === null
          ? null
          : resultSummary(testResult, locale, {
              name: t(`name.${testResult.id}`),
              verdict: isTrustworthy(testResult) ? t('trust.ok') : t('trust.bad'),
            }),
      onToggle: () => {
        toggle(id, !selected.has(id));
      },
      onRunOnly: () => {
        state.run([id]);
      },
      onRefresh: state.refresh,
    };
  };

  return (
    <div className="screen" onKeyDown={menu.onKeyDown}>
      <header className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{t('title')}</h2>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('subtitle')}</p>
        </div>
        <Button variant="ghost" size="sm" disabled={state.running} onClick={state.refresh}>
          <RefreshCw aria-hidden className="size-4" />
          {t('refresh')}
        </Button>
      </header>

      {state.listError !== null && state.listError !== NO_HOST && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {t('error.list', { message: state.listError })}
        </p>
      )}

      {/*
       * What to run and the Run button stay at the top; the results below
       * scroll on their own (S12-26), so re-running after reading the last
       * result no longer means scrolling back up for the button. Small
       * windows stack and scroll the whole body instead.
       */}
      <div className="screen-body grid-rows-[auto_minmax(var(--pane-min),1fr)]">
        <div className="flex flex-col gap-3">
          <Selection
            infos={state.infos}
            selected={selected}
            disabled={state.running}
            menu={menu}
            menuFor={menuFor}
            onToggle={toggle}
            onSelectAll={() => {
              setDeselected(new Set());
            }}
            onSelectNone={() => {
              setDeselected(new Set(runnable));
            }}
          />

          <StartPanel
            seconds={totalSeconds}
            count={selected.size}
            running={state.running}
            hasResults={state.suite !== null}
            onRun={() => {
              state.run([...selected]);
            }}
          />

          {state.runError !== null && (
            <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
              {/* Phrased as "the last completed run" when one survives, because
              the previous suite is deliberately still on screen below. */}
              {state.suite === null
                ? t('error.failed', { message: state.runError })
                : t('error.stale', { message: state.runError })}
            </p>
          )}

          {state.running && runningName !== undefined && (
            // Polite, and never assertive: an assertive region interrupts whatever
            // the user is reading, and a progress announcement is not worth that.
            // Named, because the busy button contributes a second `status` and two
            // unnamed live regions are indistinguishable to a screen reader.
            <div role="status" aria-label={t('running.region')} className="flex flex-col gap-1.5">
              <p className="flex items-center gap-2 text-sm">
                <BrandMark size={20} state="thinking" />
                {t('running.title', { name: t(`name.${runningName}`) })}
              </p>
              <p className="text-2xs text-[var(--color-fg-muted)]">{t('running.body')}</p>
              <p className="text-2xs text-[var(--color-fg-subtle)]">
                {t('running.remaining', { count: state.runningIds.length })}
              </p>
              {/* Indeterminate on purpose: the backend reports no fraction while a
              measurement loop is running, and a percentage here would be
              invented rather than measured. */}
              <ProgressBar indeterminate label={t('action.busy')} />
            </div>
          )}
        </div>

        <div className="pane-stack">
          {state.suite === null
            ? !state.running && <EmptyState title={t('idle.title')} description={t('idle.body')} />
            : state.suite.results.map((result) => (
                <ContextMenu key={result.id} {...menu.rootProps(`result:${result.id}`)}>
                  <ContextMenuTrigger asChild>
                    <Result result={result} locale={locale} onContextMenu={menu.onContextMenu} />
                  </ContextMenuTrigger>
                  <TestMenu model={menuFor(result.id)} />
                </ContextMenu>
              ))}

          {state.suite !== null && (
            <p className="text-2xs text-[var(--color-fg-subtle)]">
              {t('results.total', {
                seconds: formatCount(Math.round(state.suite.totalDurationMs / 1000), locale),
              })}{' '}
              {t('results.medianWhy')}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}

interface TestMenuModel {
  readonly id: BenchmarkId;
  readonly available: boolean;
  readonly selected: boolean;
  readonly running: boolean;
  /** The last result as copyable text, or `null` before this test has run. */
  readonly result: string | null;
  readonly onToggle: () => void;
  readonly onRunOnly: () => void;
  readonly onRefresh: () => void;
}

/** The headline, the passes and the conditions verdict, as one line of text. */
function resultSummary(
  result: BenchmarkResultDto,
  locale: string,
  labels: { readonly name: string; readonly verdict: string },
): string {
  const value = (n: number): string => `${formatCount(n, locale)} ${result.unit}`;
  return [
    labels.name,
    value(medianOf(result)),
    result.runs.map(value).join(', '),
    labels.verdict,
  ].join('\t');
}

function TestMenu({ model }: { readonly model: TestMenuModel }) {
  const { t } = useTranslation(BENCHMARKS_NS);
  // Same rule as the checkbox: nothing changes while a suite is running, and
  // a test the backend cannot run is never offered.
  const locked = model.running || !model.available;
  return (
    <ContextMenuContent>
      <ContextMenuLabel>{t(`name.${model.id}`)}</ContextMenuLabel>
      <ContextMenuItem disabled={locked} onSelect={model.onToggle}>
        {model.selected ? (
          <Square className="size-4" aria-hidden="true" />
        ) : (
          <CheckSquare className="size-4" aria-hidden="true" />
        )}
        {model.selected ? t('menu.deselect') : t('menu.select')}
      </ContextMenuItem>
      <ContextMenuItem disabled={locked} onSelect={model.onRunOnly}>
        <Play className="size-4" aria-hidden="true" />
        {t('menu.runOnly')}
      </ContextMenuItem>
      <ContextMenuItem
        disabled={model.result === null}
        onSelect={() => {
          if (model.result !== null) void globalThis.navigator?.clipboard?.writeText(model.result);
        }}
      >
        <Copy className="size-4" aria-hidden="true" />
        {t('menu.copyResult')}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem disabled={model.running} onSelect={model.onRefresh}>
        <RefreshCw className="size-4" aria-hidden="true" />
        {t('refresh')}
      </ContextMenuItem>
    </ContextMenuContent>
  );
}

function Selection({
  infos,
  selected,
  disabled,
  menu,
  menuFor,
  onToggle,
  onSelectAll,
  onSelectNone,
}: {
  readonly infos: readonly BenchmarkInfo[];
  readonly selected: ReadonlySet<BenchmarkId>;
  readonly disabled: boolean;
  readonly menu: RowMenu;
  readonly menuFor: (id: BenchmarkId) => TestMenuModel;
  readonly onToggle: (id: BenchmarkId, on: boolean) => void;
  readonly onSelectAll: () => void;
  readonly onSelectNone: () => void;
}) {
  const { t } = useTranslation(BENCHMARKS_NS);
  const sections = byGroup(infos);

  return (
    <fieldset className="flex flex-col gap-3 border-0 p-0">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <legend className="text-sm font-medium">{t('select.legend')}</legend>
        <div className="flex gap-1.5">
          <Button variant="ghost" size="sm" disabled={disabled} onClick={onSelectAll}>
            {t('select.all')}
          </Button>
          <Button variant="ghost" size="sm" disabled={disabled} onClick={onSelectNone}>
            {t('select.none')}
          </Button>
        </div>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {sections.map((section) => (
          <Card key={section.group}>
            <CardHeader>
              <CardTitle level={3}>{t(`group.${section.group}`)}</CardTitle>
            </CardHeader>
            <CardBody className="flex flex-col gap-2.5">
              {section.infos.map((info) => (
                <ContextMenu key={info.id} {...menu.rootProps(`choice:${info.id}`)}>
                  <ContextMenuTrigger asChild>
                    <BenchmarkChoice
                      info={info}
                      checked={selected.has(info.id)}
                      disabled={disabled}
                      onToggle={onToggle}
                      onContextMenu={menu.onContextMenu}
                    />
                  </ContextMenuTrigger>
                  <TestMenu model={menuFor(info.id)} />
                </ContextMenu>
              ))}
            </CardBody>
          </Card>
        ))}
      </div>
    </fieldset>
  );
}

/** Forwards ref and props: it is the `ContextMenuTrigger asChild` target. */
const BenchmarkChoice = forwardRef<
  HTMLDivElement,
  {
    readonly info: BenchmarkInfo;
    readonly checked: boolean;
    readonly disabled: boolean;
    readonly onToggle: (id: BenchmarkId, on: boolean) => void;
  } & Omit<React.ComponentPropsWithoutRef<'div'>, 'children' | 'onToggle'>
>(function BenchmarkChoice({ info, checked, disabled, onToggle, className, ...rest }, ref) {
  const { t } = useTranslation(BENCHMARKS_NS);
  const name = t(`name.${info.id}`);

  // The reason is looked up as a key first and falls through to the literal
  // string. That way a backend that starts sending a reason we have not
  // translated yet degrades to its own English rather than to a key path.
  const reasonKey = info.unavailableReason ?? 'unknown';
  const reason = t(`unavailable.${reasonKey}`, {
    defaultValue: t('unavailable.unknown'),
  });

  return (
    <div
      ref={ref}
      data-testid={`choice-${info.id}`}
      className={cn('flex flex-col gap-0.5', className)}
      {...rest}
    >
      <Checkbox
        checked={info.available && checked}
        disabled={disabled || !info.available}
        // The visible label carries the accessible name. A separate `ariaLabel`
        // would be ignored here anyway, and a name that differs from the
        // visible text breaks speech-control users saying "click CPU".
        label={name}
        // Exposed to assistive technology as well as visually. A disabled
        // control whose reason is only in adjacent grey text is unexplained
        // for anyone not reading the layout.
        {...(info.available ? {} : { 'aria-describedby': `${info.id}-reason` })}
        onCheckedChange={(next) => {
          onToggle(info.id, next === true);
        }}
      />
      {info.available ? (
        <p className="pl-6 text-2xs text-[var(--color-fg-subtle)]">
          {t(`describe.${info.id}`)} {t('results.duration', { seconds: info.estimatedSeconds })}
        </p>
      ) : (
        <div className="flex flex-col gap-0.5 pl-6">
          <Badge tone="neutral">{t('unavailable.badge')}</Badge>
          <p id={`${info.id}-reason`} className="text-2xs text-[var(--color-fg-muted)]">
            {reason}
          </p>
        </div>
      )}
    </div>
  );
});

function StartPanel({
  seconds,
  count,
  running,
  hasResults,
  onRun,
}: {
  readonly seconds: number;
  readonly count: number;
  readonly running: boolean;
  readonly hasResults: boolean;
  readonly onRun: () => void;
}) {
  const { t } = useTranslation(BENCHMARKS_NS);

  return (
    <Card>
      <CardBody className="flex flex-wrap items-center justify-between gap-3">
        <div className="min-w-0">
          {/* Stated before the click, not after. A minute-long operation that
              makes the machine unresponsive is a decision, and a decision
              needs its consequences in front of it. */}
          <p className="flex items-center gap-1.5 text-sm font-medium">
            <AlertTriangle aria-hidden className="size-4 text-[var(--color-status-warn)]" />
            {t('warning.title')}
          </p>
          <p className="mt-0.5 text-2xs text-[var(--color-fg-muted)]">{t('warning.body')}</p>
          <p className="mt-0.5 text-2xs text-[var(--color-fg-default)]">
            {count === 0
              ? t('warning.nothingSelected')
              : t('warning.estimate', { count: Math.round(seconds) })}
          </p>
        </div>
        <Button
          disabled={count === 0}
          loading={running}
          loadingLabel={t('action.busy')}
          onClick={onRun}
        >
          {hasResults ? t('action.rerun') : t('action.start')}
        </Button>
      </CardBody>
    </Card>
  );
}

/** Forwards ref and props: it is the `ContextMenuTrigger asChild` target. */
const Result = forwardRef<
  HTMLDivElement,
  {
    readonly result: BenchmarkResultDto;
    readonly locale: string;
  } & Omit<React.ComponentPropsWithoutRef<'div'>, 'children' | 'className' | 'title'>
>(function Result({ result, locale, ...rest }, ref) {
  const { t } = useTranslation(BENCHMARKS_NS);
  const trusted = isTrustworthy(result);
  const reasons = distrustReasons(result);
  const median = medianOf(result);
  const spread = spreadOf(result);

  // The unit always comes from the backend: it is MB/s for bandwidth, ns for
  // latency and ops/s for the CPU tests, and a hardcoded unit here would
  // silently mislabel two thirds of the results. There is deliberately no
  // dimensionless composite "score" — that number would be ours, not the
  // machine's.
  //
  // Not rounded to an integer first: the backend reports 148.92 ns, and
  // truncating that to 149 discards the only digits that distinguish two
  // memory configurations. `formatCount` keeps up to three decimals and adds
  // the grouping that makes a nine-digit ops/s figure legible.
  const value = (n: number): string => `${formatCount(n, locale)} ${result.unit}`;
  // Already a percentage from the backend, so it is passed straight through.
  // Scaling it here would be the classic double conversion.
  const percent = (value: number): string => formatPercent(value, locale);

  return (
    <Card ref={ref} regionLabel={t(`name.${result.id}`)} {...rest}>
      <CardHeader
        actions={
          trusted ? (
            <Badge tone="ok" icon={<CheckCircle2 />}>
              {t('trust.ok')}
            </Badge>
          ) : (
            <Badge tone="warn" icon={<AlertTriangle />}>
              {t('trust.bad')}
            </Badge>
          )
        }
      >
        <CardTitle level={3}>{t(`name.${result.id}`)}</CardTitle>
      </CardHeader>

      <CardBody className="flex flex-col gap-2.5">
        <div>
          <p
            className={
              trusted
                ? 'tnum font-mono text-2xl'
                : // Struck through would overstate it — the figure is real, it
                  // just describes a machine in a bad state. Muted plus the
                  // warning badge above says "provisional" without hiding it.
                  'tnum font-mono text-2xl text-[var(--color-fg-muted)]'
            }
          >
            {value(median)}
          </p>
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t('results.headline', { count: result.runs.length })}
          </p>
        </div>

        {!trusted && reasons.length > 0 && (
          <div className="rounded-md border border-[var(--color-status-warn)]/40 bg-[var(--color-status-warn)]/10 p-2.5">
            <p className="text-2xs font-medium">{t('trust.badIntro')}</p>
            <ul className="mt-1 list-disc space-y-0.5 pl-4 text-2xs">
              {reasons.map((reason) => (
                <li key={reason}>
                  {t(`trust.reason.${reason}`, {
                    percent:
                      reason === 'variability'
                        ? percent(result.variability ?? 0)
                        : percent(result.conditions.backgroundLoad),
                  })}
                </li>
              ))}
            </ul>
          </div>
        )}

        {/* The individual passes, not just the summary. Two runs of 100 and
            900 have the same median as 490 and 510, and only one of those is
            a measurement. */}
        <div>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('results.runs')}</p>
          <ul className="mt-0.5 flex flex-wrap gap-x-3 gap-y-0.5">
            {result.runs.map((run, index) => (
              <li key={index} className="tnum font-mono text-2xs">
                {value(run)}
              </li>
            ))}
          </ul>
          {spread !== null && (
            <p className="mt-0.5 text-2xs text-[var(--color-fg-subtle)]">
              {t('results.spread', { best: value(spread.max), worst: value(spread.min) })}
            </p>
          )}
          <p className="text-2xs text-[var(--color-fg-subtle)]">
            {result.variability === null
              ? t('results.variabilitySingle')
              : t('results.variability', { percent: percent(result.variability) })}
          </p>
        </div>

        <Conditions result={result} locale={locale} />
      </CardBody>
    </Card>
  );
});

function Conditions({
  result,
  locale,
}: {
  readonly result: BenchmarkResultDto;
  readonly locale: string;
}) {
  const { t } = useTranslation(BENCHMARKS_NS);
  const { conditions } = result;

  // Separated by spacing, not a rule: a divider here sat a few pixels above
  // the card's own bottom edge and read as a doubled border.
  return (
    <div className="pt-1">
      <p className="text-2xs font-medium">{t('trust.conditions')}</p>
      <ul className="mt-0.5 space-y-0.5 text-2xs text-[var(--color-fg-muted)]">
        <li>
          {conditions.powerPlan === null
            ? t('trust.powerPlanUnknown')
            : t('trust.powerPlan', { plan: conditions.powerPlan })}
        </li>
        <li>
          {t('trust.background', {
            percent: formatPercent(conditions.backgroundLoad, locale),
          })}
        </li>
        <li>
          {/* Never substituted with a plausible number: "0 °C" would be read
              as a measurement, and no reading at all is a different fact. */}
          {conditions.ambientStartTemp === null || conditions.ambientEndTemp === null
            ? t('trust.temperatureUnknown')
            : t('trust.temperature', {
                start: formatTemperature(conditions.ambientStartTemp, locale),
                end: formatTemperature(conditions.ambientEndTemp, locale),
              })}
        </li>
        <li>
          {t('results.duration', {
            seconds: formatCount(Math.round(result.durationMs / 1000), locale),
          })}
        </li>
      </ul>
    </div>
  );
}

function BenchmarksSkeleton(): React.JSX.Element {
  return (
    <div aria-busy="true" className="space-y-3">
      <Skeleton className="h-7 w-56" />
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        {[0, 1, 2, 3].map((index) => (
          <Skeleton key={index} className="h-36" />
        ))}
      </div>
      <Skeleton className="h-20 w-full" />
    </div>
  );
}
