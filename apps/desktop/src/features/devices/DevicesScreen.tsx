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

import { Cpu, Lock, PlugZap, RefreshCw, ShieldAlert, ShieldCheck } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
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
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  cn,
  formatCount,
  formatPercent,
  formatTemperature,
  formatUptime,
  formatWatts,
} from '@vitals/ui';

import { ExportButton } from '../../components/ExportButton';
import { errorMessage, isCommandError } from '../../lib/commandError';
import type { ExportColumn } from '../../lib/export';
import { hasTauriHost } from '../../shell/host';
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
import { DeviceTreePanel } from './DeviceTreePanel';
import { HardwarePanel } from './HardwarePanel';
import { tauriHardwareApi, type HardwareApi } from './hardwareApi';
import {
  serviceAction,
  tauriSensorsService,
  type SensorsServiceApi,
  type SensorsServiceStatus,
} from './sensorsService';
import { NO_HOST, useSensors, type SensorsReader } from './useSensors';

export interface DevicesScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly reader?: SensorsReader;
  /** Injectable for the same reason; `null` hides the service panel. */
  readonly service?: SensorsServiceApi | null;
  /** Hardware inventory and device list; `null` hides those tabs. */
  readonly hardware?: HardwareApi | null;
}

export function DevicesScreen({
  reader,
  service,
  hardware,
}: DevicesScreenProps): React.JSX.Element {
  const { t, i18n } = useTranslation(DEVICES_NS);
  const state = useSensors(reader);
  const snapshot = state.snapshot;
  // An injected reader without an injected service is a test of something
  // else; it must not reach for the real IPC module.
  const serviceApi =
    service !== undefined
      ? service
      : reader === undefined && hasTauriHost()
        ? tauriSensorsService
        : null;
  const hardwareApi =
    hardware !== undefined
      ? hardware
      : reader === undefined && hasTauriHost()
        ? tauriHardwareApi
        : null;
  // The tab's data is read the first time it is opened, not on screen
  // mount: the device list and inventory are hundreds of milliseconds of
  // WMI and SetupAPI that the Sensors tab does not need.
  const [tab, setTab] = useState('sensors');

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

      {state.error !== null && state.error !== NO_HOST && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {t('stale', { message: state.error })}
        </p>
      )}

      {snapshot !== null && (
        <Tabs
          value={tab}
          onValueChange={setTab}
          activationMode="manual"
          className="flex min-h-0 flex-1 flex-col gap-2"
        >
          <TabsList aria-label={t('tabs.label')}>
            <TabsTrigger value="sensors">{t('tabs.sensors')}</TabsTrigger>
            {hardwareApi !== null && (
              <>
                <TabsTrigger value="hardware">{t('tabs.hardware')}</TabsTrigger>
                <TabsTrigger value="devices">{t('tabs.devices')}</TabsTrigger>
              </>
            )}
          </TabsList>
          <TabsContent value="sensors" className="flex min-h-0 flex-1 flex-col gap-2">
            <p className="text-2xs text-[var(--color-fg-subtle)]">
              {updatedLabel !== null && `${t('updated', { time: updatedLabel })} · `}
              {t('cadence', { seconds: Math.round(snapshot.cadenceMs / 1000) })}{' '}
              {t('cost', { ms: Math.round(snapshot.elapsedMs) })}
            </p>

            {/*
             * The short, fixed facts (power, battery, thermal zones) are a row
             * of separate cards at their own height: they are a handful of
             * values and must never scroll (S12-29 — stacked in one scrolling
             * column, the zones were cut off below the power card). The two
             * open-ended lists share the rest of the height side by side and
             * scroll inside themselves, so their titles and the table header
             * stay put. A window too short for that stacks everything and
             * the body scrolls as one (`.screen-body`).
             */}
            <div className="devices-body screen-body">
              <div className="devices-facts">
                <PowerSection snapshot={snapshot} />
                <ThermalSection snapshot={snapshot} />
                {snapshot.batteries.length > 0 && <BatterySection snapshot={snapshot} />}
              </div>
              <ReadingsSection snapshot={snapshot} />
              <GapsSection
                gaps={snapshot.gaps}
                service={serviceApi}
                onServiceChanged={state.refresh}
              />
            </div>
          </TabsContent>
          {hardwareApi !== null && (
            <>
              <TabsContent value="hardware" className="flex min-h-0 flex-1 flex-col">
                {tab === 'hardware' && <HardwarePanel api={hardwareApi} />}
              </TabsContent>
              <TabsContent value="devices" className="flex min-h-0 flex-1 flex-col">
                {tab === 'devices' && <DeviceTreePanel api={hardwareApi} />}
              </TabsContent>
            </>
          )}
        </Tabs>
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

  // The raw value and its unit as separate columns. A "42 °C" string cannot be
  // charted; `42` with `temperature` beside it can, and the unit column is
  // what stops a chipset at 42 °C being read as a rail at 42 V.
  const exportColumns = useMemo(
    (): readonly ExportColumn<SensorReading>[] => [
      { id: 'key', header: t('sensors.key'), value: (reading) => reading.key },
      { id: 'label', header: t('sensors.label'), value: (reading) => reading.label },
      { id: 'value', header: t('sensors.value'), value: (reading) => reading.value },
      { id: 'unit', header: t('sensors.unit'), value: (reading) => reading.unit },
      { id: 'source', header: t('sensors.source'), value: (reading) => reading.source },
      { id: 'quality', header: t('sensors.quality'), value: (reading) => reading.quality },
    ],
    [t],
  );

  return (
    <Card regionLabel={t('sensors.title')} className="pane">
      <CardHeader>
        <CardTitle level={3}>
          <span className="inline-flex items-center gap-1.5">
            <Cpu aria-hidden className="size-4" />
            {t('sensors.title')}
          </span>
        </CardTitle>
        <ExportButton name="sensors" rows={rows} columns={exportColumns} />
      </CardHeader>
      {/* The table draws its own padding, so the body sheds it — otherwise the
          header rule and the first row do not line up. */}
      <CardBody className={cn('pane-scroll', hasMeasurements(snapshot) ? 'p-0' : 'p-3')}>
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
    case 'percent':
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

function GapsSection({
  gaps,
  service,
  onServiceChanged,
}: {
  readonly gaps: readonly DriverGap[];
  readonly service: SensorsServiceApi | null;
  readonly onServiceChanged: () => void;
}) {
  const { t } = useTranslation(DEVICES_NS);
  const { actionable, permanent } = useMemo(() => partitionGaps(gaps), [gaps]);

  return (
    <Card regionLabel={t('gaps.title')} className="pane">
      <CardHeader>
        <CardTitle level={3}>{t('gaps.title')}</CardTitle>
      </CardHeader>
      <CardBody className="pane-scroll flex flex-col gap-4">
        {service !== null && <SensorsServicePanel api={service} onChanged={onServiceChanged} />}

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

/**
 * The one gap Vitals can close itself: CPU temperature and package power,
 * through the optional sensors service (ADR-0031).
 *
 * Everything the user is agreeing to is on the panel before the UAC prompt:
 * a signed third-party driver, a service running as SYSTEM, and — when the
 * driver is missing — a download. A prompt that appears without that context
 * is the kind people learn to click through.
 */
function SensorsServicePanel({
  api,
  onChanged,
}: {
  readonly api: SensorsServiceApi;
  readonly onChanged: () => void;
}) {
  const { t } = useTranslation(DEVICES_NS);
  const [status, setStatus] = useState<SensorsServiceStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<{ tone: 'error' | 'info'; text: string } | null>(null);

  const load = useCallback(
    () =>
      api.status().then(setStatus, (cause: unknown) => {
        setMessage({ tone: 'error', text: errorMessage(cause) });
      }),
    [api],
  );

  useEffect(() => {
    // State is set in the promise callbacks, after the external read — the
    // subscription shape the effect rule allows.
    void load();
  }, [load]);

  const action = serviceAction(status);

  const run = async (install: boolean) => {
    setBusy(true);
    setMessage(null);
    try {
      await api.setup(install);
      setMessage({ tone: 'info', text: t(install ? 'service.installed' : 'service.removed') });
    } catch (cause: unknown) {
      // A dismissed UAC prompt is the user's answer, not a failure.
      const declined = isCommandError(cause) && cause.kind === 'refused';
      setMessage({
        tone: declined ? 'info' : 'error',
        text: declined
          ? errorMessage(cause)
          : t('service.failed', { message: errorMessage(cause) }),
      });
    } finally {
      setBusy(false);
      await load();
      onChanged();
    }
  };

  if (status === null && message === null) return null;

  return (
    <section
      aria-labelledby="sensors-service-title"
      className="rounded-md border border-[var(--color-border-subtle)] p-3"
    >
      <div className="flex flex-wrap items-center gap-1.5">
        <ShieldCheck aria-hidden className="size-4" />
        <h4 id="sensors-service-title" className="text-sm font-medium">
          {t('service.title')}
        </h4>
        {status !== null && (
          <Badge tone={status.running ? 'ok' : status.installed ? 'warn' : 'neutral'}>
            {t(
              status.running
                ? 'service.state.running'
                : status.installed
                  ? 'service.state.notReading'
                  : 'service.state.notInstalled',
            )}
          </Badge>
        )}
      </div>
      <p className="mt-1 text-2xs text-[var(--color-fg-muted)]">{t('service.body')}</p>
      {status !== null && !status.installed && !status.pawnioInstalled && (
        <p className="mt-1 text-2xs text-[var(--color-fg-muted)]">{t('service.download')}</p>
      )}
      {status?.installed === true && !status.running && status.error !== null && (
        <p className="mt-1 text-2xs text-[var(--color-status-warn)]">
          {t('service.notReading', { message: status.error })}
        </p>
      )}
      {action === 'unavailable' && (
        <p className="mt-1 text-2xs text-[var(--color-fg-subtle)]">{t('service.noHelper')}</p>
      )}
      {(action === 'install' || action === 'remove') && (
        <div className="mt-2">
          <Button
            size="sm"
            variant={action === 'install' ? 'primary' : 'secondary'}
            loading={busy}
            loadingLabel={t('service.working')}
            onClick={() => void run(action === 'install')}
          >
            {t(action === 'install' ? 'service.install' : 'service.remove')}
          </Button>
        </div>
      )}
      {message !== null && (
        <p
          role={message.tone === 'error' ? 'alert' : 'status'}
          className={cn(
            'mt-2 text-2xs',
            message.tone === 'error'
              ? 'text-[var(--color-status-danger)]'
              : 'text-[var(--color-fg-muted)]',
          )}
        >
          {message.text}
        </p>
      )}
    </section>
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
