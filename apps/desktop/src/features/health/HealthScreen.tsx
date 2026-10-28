/**
 * The Health page: the owner's own readings from vmui, at a glance.
 *
 * Every reading is optional. A phone that publishes no blood pressure is the
 * common case, not an error, so a missing value is an em dash in its usual
 * place — never a 0, and never a hidden row that makes the cards jump when
 * the reading comes back.
 */

import { HeartPulse, RefreshCw, ShieldAlert, WifiOff } from 'lucide-react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  Card,
  CardBody,
  CardHeader,
  CardTitle,
  EmptyState,
  Skeleton,
  StatList,
  type Stat,
} from '@vitals/ui';

import { openSettingsAt } from '../../shell/navigation';
import type { HealthApi, HealthErrorKind, HealthReadings, MeHealth, Nutrition } from './api';
import { HEALTH_NS } from './strings';
import { useHealth } from './useHealth';

export interface HealthScreenProps {
  /** Injectable so the screen renders in tests without a Tauri host. */
  readonly api?: HealthApi;
  readonly pollMs?: number;
}

type T = (key: string, options?: Record<string, unknown>) => string;

function num(value: number | null | undefined, locale: string, digits = 0): string | null {
  if (value === null || value === undefined || !Number.isFinite(value)) return null;
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(value);
}

/** `unit.*` with `{{value}}` filled, or `null` when the reading is absent. */
function withUnit(
  t: T,
  unit: string,
  value: number | null | undefined,
  locale: string,
  digits = 0,
): string | null {
  const v = num(value, locale, digits);
  return v === null ? null : t(`unit.${unit}`, { value: v });
}

/** "eaten / target" — only when the eaten side is known. */
function ofTarget(
  t: T,
  unit: string,
  value: number | null | undefined,
  target: number | null | undefined,
  locale: string,
): string | null {
  const v = num(value, locale);
  if (v === null) return null;
  const goal = num(target, locale) ?? '—';
  return t(`unit.${unit}`, { value: t('unit.ofTarget', { value: v, target: goal }) });
}

/** A tiny inline line of the last 24 h of heart rate. Decorative beside the number. */
function Sparkline({
  points,
  label,
}: {
  readonly points: readonly { readonly t: number | null; readonly v: number | null }[];
  readonly label: string;
}) {
  const usable = points.filter(
    (p): p is { t: number; v: number } =>
      p.t !== null && p.v !== null && Number.isFinite(p.t) && Number.isFinite(p.v),
  );
  if (usable.length < 2) return null;
  const width = 160;
  const height = 32;
  const ts = usable.map((p) => p.t);
  const vs = usable.map((p) => p.v);
  const t0 = Math.min(...ts);
  const t1 = Math.max(...ts);
  const v0 = Math.min(...vs);
  const v1 = Math.max(...vs);
  const x = (t: number): number => (t1 === t0 ? 0 : ((t - t0) / (t1 - t0)) * width);
  const y = (v: number): number =>
    v1 === v0 ? height / 2 : height - ((v - v0) / (v1 - v0)) * height;
  const d = usable
    .map((p, i) => `${i === 0 ? 'M' : 'L'}${x(p.t).toFixed(1)},${y(p.v).toFixed(1)}`)
    .join(' ');
  return (
    <svg
      role="img"
      aria-label={label}
      viewBox={`0 0 ${width} ${height}`}
      className="h-8 w-40 text-[var(--color-accent)]"
      preserveAspectRatio="none"
    >
      <path
        d={d}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

function sleepText(t: T, minutes: number | null | undefined): string | null {
  if (minutes === null || minutes === undefined || !Number.isFinite(minutes)) return null;
  const total = Math.round(minutes);
  return t('unit.hoursMinutes', { hours: Math.floor(total / 60), minutes: total % 60 });
}

function activityStats(t: T, h: HealthReadings | null, locale: string): Stat[] {
  return [
    { key: 'steps', label: t('metric.steps'), value: num(h?.steps, locale) },
    {
      key: 'distance',
      label: t('metric.distance'),
      value: withUnit(t, 'km', h?.distanceKm, locale, 1),
    },
    { key: 'floors', label: t('metric.floors'), value: num(h?.floors, locale) },
    {
      key: 'active',
      label: t('metric.activeKcal'),
      value: withUnit(t, 'kcal', h?.activeKcal, locale),
    },
    {
      key: 'total',
      label: t('metric.totalKcal'),
      value: withUnit(t, 'kcal', h?.totalKcal, locale),
    },
  ];
}

function heartStats(t: T, h: HealthReadings | null, locale: string): Stat[] {
  const bp = h?.bloodPressure;
  const sys = num(bp?.sys, locale);
  const dia = num(bp?.dia, locale);
  return [
    { key: 'hr', label: t('metric.heartRate'), value: withUnit(t, 'bpm', h?.heartRate, locale) },
    { key: 'rest', label: t('metric.restingHr'), value: withUnit(t, 'bpm', h?.restingHr, locale) },
    { key: 'hrv', label: t('metric.hrv'), value: withUnit(t, 'ms', h?.hrv, locale) },
    { key: 'spo2', label: t('metric.spo2'), value: withUnit(t, 'pct', h?.spo2, locale) },
    {
      key: 'resp',
      label: t('metric.respiratoryRate'),
      value: withUnit(t, 'perMin', h?.respiratoryRate, locale),
    },
    {
      key: 'bp',
      label: t('metric.bloodPressure'),
      value: sys !== null && dia !== null ? t('unit.mmHg', { sys, dia }) : null,
    },
  ];
}

function bodyStats(t: T, h: HealthReadings | null, locale: string): Stat[] {
  const estimate = (
    <Badge tone="neutral" className="text-2xs">
      {t('estimate')}
    </Badge>
  );
  const bmi = num(h?.body?.bmi, locale, 1);
  // The scale's own body-fat reading first; the height/weight estimate only
  // when the scale gave none, and then labelled as such.
  const fatMeasured = withUnit(t, 'pct', h?.bodyFat, locale, 1);
  const fatEstimated = withUnit(t, 'pct', h?.body?.fatPct, locale, 1);
  const bmrMeasured = withUnit(t, 'kcalPerDay', h?.bmr, locale);
  const bmrEstimated = withUnit(t, 'kcalPerDay', h?.body?.bmrKcal, locale);
  return [
    { key: 'weight', label: t('metric.weight'), value: withUnit(t, 'kg', h?.weightKg, locale, 1) },
    {
      key: 'fat',
      label: t('metric.bodyFat'),
      value: fatMeasured ?? fatEstimated,
      extra: fatMeasured === null && fatEstimated !== null ? estimate : undefined,
    },
    { key: 'bmi', label: t('metric.bmi'), value: bmi, extra: bmi !== null ? estimate : undefined },
    {
      key: 'bmr',
      label: t('metric.bmr'),
      value: bmrMeasured ?? bmrEstimated,
      extra: bmrMeasured === null && bmrEstimated !== null ? estimate : undefined,
    },
    { key: 'vo2', label: t('metric.vo2max'), value: withUnit(t, 'vo2', h?.vo2max, locale) },
  ];
}

function nutritionStats(t: T, n: Nutrition | null, locale: string): Stat[] {
  return [
    {
      key: 'eaten',
      label: t('metric.eaten'),
      value: withUnit(t, 'kcal', n?.today?.calories, locale),
    },
    {
      key: 'target',
      label: t('metric.target'),
      value: withUnit(t, 'kcal', n?.targets?.calories, locale),
    },
    {
      key: 'remaining',
      label: t('metric.remaining'),
      value: withUnit(t, 'kcal', n?.remaining?.calories, locale),
    },
    {
      key: 'protein',
      label: t('metric.protein'),
      value: ofTarget(t, 'grams', n?.today?.protein, n?.targets?.protein, locale),
    },
    {
      key: 'carbs',
      label: t('metric.carbs'),
      value: ofTarget(t, 'grams', n?.today?.carbs, n?.targets?.carbs, locale),
    },
    {
      key: 'fats',
      label: t('metric.fats'),
      value: ofTarget(t, 'grams', n?.today?.fats, n?.targets?.fats, locale),
    },
    {
      key: 'water',
      label: t('metric.water'),
      value: ofTarget(t, 'ml', n?.water?.ml, n?.water?.targetMl, locale),
    },
  ];
}

function HealthCards({
  data,
  t,
  locale,
}: {
  readonly data: MeHealth;
  readonly t: T;
  readonly locale: string;
}) {
  const h = data.health;
  const n = data.nutrition;
  const unavailable = t('unavailable');
  const card = (key: string, title: string, stats: Stat[], extra?: React.ReactNode) => (
    <Card key={key} regionLabel={title}>
      <CardHeader>
        <CardTitle level={3}>{title}</CardTitle>
      </CardHeader>
      <CardBody className="space-y-3">
        <StatList stats={stats} columns={2} unavailableLabel={unavailable} />
        {extra}
      </CardBody>
    </Card>
  );
  return (
    <div className="grid gap-3 lg:grid-cols-2 2xl:grid-cols-3">
      {card('activity', t('card.activity'), activityStats(t, h, locale))}
      {card(
        'heart',
        t('card.heart'),
        heartStats(t, h, locale),
        <Sparkline points={h?.hrHistory ?? []} label={t('metric.hrHistory')} />,
      )}
      {card('sleep', t('card.sleep'), [
        { key: 'sleep', label: t('metric.sleep'), value: sleepText(t, h?.sleepMin) },
      ])}
      {card('body', t('card.body'), bodyStats(t, h, locale))}
      {card('nutrition', t('card.nutrition'), nutritionStats(t, n, locale))}
    </div>
  );
}

const ERROR_ICON: Record<HealthErrorKind, React.ReactNode> = {
  signedOut: <ShieldAlert />,
  notOwner: <ShieldAlert />,
  unreachable: <WifiOff />,
  notConfigured: <HeartPulse />,
};

export function HealthScreen({ api, pollMs }: HealthScreenProps = {}) {
  const { t: raw, i18n } = useTranslation(HEALTH_NS);
  const t = raw as unknown as T;
  const locale = i18n.language;
  const { state, refresh } = useHealth(api, pollMs);

  const updated = useMemo(() => {
    const at = state.data?.at;
    if (at === null || at === undefined || !Number.isFinite(at)) return null;
    return new Intl.DateTimeFormat(locale, { dateStyle: 'medium', timeStyle: 'short' }).format(
      new Date(at),
    );
  }, [state.data, locale]);

  const pairAgain = (
    <Button
      variant="primary"
      onClick={() => {
        openSettingsAt('health');
      }}
    >
      {t(state.error === 'notConfigured' ? 'openSettings' : 'pairAgain')}
    </Button>
  );

  return (
    <div className="screen">
      <header className="screen-header">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{t('title')}</h2>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('notAdvice')}</p>
        </div>
        <div className="flex items-center gap-2">
          {state.status === 'error' && state.data !== null && (
            <Badge tone="warn">{t('stale')}</Badge>
          )}
          {updated !== null && (
            <span className="text-2xs text-[var(--color-fg-muted)]">
              {t('updated', { time: updated })}
            </span>
          )}
          <Button variant="ghost" size="sm" onClick={refresh}>
            <RefreshCw aria-hidden className="size-4" />
            {t('retry')}
          </Button>
        </div>
      </header>

      {state.status === 'loading' && (
        <div aria-busy="true" aria-label={t('loading')} className="grid gap-3 lg:grid-cols-2">
          {[0, 1, 2, 3].map((i) => (
            <Skeleton key={i} className="h-36" />
          ))}
        </div>
      )}

      {state.status === 'error' && state.data === null && (
        <EmptyState
          icon={ERROR_ICON[state.error]}
          title={t(`error.${state.error}.title`)}
          description={t(`error.${state.error}.body`)}
          {...((state.error === 'signedOut' || state.error === 'notConfigured') && {
            action: pairAgain,
          })}
        />
      )}

      {state.status === 'error' && state.data !== null && (
        <p role="status" className="text-2xs text-[var(--color-warn-text)]">
          {t('error.unreachable.body')}
        </p>
      )}

      {state.data !== null && <HealthCards data={state.data} t={t} locale={locale} />}
    </div>
  );
}
