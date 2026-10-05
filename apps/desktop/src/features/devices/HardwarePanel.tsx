/**
 * Hardware: what this computer is made of — processor, memory modules,
 * graphics, drives, motherboard.
 *
 * Read once per visit (WMI, hundreds of milliseconds cold), with a refresh
 * button; none of it changes while the machine runs except drive temperature,
 * which the Sensors tab reports live. Every unknown value is "Not available"
 * with the reason in its tooltip, never a zero.
 */

import { CircuitBoard, Copy, Cpu, HardDrive, MemoryStick, Monitor, RefreshCw } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  Card,
  CardBody,
  CardHeader,
  CardTitle,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
  EmptyState,
  Skeleton,
  formatBytes,
  formatCount,
  formatTemperature,
} from '@vitals/ui';

import { errorMessage } from '../../lib/commandError';
import { useRowMenu } from '../../lib/useRowMenu';
import { Field } from './Field';
import { installedMemory, type HardwareApi, type HardwareInfo } from './hardwareApi';
import { DEVICES_NS } from './strings';

export function HardwarePanel({ api }: { readonly api: HardwareApi }): React.JSX.Element {
  const { t } = useTranslation(DEVICES_NS);
  const [info, setInfo] = useState<HardwareInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);

  const read = useCallback(
    () =>
      api.hardware().then(
        (next) => {
          setInfo(next);
          setError(null);
          setBusy(false);
        },
        (cause: unknown) => {
          setError(errorMessage(cause));
          setBusy(false);
        },
      ),
    [api],
  );

  useEffect(() => {
    // State is set only in the promise callbacks, after the external read;
    // `busy` starts true, so the first load needs no synchronous set.
    void read();
  }, [read]);

  const refresh = () => {
    setBusy(true);
    void read();
  };

  if (info === null && busy) {
    return (
      <div className="flex flex-col gap-3" aria-busy="true">
        {[0, 1, 2].map((i) => (
          <Skeleton key={i} className="h-32 w-full" />
        ))}
      </div>
    );
  }

  if (info === null) {
    return <EmptyState title={t('hardware.failed')} description={error ?? ''} />;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <p className="text-2xs text-[var(--color-fg-subtle)]">
          {t('hardware.cost', { ms: Math.round(info.elapsedMs) })}
        </p>
        <Button
          variant="ghost"
          size="sm"
          loading={busy}
          loadingLabel={t('hardware.reading')}
          onClick={refresh}
        >
          <RefreshCw aria-hidden className="size-4" />
          {t('refresh')}
        </Button>
      </div>
      {error !== null && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {t('stale', { message: error })}
        </p>
      )}
      <div className="hardware-grid pane-scroll">
        <CpuCard info={info} />
        <MemoryCard info={info} onRefresh={refresh} />
        <GpuCard info={info} />
        <DrivesCard info={info} onRefresh={refresh} />
        <BoardCard info={info} />
      </div>
    </div>
  );
}

/** The menu shared by a memory module and a drive: copy what it says, re-read. */
function CopyMenu({
  label,
  details,
  onRefresh,
}: {
  readonly label: string;
  readonly details: string;
  readonly onRefresh: () => void;
}) {
  const { t } = useTranslation(DEVICES_NS);
  return (
    <ContextMenuContent>
      <ContextMenuLabel>{label}</ContextMenuLabel>
      <ContextMenuItem
        onSelect={() => {
          void globalThis.navigator?.clipboard?.writeText(label);
        }}
      >
        <Copy className="size-4" aria-hidden="true" />
        {t('menu.copyName')}
      </ContextMenuItem>
      <ContextMenuItem
        onSelect={() => {
          void globalThis.navigator?.clipboard?.writeText(details);
        }}
      >
        <Copy className="size-4" aria-hidden="true" />
        {t('menu.copyDetails')}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={onRefresh}>
        <RefreshCw className="size-4" aria-hidden="true" />
        {t('refresh')}
      </ContextMenuItem>
    </ContextMenuContent>
  );
}

function Section({
  title,
  icon,
  children,
}: {
  readonly title: string;
  readonly icon: React.ReactNode;
  readonly children: React.ReactNode;
}) {
  return (
    <Card regionLabel={title}>
      <CardHeader>
        <CardTitle level={3}>
          <span className="inline-flex items-center gap-1.5">
            {icon}
            {title}
          </span>
        </CardTitle>
      </CardHeader>
      <CardBody className="flex flex-col gap-3">{children}</CardBody>
    </Card>
  );
}

function CpuCard({ info }: { readonly info: HardwareInfo }) {
  const { t, i18n } = useTranslation(DEVICES_NS);
  const n = (v: number | null) => (v === null ? null : formatCount(v, i18n.language));
  return (
    <Section title={t('hardware.cpu')} icon={<Cpu aria-hidden className="size-4" />}>
      {info.cpus.length === 0 && <p className="text-2xs">{t('unavailable')}</p>}
      {info.cpus.map((cpu, i) => (
        <div key={`${cpu.name}-${i}`}>
          <p className="mb-2 text-sm font-medium">{cpu.name}</p>
          <dl className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            <Field label={t('hardware.cores')} value={n(cpu.cores)} />
            <Field label={t('hardware.threads')} value={n(cpu.threads)} />
            <Field
              label={t('hardware.baseClock')}
              value={cpu.baseClockMhz === null ? null : `${n(cpu.baseClockMhz) ?? ''} MHz`}
            />
            <Field
              label={t('hardware.l2')}
              value={
                cpu.l2CacheKb === null ? null : formatBytes(cpu.l2CacheKb * 1024, i18n.language, 0)
              }
            />
            <Field
              label={t('hardware.l3')}
              value={
                cpu.l3CacheKb === null ? null : formatBytes(cpu.l3CacheKb * 1024, i18n.language, 0)
              }
            />
            <Field label={t('hardware.socket')} value={cpu.socket} />
            <Field
              label={t('hardware.virtualization')}
              value={
                cpu.virtualization === null
                  ? null
                  : t(cpu.virtualization ? 'hardware.on' : 'hardware.off')
              }
            />
          </dl>
        </div>
      ))}
    </Section>
  );
}

function MemoryCard({
  info,
  onRefresh,
}: {
  readonly info: HardwareInfo;
  readonly onRefresh: () => void;
}) {
  const { t, i18n } = useTranslation(DEVICES_NS);
  const menu = useRowMenu();
  const mem = info.memory;
  const installed = installedMemory(mem);
  const bytes = (v: number | null) => (v === null ? null : formatBytes(v, i18n.language, 1));
  return (
    <Section title={t('hardware.memory')} icon={<MemoryStick aria-hidden className="size-4" />}>
      <dl className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Field label={t('hardware.installed')} value={bytes(installed)} />
        <Field label={t('hardware.usable')} value={bytes(mem.usableBytes)} />
        <Field
          label={t('hardware.slots')}
          value={
            mem.slots === null
              ? null
              : t('hardware.slotsUsed', { used: mem.modules.length, total: mem.slots })
          }
        />
        <Field label={t('hardware.maxCapacity')} value={bytes(mem.maxCapacityBytes)} />
      </dl>
      {mem.modules.length > 0 && (
        <table className="w-full text-left text-2xs" onKeyDown={menu.onKeyDown}>
          <thead>
            <tr className="text-[var(--color-fg-muted)]">
              <th scope="col" className="py-1 pr-2 font-normal">
                {t('hardware.slot')}
              </th>
              <th scope="col" className="py-1 pr-2 font-normal">
                {t('hardware.size')}
              </th>
              <th scope="col" className="py-1 pr-2 font-normal">
                {t('hardware.type')}
              </th>
              <th scope="col" className="py-1 pr-2 font-normal">
                {t('hardware.speed')}
              </th>
              <th scope="col" className="py-1 pr-2 font-normal">
                {t('hardware.part')}
              </th>
            </tr>
          </thead>
          <tbody>
            {mem.modules.map((m, i) => {
              const id = m.slot ?? String(i);
              const kind = [m.kind, m.formFactor].filter((x) => x !== null).join(' ') || '—';
              const part =
                [m.manufacturer, m.partNumber].filter((x) => x !== null).join(' ') || '—';
              const speed =
                m.configuredSpeedMts !== null
                  ? `${formatCount(m.configuredSpeedMts, i18n.language)} MT/s`
                  : '—';
              const label = m.slot ?? `${t('hardware.slot')} ${String(i + 1)}`;
              const details = [label, bytes(m.capacityBytes) ?? '—', kind, speed, part].join('\t');
              return (
                <ContextMenu key={id} {...menu.rootProps(id)}>
                  <ContextMenuTrigger asChild>
                    <tr
                      className="border-t border-[var(--color-border-subtle)]"
                      onContextMenu={menu.onContextMenu}
                    >
                      <td className="py-1 pr-2">{m.slot ?? '—'}</td>
                      <td className="py-1 pr-2 tabular-nums">{bytes(m.capacityBytes) ?? '—'}</td>
                      <td className="py-1 pr-2">{kind}</td>
                      <td className="py-1 pr-2 tabular-nums">
                        {speed}
                        {m.speedMts !== null &&
                          m.configuredSpeedMts !== null &&
                          m.speedMts !== m.configuredSpeedMts && (
                            <span className="ml-1 text-[var(--color-fg-subtle)]">
                              {t('hardware.rated', {
                                speed: formatCount(m.speedMts, i18n.language),
                              })}
                            </span>
                          )}
                      </td>
                      <td className="py-1 pr-2">{part}</td>
                    </tr>
                  </ContextMenuTrigger>
                  <CopyMenu label={label} details={details} onRefresh={onRefresh} />
                </ContextMenu>
              );
            })}
          </tbody>
        </table>
      )}
    </Section>
  );
}

function GpuCard({ info }: { readonly info: HardwareInfo }) {
  const { t, i18n } = useTranslation(DEVICES_NS);
  return (
    <Section title={t('hardware.gpu')} icon={<Monitor aria-hidden className="size-4" />}>
      {info.gpus.length === 0 && <p className="text-2xs">{t('unavailable')}</p>}
      {info.gpus.map((gpu, i) => (
        <div key={`${gpu.name}-${i}`}>
          <p className="mb-2 text-sm font-medium">{gpu.name}</p>
          <dl className="grid grid-cols-2 gap-3 sm:grid-cols-3">
            <Field
              label={t('hardware.videoMemory')}
              value={
                gpu.videoMemoryBytes === null
                  ? null
                  : formatBytes(gpu.videoMemoryBytes, i18n.language, 1)
              }
            />
            <Field
              label={t('hardware.display')}
              value={
                gpu.resolution === null
                  ? null
                  : gpu.refreshHz === null
                    ? gpu.resolution
                    : `${gpu.resolution} @ ${gpu.refreshHz} Hz`
              }
            />
            <Field
              label={t('hardware.driver')}
              value={
                gpu.driverVersion === null
                  ? null
                  : gpu.driverDate === null
                    ? gpu.driverVersion
                    : `${gpu.driverVersion} (${gpu.driverDate})`
              }
            />
          </dl>
        </div>
      ))}
    </Section>
  );
}

function DrivesCard({
  info,
  onRefresh,
}: {
  readonly info: HardwareInfo;
  readonly onRefresh: () => void;
}) {
  const { t, i18n } = useTranslation(DEVICES_NS);
  const menu = useRowMenu();
  return (
    <Section title={t('hardware.drives')} icon={<HardDrive aria-hidden className="size-4" />}>
      {info.drives.length === 0 && <p className="text-2xs">{t('unavailable')}</p>}
      <ul className="flex flex-col gap-3" onKeyDown={menu.onKeyDown}>
        {info.drives.map((d) => {
          const size = d.sizeBytes === null ? null : formatBytes(d.sizeBytes, i18n.language, 1);
          const temperature =
            d.temperatureCelsius === null
              ? null
              : formatTemperature(d.temperatureCelsius, i18n.language);
          const details = [
            d.model,
            t(`media.${d.media}`),
            d.bus ?? '—',
            size ?? '—',
            temperature ?? '—',
            d.firmware ?? '—',
          ].join('\t');
          return (
            <ContextMenu key={d.index} {...menu.rootProps(String(d.index))}>
              <ContextMenuTrigger asChild>
                <li className="min-w-0" onContextMenu={menu.onContextMenu}>
                  <div className="mb-1 flex flex-wrap items-center gap-1.5">
                    <span className="text-sm font-medium">{d.model}</span>
                    <Badge tone="neutral">{t(`media.${d.media}`)}</Badge>
                    {d.bus !== null && <Badge tone="neutral">{d.bus}</Badge>}
                    {d.health !== null && (
                      <Badge tone={d.health === 'Healthy' ? 'ok' : 'warn'}>
                        {t(`health.${d.health}`, { defaultValue: d.health })}
                      </Badge>
                    )}
                  </div>
                  <dl className="grid grid-cols-2 gap-3 sm:grid-cols-4">
                    <Field label={t('hardware.size')} value={size} />
                    <Field label={t('hardware.temperature')} value={temperature} />
                    <Field label={t('hardware.firmware')} value={d.firmware} />
                    <Field
                      label={t('hardware.serial')}
                      value={d.serialTail === null ? null : `…${d.serialTail}`}
                    />
                  </dl>
                </li>
              </ContextMenuTrigger>
              <CopyMenu label={d.model} details={details} onRefresh={onRefresh} />
            </ContextMenu>
          );
        })}
      </ul>
    </Section>
  );
}

function BoardCard({ info }: { readonly info: HardwareInfo }) {
  const { t } = useTranslation(DEVICES_NS);
  const b = info.board;
  const join = (...parts: readonly (string | null)[]) => {
    const text = parts.filter((p) => p !== null).join(' ');
    return text === '' ? null : text;
  };
  return (
    <Section title={t('hardware.board')} icon={<CircuitBoard aria-hidden className="size-4" />}>
      <dl className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        <Field
          label={t('hardware.motherboard')}
          value={join(b.manufacturer, b.product, b.version)}
        />
        <Field
          label={t('hardware.bios')}
          value={join(b.biosVendor, b.biosVersion, b.biosDate === null ? null : `(${b.biosDate})`)}
        />
        <Field label={t('hardware.system')} value={join(b.systemManufacturer, b.systemModel)} />
      </dl>
    </Section>
  );
}
