/**
 * Devices & sensors.
 *
 * The inventory view: what this machine exposes, what it measures, and — the
 * substantial part — what it cannot measure and why. The Thermals tab under
 * Performance charts the zones over time; this screen explains them.
 *
 * # The empty state is the design, not a fallback
 *
 * On a desktop, unelevated, this backend measures nothing: ACPI refuses the
 * query, there is no battery, and core temperature, fan RPM, package power
 * and rail voltages are all behind ring 0. Every other tool responds to that
 * by rendering an empty table, which is indistinguishable from a broken
 * application. Here the gap list is first-class content, each entry naming
 * the concrete mechanism that would close it, so the absence is legible.
 *
 * # No value is ever a plausible stand-in
 *
 * Where a figure could not be obtained the screen prints "Not available" with
 * a reason. Not 0 °C, not 100% health, not "0 minutes remaining" — each of
 * those is a fabrication the user would act on.
 */

import { Cpu, Lock, PlugZap, RefreshCw, ShieldAlert } from 'lucide-react';
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
  formatCount,
  formatPercent,
  formatTemperature,
  formatUptime,
  formatWatts,
} from '@vitals/ui';

import {
  aggregateIsRedundant,
  batteryHealthPercent,
  groupGapsByCapability,
  hasMeasurements,
  partitionGaps,
  sortReadings,
  thermalsNeedElevation,
  type Battery,
  type DriverGap,
  type SensorReading,
  type SensorsSnapshot,
} from './model';
import { DEVICES_NS } from './strings';
import { NO_HOST, useSensors, type SensorsReader } from './useSensors';

export interface DevicesScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly reader?: SensorsReader;
}

export function DevicesScreen({ reader }: DevicesScreenProps): React.JSX.Element {
  const { t, i18n } = useTranslation(DEVICES_NS);
  const state = useSensors(reader);
  const snapshot = state.snapshot;

  const updatedLabel = useMemo(() => {
    if (state.updatedAt === null) return null;
    // Formatted from the recorded timestamp rather than `Date.now()`: reading
    // the clock during render is impure and the lint rule that bans it is
    // right — two renders of the same data would disagree.
    return new Intl.DateTimeFormat(i18n.language, { timeStyle: 'medium' }).format(state.updatedAt);
  }, [state.updatedAt, i18n.language]);

  if (state.pending && snapshot === null) return <DevicesSkeleton />;

  if (state.error === NO_HOST) {
    return (
      <EmptyState icon={<ShieldAlert />} title={t('noHost.title')} description={t('noHost.body')} />
    );
  }

  return (
    <div className="flex flex-col gap-4">
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

      {state.error !== null && state.error !== NO_HOST && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {t('stale', { message: state.error })}
        </p>
      )}

      {snapshot !== null && (
        <>
          <p className="text-2xs text-[var(--color-fg-subtle)]">
            {updatedLabel !== null && `${t('updated', { time: updatedLabel })} · `}
            {t('cadence', { seconds: Math.round(snapshot.cadenceMs / 1000) })}{' '}
            {t('cost', { ms: Math.round(snapshot.elapsedMs) })}
          </p>

          <PowerSection snapshot={snapshot} />
          <BatterySection snapshot={snapshot} />
          <ThermalSection snapshot={snapshot} />
          <ReadingsSection snapshot={snapshot} />
          <GapsSection gaps={snapshot.gaps} />
        </>
      )}
    </div>
  );
}

/** A label with either a value or an explicit, reasoned absence. */
function Field({
  label,
  value,
  hint,
}: {
  readonly label: string;
  /** `null` renders as "Not available", never as a zero or a dash alone. */
  readonly value: string | null;
  readonly hint?: string;
}) {
  const { t } = useTranslation(DEVICES_NS);

  return (
    <div className="min-w-0">
      <dt className="text-2xs text-[var(--color-fg-muted)]">{label}</dt>
      <dd className="truncate text-sm">
        {value ?? (
          // `title` rather than Tooltip: Tooltip throws outside the shell's
          // TooltipProvider, which would crash this screen anywhere else it
          // is rendered — including in tests.
          <span className="text-[var(--color-fg-subtle)]" title={t('unavailableHint')}>
            {t('unavailable')}
          </span>
        )}
        {hint !== undefined && value !== null && (
          <span className="ml-1 text-2xs text-[var(--color-fg-subtle)]">{hint}</span>
        )}
      </dd>
    </div>
  );
}

function PowerSection({ snapshot }: { readonly snapshot: SensorsSnapshot }) {
  const { t, i18n } = useTranslation(DEVICES_NS);
  const power = snapshot.power;

  return (
    <Card regionLabel={t('power.title')}>
      <CardHeader>
        <CardTitle level={3}>
          <span className="inline-flex items-center gap-1.5">
            <PlugZap aria-hidden className="size-4" />
            {t('power.title')}
          </span>
        </CardTitle>
      </CardHeader>
      <CardBody>
        <dl className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <Field
            label={t('power.line')}
            value={t(`line.${power.line}`)}
            {...(power.line === 'unknown' ? { hint: t('lineUnknownHint') } : {})}
          />
          <Field label={t('power.mode')} value={t(`mode.${power.mode}`)} />
          <Field label={t('power.scheme')} value={power.schemeGuid} />
          <Field
            label={t('power.saver')}
            value={power.powerSaver ? t('power.saverOn') : t('power.saverOff')}
          />
          {power.hasBattery && (
            <>
              <Field
                label={t('battery.charge')}
                value={
                  power.batteryPercent === null
                    ? null
                    : formatPercent(power.batteryPercent, i18n.language, 0)
                }
              />
              {/* `null` when the OS estimator has not settled. Rendering that
                  as "0m" on a full pack is the exact fabrication this screen
                  exists to avoid. */}
              <Field
                label={t('battery.remaining')}
                value={
                  power.secondsRemaining === null ? null : formatUptime(power.secondsRemaining)
                }
              />
            </>
          )}
        </dl>
        {!power.hasBattery && (
          <p className="mt-2 text-2xs text-[var(--color-fg-muted)]">{t('power.noBattery')}</p>
        )}
      </CardBody>
    </Card>
  );
}

function BatterySection({ snapshot }: { readonly snapshot: SensorsSnapshot }) {
  const { t } = useTranslation(DEVICES_NS);

  if (snapshot.batteries.length === 0 && snapshot.aggregateBattery === null) return null;

  return (
    <Card regionLabel={t('battery.title')}>
      <CardHeader>
        <CardTitle level={3}>{t('battery.title')}</CardTitle>
      </CardHeader>
      <CardBody className="flex flex-col gap-4">
        {snapshot.batteries.map((pack) => (
          <BatteryPack key={pack.devicePath} pack={pack} />
        ))}

        {/* Suppressed against a single pack: the two views would say the same
            thing, and printing both is how a user concludes there are two. */}
        {snapshot.aggregateBattery !== null && !aggregateIsRedundant(snapshot) && (
          <div>
            <p className="mb-1 text-2xs text-[var(--color-fg-muted)]">{t('battery.aggregate')}</p>
            <dl className="grid gap-3 sm:grid-cols-3">
              <Field
                label={t('battery.remaining')}
                value={
                  snapshot.aggregateBattery.estimatedSeconds === null
                    ? null
                    : formatUptime(snapshot.aggregateBattery.estimatedSeconds)
                }
              />
              <Field
                label={t('battery.fullCapacity')}
                value={
                  snapshot.aggregateBattery.maxCapacity === null
                    ? null
                    : `${snapshot.aggregateBattery.maxCapacity} mWh`
                }
              />
              <Field
                label={t('battery.charge')}
                value={
                  snapshot.aggregateBattery.remainingCapacity === null
                    ? null
                    : `${snapshot.aggregateBattery.remainingCapacity} mWh`
                }
              />
            </dl>
          </div>
        )}
      </CardBody>
    </Card>
  );
}

function BatteryPack({ pack }: { readonly pack: Battery }) {
  const { t, i18n } = useTranslation(DEVICES_NS);
  const health = batteryHealthPercent(pack);

  // A relative-capacity gauge reports unitless counts, so the mWh suffix
  // would be a fabricated unit. The ratios stay valid because it cancels.
  const capacity = (value: number | null): string | null =>
    value === null
      ? null
      : pack.capacityIsRelative
        ? formatCount(value, i18n.language)
        : `${value} mWh`;

  return (
    <div>
      <div className="mb-1 flex flex-wrap items-center gap-1.5">
        <span className="text-sm font-medium">{pack.chemistry}</span>
        <Badge tone={pack.state === 'discharging' ? 'warn' : 'neutral'}>
          {t(`chargeState.${pack.state}`)}
        </Badge>
        {pack.isShortTerm && (
          <Badge tone="info" title={t('battery.upsHint')}>
            {t('battery.ups')}
          </Badge>
        )}
      </div>
      <dl className="grid gap-3 sm:grid-cols-3 xl:grid-cols-4">
        <Field
          label={t('battery.charge')}
          value={pack.charge === null ? null : formatPercent(pack.charge, i18n.language, 0)}
        />
        <Field
          label={t('battery.health')}
          value={health === null ? null : formatPercent(health, i18n.language, 0)}
        />
        <Field
          label={t('battery.rate')}
          value={pack.rateWatts === null ? null : formatWatts(pack.rateWatts, i18n.language)}
        />
        <Field
          label={t('battery.voltage')}
          value={pack.voltage === null ? null : `${pack.voltage} V`}
        />
        {/* `null` when the gauge does not count. "0 cycles" on a five-year-old
            laptop is a lie the user believes. */}
        <Field
          label={t('battery.cycles')}
          value={pack.cycleCount === null ? null : formatCount(pack.cycleCount, i18n.language)}
        />
        <Field label={t('battery.designCapacity')} value={capacity(pack.designCapacityMwh)} />
        <Field label={t('battery.fullCapacity')} value={capacity(pack.fullChargeCapacityMwh)} />
        <Field
          label={t('battery.remaining')}
          value={pack.secondsToEmpty === null ? null : formatUptime(pack.secondsToEmpty)}
        />
      </dl>
      {pack.capacityIsRelative && (
        <p className="mt-1 text-2xs text-[var(--color-fg-muted)]">
          {t('battery.relativeCapacity')}
        </p>
      )}
    </div>
  );
}

function ThermalSection({ snapshot }: { readonly snapshot: SensorsSnapshot }) {
  const { t, i18n } = useTranslation(DEVICES_NS);
  const needsElevation = thermalsNeedElevation(snapshot.thermalAvailability);

  return (
    <Card regionLabel={t('thermal.title')}>
      <CardHeader
        actions={
          needsElevation ? (
            <Badge tone="warn" icon={<Lock aria-hidden />}>
              {t('reason.needsElevation')}
            </Badge>
          ) : undefined
        }
      >
        <CardTitle level={3}>{t('thermal.title')}</CardTitle>
      </CardHeader>
      <CardBody>
        {snapshot.zones.length === 0 ? (
          // Three of the four availability states produce an empty list and
          // only one is fixed by elevating. Saying which is the whole point:
          // a UAC prompt that changes nothing teaches the user to ignore
          // prompts, and "your board has no sensors" is simply false when the
          // truth is "we were not allowed to ask".
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t(`availability.${snapshot.thermalAvailability}`)}
          </p>
        ) : (
          <>
            <dl className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
              {snapshot.zones.map((zone) => (
                <div key={zone.instance} className="min-w-0">
                  <dt className="truncate font-mono text-2xs text-[var(--color-fg-muted)]">
                    {zone.instance}
                  </dt>
                  <dd className="text-sm">
                    {formatTemperature(zone.celsius, i18n.language)}
                    {zone.criticalCelsius !== null && (
                      <span className="ml-1.5 text-2xs text-[var(--color-fg-subtle)]">
                        {t('thermal.critical')}{' '}
                        {formatTemperature(zone.criticalCelsius, i18n.language)}
                      </span>
                    )}
                    {zone.activeCooling !== null && (
                      <span className="ml-1.5 text-2xs text-[var(--color-fg-subtle)]">
                        {zone.activeCooling ? t('thermal.active') : t('thermal.passive')}
                      </span>
                    )}
                  </dd>
                </div>
              ))}
            </dl>
            <p className="mt-2 text-2xs text-[var(--color-fg-muted)]">{t('thermal.notZone')}</p>
          </>
        )}
      </CardBody>
    </Card>
  );
}

function ReadingsSection({ snapshot }: { readonly snapshot: SensorsSnapshot }) {
  const { t } = useTranslation(DEVICES_NS);
  const rows = useMemo(() => sortReadings(snapshot.readings), [snapshot.readings]);

  return (
    <Card regionLabel={t('sensors.title')}>
      <CardHeader>
        <CardTitle level={3}>
          <span className="inline-flex items-center gap-1.5">
            <Cpu aria-hidden className="size-4" />
            {t('sensors.title')}
          </span>
        </CardTitle>
      </CardHeader>
      {/* The table draws its own padding, so the body sheds it — otherwise the
          header rule and the first row do not line up. */}
      <CardBody className={hasMeasurements(snapshot) ? 'p-0' : 'p-3'}>
        {rows.length === 0 ? (
          <EmptyState title={t('sensors.none')} description={t('sensors.noneBody')} />
        ) : (
          <table className="w-full text-left">
            <thead>
              <tr className="text-2xs text-[var(--color-fg-muted)]">
                <th scope="col" className="px-2.5 py-1.5 font-normal">
                  {t('sensors.label')}
                </th>
                <th scope="col" className="px-2.5 py-1.5 font-normal">
                  {t('sensors.value')}
                </th>
                <th scope="col" className="px-2.5 py-1.5 font-normal">
                  {t('sensors.source')}
                </th>
                <th scope="col" className="px-2.5 py-1.5 font-normal">
                  {t('sensors.quality')}
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((reading) => (
                <ReadingRow key={reading.key} reading={reading} />
              ))}
            </tbody>
          </table>
        )}
      </CardBody>
    </Card>
  );
}

function ReadingRow({ reading }: { readonly reading: SensorReading }) {
  const { t, i18n } = useTranslation(DEVICES_NS);

  return (
    <tr className="border-t border-[var(--color-border-subtle)]">
      <td className="px-2.5 py-1.5 text-sm">{reading.label}</td>
      <td className="px-2.5 py-1.5 text-sm tabular-nums">
        {formatReading(reading, i18n.language)}
      </td>
      <td className="px-2.5 py-1.5 text-2xs">{t(`source.${reading.source}`)}</td>
      <td className="px-2.5 py-1.5 text-2xs">
        {/* Provenance is not decoration. A derived figure and a measured one
            look identical once rendered, and users make hardware decisions on
            the difference — battery health is a ratio of two firmware
            constants, not a measurement of anything. */}
        <Badge
          tone={reading.quality === 'measured' ? 'ok' : 'neutral'}
          title={t(`qualityHint.${reading.quality}`)}
        >
          {t(`quality.${reading.quality}`)}
        </Badge>
      </td>
    </tr>
  );
}

/**
 * Renders a reading in its own unit.
 *
 * The unit travels on the reading rather than being inferred from the label,
 * so a chipset at 42 °C cannot become a chipset at 42 W somewhere in the
 * render path.
 */
export function formatReading(reading: SensorReading, locale?: string): string {
  switch (reading.unit) {
    case 'temperature':
      return formatTemperature(reading.value, locale);
    case 'power':
      return formatWatts(reading.value, locale);
    case 'charge':
      return formatPercent(reading.value, locale, 0);
    case 'voltage':
      // No `formatVolts` exists in @vitals/ui, and adding one would mean
      // editing a shared package another agent is working in. Two decimals is
      // the meaningful precision for a rail voltage.
      return `${new Intl.NumberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(reading.value)} V`;
    case 'fanSpeed':
      return `${formatCount(reading.value, locale)} RPM`;
    default: {
      const exhaustive: never = reading.unit;
      return exhaustive;
    }
  }
}

function GapsSection({ gaps }: { readonly gaps: readonly DriverGap[] }) {
  const { t } = useTranslation(DEVICES_NS);
  const { actionable, permanent } = useMemo(() => partitionGaps(gaps), [gaps]);

  return (
    <Card regionLabel={t('gaps.title')}>
      <CardHeader>
        <CardTitle level={3}>{t('gaps.title')}</CardTitle>
      </CardHeader>
      <CardBody className="flex flex-col gap-4">
        <p className="text-2xs text-[var(--color-fg-muted)]">{t('gaps.body')}</p>

        {gaps.length === 0 && (
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('gaps.empty')}</p>
        )}

        {actionable.length > 0 && <GapGroup heading={t('gaps.actionable')} gaps={actionable} />}
        {permanent.length > 0 && <GapGroup heading={t('gaps.permanent')} gaps={permanent} />}
      </CardBody>
    </Card>
  );
}

function GapGroup({
  heading,
  gaps,
}: {
  readonly heading: string;
  readonly gaps: readonly DriverGap[];
}) {
  const { t } = useTranslation(DEVICES_NS);
  const groups = useMemo(() => groupGapsByCapability(gaps), [gaps]);

  return (
    <section>
      <h4 className="mb-1.5 text-2xs font-semibold text-[var(--color-fg-muted)]">{heading}</h4>
      <ul className="flex flex-col gap-2.5">
        {groups.map(([capability, items]) =>
          items.map((gap) => (
            <li key={gap.label} className="min-w-0">
              <div className="flex flex-wrap items-center gap-1.5">
                <span className="text-sm font-medium">{gap.label}</span>
                <Badge tone="neutral">{t(`capability.${capability}`)}</Badge>
                <Badge tone={gap.actionable ? 'info' : 'neutral'}>
                  {t(`reason.${gap.reason}`)}
                </Badge>
              </div>
              <p className="mt-0.5 text-2xs text-[var(--color-fg-muted)]">
                <span className="text-[var(--color-fg-subtle)]">{t('gaps.requirement')}: </span>
                {gap.requirement}
              </p>
            </li>
          )),
        )}
      </ul>
    </section>
  );
}

function DevicesSkeleton(): React.JSX.Element {
  return (
    <div aria-busy="true" className="space-y-3">
      <Skeleton className="h-7 w-56" />
      {[0, 1, 2].map((index) => (
        <Skeleton key={index} className="h-32" />
      ))}
    </div>
  );
}
