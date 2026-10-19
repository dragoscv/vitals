/**
 * "Why is this drive growing?" and the weekly clean-up, under the wizard.
 *
 * # Honest about what is known
 *
 * A rate is shown only when the readings cover enough of its window; with no
 * earlier analysis the panel says this is the first one rather than showing
 * an empty list that reads as "nothing changed". Unmeasured sizes never
 * enter a delta (the backend leaves them out).
 *
 * # The weekly clean-up says exactly what it will do
 *
 * Turning it on lists the choices it will run — the drive's saved choices
 * that are safe unattended — and says so plainly when that is none of them.
 */

import { CalendarClock, TrendingUp } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Badge, Select, Switch, formatBytes, formatCount } from '@vitals/ui';

import { useSettings } from '../../../settings/store';
import { STORAGE_NS } from '../strings';
import {
  changeDelta,
  sparkPath,
  topChanges,
  type DriveChange,
  type DriveGrowth,
  type DriveHistory,
  type DriveHistorySource,
  type DriveRun,
} from './history';
import { isKnownKind, mountKey } from './model';
import {
  DEFAULT_MINUTE,
  DEFAULT_WEEKDAY,
  minuteText,
  scheduledKinds,
  schedulableKinds,
} from './schedule';

function useStorageT() {
  return useTranslation(STORAGE_NS).t;
}
type T = ReturnType<typeof useStorageT>;

export interface DriveInsightsProps {
  readonly mount: string;
  readonly locale: string;
  readonly source: DriveHistorySource;
  /** Changes when the history should be read again (after a run). */
  readonly reloadKey?: string | number;
}

type Loaded =
  | { readonly state: 'loading' }
  | { readonly state: 'failed'; readonly message: string }
  | { readonly state: 'ready'; readonly history: DriveHistory };

const SPARK_W = 240;
const SPARK_H = 40;

/** Unix seconds. Outside the component: it is only called from a handler. */
function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

/** The name of an item kind, from the wizard's names or the history's own. */
function kindName(change: { kind: string; label: string | null }, t: T): string {
  const base =
    change.kind === 'dockerDisk' || change.kind === 'wslDisk'
      ? t(`insight.growth.item.${change.kind}`)
      : isKnownKind(change.kind)
        ? t(`drive.item.${change.kind}.name`)
        : t('drive.item.other.name');
  return change.label === null ? base : `${base} (${change.label})`;
}

function dateText(ts: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, { day: 'numeric', month: 'short' }).format(
    new Date(ts * 1000),
  );
}

function dateTimeText(ts: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    day: 'numeric',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
  }).format(new Date(ts * 1000));
}

/** Monday = 0, named in the user's language without a string per day. */
function weekdayName(weekday: number, locale: string): string {
  // 2024-01-01 was a Monday.
  return new Intl.DateTimeFormat(locale, { weekday: 'long', timeZone: 'UTC' }).format(
    new Date(Date.UTC(2024, 0, 1 + weekday)),
  );
}

function Rate({
  label,
  value,
  locale,
  t,
}: {
  label: string;
  value: number | null;
  locale: string;
  t: T;
}) {
  const text =
    value === null
      ? t('insight.growth.notEnough')
      : value === 0
        ? t('insight.growth.steady')
        : value > 0
          ? t('insight.growth.perDay', { size: formatBytes(value, locale) })
          : t('insight.growth.perDayLess', { size: formatBytes(-value, locale) });
  return (
    <div className="flex flex-col">
      <dt className="text-2xs text-[var(--color-fg-muted)]">{label}</dt>
      <dd
        className={`tnum text-sm ${value === null ? 'text-[var(--color-fg-muted)]' : 'font-medium'}`}
      >
        {text}
      </dd>
    </div>
  );
}

function Growth({ growth, locale, t }: { growth: DriveGrowth; locale: string; t: T }) {
  return (
    <dl className="grid grid-cols-3 gap-2">
      <Rate label={t('insight.growth.day')} value={growth.day} locale={locale} t={t} />
      <Rate label={t('insight.growth.week')} value={growth.week} locale={locale} t={t} />
      <Rate label={t('insight.growth.month')} value={growth.month} locale={locale} t={t} />
    </dl>
  );
}

function Changes({ history, locale, t }: { history: DriveHistory; locale: string; t: T }) {
  if (history.previousAnalysis === null) {
    return (
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('insight.growth.firstAnalysis')}</p>
    );
  }
  const date = dateText(history.previousAnalysis, locale);
  const top = topChanges(history.changes);
  if (top.length === 0) {
    return (
      <p className="text-2xs text-[var(--color-fg-muted)]">
        {t('insight.growth.noChanges', { date })}
      </p>
    );
  }
  return (
    <div className="flex flex-col gap-1">
      <p className="text-2xs font-medium">{t('insight.growth.changesTitle', { date })}</p>
      <ul className="flex flex-col gap-0.5">
        {top.map((change: DriveChange) => {
          const delta = changeDelta(change);
          return (
            <li
              key={`${change.kind}:${change.label ?? ''}`}
              className="flex items-baseline justify-between gap-2 text-2xs"
            >
              <span className="break-all">{kindName(change, t)}</span>
              <span
                className={`tnum shrink-0 ${delta > 0 ? 'text-[var(--color-warn-text)]' : 'text-[var(--color-fg-muted)]'}`}
              >
                {delta > 0
                  ? t('insight.growth.grew', { size: formatBytes(delta, locale) })
                  : t('insight.growth.shrank', { size: formatBytes(-delta, locale) })}
              </span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function Spark({ history, mount, t }: { history: DriveHistory; mount: string; t: T }) {
  const path = sparkPath(history.series, SPARK_W, SPARK_H);
  if (path === null) return null;
  return (
    <svg
      role="img"
      aria-label={t('insight.growth.chartLabel', { mount })}
      viewBox={`0 0 ${SPARK_W} ${SPARK_H}`}
      preserveAspectRatio="none"
      className="h-10 w-full text-[var(--color-accent)]"
    >
      <polyline
        points={path}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

function RunLine({ run, locale, t }: { run: DriveRun; locale: string; t: T }) {
  const outcome =
    run.items === 0
      ? t('insight.weekly.nothing')
      : run.freed === null
        ? t('insight.weekly.freedUnknown')
        : t('insight.weekly.freed', { size: formatBytes(run.freed, locale) });
  return (
    <li className="flex flex-col gap-0.5 text-2xs">
      <span className="flex flex-wrap items-center gap-1">
        <Badge tone={run.scheduled ? 'accent' : 'neutral'}>
          {run.scheduled ? t('insight.weekly.scheduled') : t('insight.weekly.byUser')}
        </Badge>
        <span className="tnum">{dateTimeText(run.ts, locale)}</span>
        <span>· {outcome}</span>
        {run.failed > 0 && (
          <span className="text-[var(--color-danger-text)]">
            · {t('insight.weekly.failures', { n: formatCount(run.failed, locale) })}
          </span>
        )}
      </span>
      {run.errors.map((error) => (
        <span key={error} className="pl-2 break-all text-[var(--color-fg-muted)]">
          {error}
        </span>
      ))}
    </li>
  );
}

function Weekly({
  history,
  mount,
  locale,
  t,
}: {
  history: DriveHistory;
  mount: string;
  locale: string;
  t: T;
}) {
  const key = mountKey(mount);
  const schedules = useSettings((s) => s.settings.driveCleanupSchedules);
  const profile = useSettings((s) => s.settings.driveCleanupProfiles[key]);
  const patch = useSettings((s) => s.patch);
  const schedule = schedules[history.volume];
  const on = schedule !== undefined;

  const setSchedule = (next: { weekday: number; minute: number } | null) => {
    const others = Object.fromEntries(
      Object.entries(schedules).filter(([volume]) => volume !== history.volume),
    );
    patch({
      driveCleanupSchedules:
        next === null
          ? others
          : {
              ...others,
              [history.volume]: {
                mount: key,
                weekday: next.weekday,
                minute: next.minute,
                enabledAt: schedule?.enabledAt ?? nowSeconds(),
              },
            },
    });
  };

  const kinds = scheduledKinds(profile ?? schedulableKinds);
  const names = kinds.map((kind) =>
    isKnownKind(kind) ? t(`drive.item.${kind}.name`) : t('drive.item.other.name'),
  );
  const days = Array.from({ length: 7 }, (_, d) => ({
    value: String(d),
    label: weekdayName(d, locale),
  }));
  const times = Array.from({ length: 48 }, (_, h) => ({
    value: String(h * 30),
    label: minuteText(h * 30),
  }));

  return (
    <div className="flex flex-col gap-2">
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('insight.weekly.body')}</p>
      <Switch
        checked={on}
        label={t('insight.weekly.toggle', { mount: key })}
        onCheckedChange={(next) => {
          setSchedule(next ? { weekday: DEFAULT_WEEKDAY, minute: DEFAULT_MINUTE } : null);
        }}
      />
      {on && (
        <div className="flex flex-col gap-2 pl-6">
          <div className="flex flex-wrap gap-2">
            <Select
              label={t('insight.weekly.day')}
              value={String(schedule.weekday)}
              options={days}
              onValueChange={(value) => {
                setSchedule({ weekday: Number(value), minute: schedule.minute });
              }}
            />
            <Select
              label={t('insight.weekly.time')}
              value={String(schedule.minute)}
              options={
                times.some((o) => o.value === String(schedule.minute))
                  ? times
                  : [
                      ...times,
                      { value: String(schedule.minute), label: minuteText(schedule.minute) },
                    ]
              }
              onValueChange={(value) => {
                setSchedule({ weekday: schedule.weekday, minute: Number(value) });
              }}
            />
          </div>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('insight.weekly.when')}</p>
          {profile === undefined && (
            <p className="text-2xs text-[var(--color-fg-muted)]">
              {t('insight.weekly.fromDefaults')}
            </p>
          )}
          <p className="text-2xs" aria-live="polite">
            {names.length === 0
              ? t('insight.weekly.includesNone')
              : t('insight.weekly.includes', { list: names.join(', ') })}
          </p>
        </div>
      )}
      <div className="flex flex-col gap-1">
        <p className="text-2xs font-medium">{t('insight.weekly.recent')}</p>
        {history.runs.length === 0 ? (
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('insight.weekly.none')}</p>
        ) : (
          <ul className="flex flex-col gap-1">
            {history.runs.map((run) => (
              <RunLine key={`${run.ts}:${String(run.scheduled)}`} run={run} locale={locale} t={t} />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

export function DriveInsights({ mount, locale, source, reloadKey }: DriveInsightsProps) {
  const t = useStorageT();
  const [loaded, setLoaded] = useState<Loaded>({ state: 'loading' });

  useEffect(() => {
    let live = true;
    source
      .load(mount)
      .then((history) => {
        if (live) setLoaded({ state: 'ready', history });
      })
      .catch((cause: unknown) => {
        if (live) {
          setLoaded({
            state: 'failed',
            message: cause instanceof Error ? cause.message : String(cause),
          });
        }
      });
    return () => {
      live = false;
    };
  }, [mount, source, reloadKey]);

  if (loaded.state === 'loading') return null;
  if (loaded.state === 'failed') {
    return (
      <p className="text-2xs text-[var(--color-fg-muted)]">
        {t('insight.growth.unavailable', { message: loaded.message })}
      </p>
    );
  }
  const history = loaded.history;
  return (
    <div className="flex flex-col gap-2">
      <details className="rounded-md border border-[var(--color-border-subtle)] p-2">
        <summary className="flex cursor-pointer items-center gap-2 font-medium">
          <TrendingUp aria-hidden className="size-4" />
          {t('insight.growth.title')}
        </summary>
        <div className="mt-2 flex flex-col gap-2">
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('insight.growth.how')}</p>
          <Growth growth={history.growth} locale={locale} t={t} />
          <Spark history={history} mount={mount} t={t} />
          <Changes history={history} locale={locale} t={t} />
        </div>
      </details>
      <details className="rounded-md border border-[var(--color-border-subtle)] p-2">
        <summary className="flex cursor-pointer items-center gap-2 font-medium">
          <CalendarClock aria-hidden className="size-4" />
          {t('insight.weekly.title')}
        </summary>
        <div className="mt-2">
          <Weekly history={history} mount={mount} locale={locale} t={t} />
        </div>
      </details>
    </div>
  );
}
