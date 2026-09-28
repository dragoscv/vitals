/**
 * Every device Windows Device Manager lists, grouped by class the way it
 * groups them, with the driver behind each and any problem it reports.
 *
 * Classes start collapsed — a desktop has 400-odd nodes, and the question a
 * person usually brings here is "is anything wrong", which the problem badge
 * on each heading answers without opening anything. A class with a problem
 * opens by itself. Search filters across every class at once.
 */

import { AlertTriangle, ChevronRight, CircleOff, RefreshCw, Unplug } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Badge, Button, Checkbox, EmptyState, SearchInput, Skeleton, cn } from '@vitals/ui';

import { errorMessage } from '../../lib/commandError';
import {
  filterTree,
  problemCount,
  type DeviceClass,
  type DeviceInfo,
  type DeviceTree,
  type HardwareApi,
} from './hardwareApi';
import { DEVICES_NS } from './strings';

export function DeviceTreePanel({ api }: { readonly api: HardwareApi }): React.JSX.Element {
  const { t } = useTranslation(DEVICES_NS);
  const [tree, setTree] = useState<DeviceTree | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(true);
  const [includeHidden, setIncludeHidden] = useState(false);
  const [query, setQuery] = useState('');

  const read = useCallback(
    (hidden: boolean) =>
      api.devices(hidden).then(
        (next) => {
          setTree(next);
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
    // State is set only in the promise callbacks, after the external read.
    void read(includeHidden);
  }, [read, includeHidden]);

  const reload = (hidden: boolean) => {
    setBusy(true);
    void read(hidden);
  };

  const classes = useMemo(() => filterTree(tree?.classes ?? [], query), [tree, query]);
  const total = useMemo(
    () => (tree?.classes ?? []).reduce((n, c) => n + c.devices.length, 0),
    [tree],
  );
  const problems = useMemo(() => problemCount(tree?.classes ?? []), [tree]);

  if (tree === null && busy) {
    return (
      <div className="flex flex-col gap-2" aria-busy="true">
        {Array.from({ length: 8 }, (_, i) => (
          <Skeleton key={i} className="h-8 w-full" />
        ))}
      </div>
    );
  }

  if (tree === null) {
    return <EmptyState title={t('tree.failed')} description={error ?? ''} />;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <SearchInput
          className="w-64 max-w-full"
          value={query}
          onValueChange={setQuery}
          clearLabel={t('tree.clear')}
          placeholder={t('tree.search')}
          aria-label={t('tree.search')}
          resultsAnnouncement={t('tree.matches', {
            count: classes.reduce((n, c) => n + c.devices.length, 0),
          })}
        />
        <Checkbox
          checked={includeHidden}
          onCheckedChange={(v) => {
            setBusy(true);
            setIncludeHidden(v === true);
          }}
          label={t('tree.showHidden')}
        />
        <span className="text-2xs text-[var(--color-fg-muted)]" aria-live="polite">
          {t('tree.summary', { count: total, classes: tree.classes.length })}
          {problems > 0 && ` · ${t('tree.problems', { count: problems })}`}
        </span>
        <Button
          className="ml-auto"
          variant="ghost"
          size="sm"
          loading={busy}
          loadingLabel={t('tree.reading')}
          onClick={() => reload(includeHidden)}
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
      <div className="list-scroll">
        {classes.length === 0 ? (
          <p className="p-3 text-2xs text-[var(--color-fg-muted)]">{t('tree.noMatch')}</p>
        ) : (
          <ul aria-label={t('tree.title')}>
            {classes.map((cls) => (
              <ClassGroup key={cls.guid} cls={cls} forceOpen={query.trim() !== ''} />
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

function ClassGroup({
  cls,
  forceOpen,
}: {
  readonly cls: DeviceClass;
  readonly forceOpen: boolean;
}) {
  const { t } = useTranslation(DEVICES_NS);
  const problems = cls.devices.filter((d) => d.status === 'problem').length;
  const [open, setOpen] = useState(problems > 0);
  const expanded = open || forceOpen;
  const id = `class-${cls.guid}`;

  return (
    <li>
      <button
        type="button"
        aria-expanded={expanded}
        aria-controls={id}
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-1.5 rounded-md px-2 py-1.5 text-left text-sm hover:bg-[var(--color-bg-inset)] focus-visible:outline-2 focus-visible:outline-[var(--color-accent)]"
      >
        <ChevronRight
          aria-hidden
          className={cn('size-4 shrink-0 transition-transform', expanded && 'rotate-90')}
        />
        <span className="font-medium">{cls.description}</span>
        <span className="text-2xs text-[var(--color-fg-subtle)]">{cls.devices.length}</span>
        {problems > 0 && (
          <Badge tone="warn" icon={<AlertTriangle aria-hidden />}>
            {t('tree.problems', { count: problems })}
          </Badge>
        )}
      </button>
      {expanded && (
        <ul id={id} className="mb-1 ml-6 border-l border-[var(--color-border-subtle)]">
          {cls.devices.map((device) => (
            <DeviceRow key={device.instanceId} device={device} />
          ))}
        </ul>
      )}
    </li>
  );
}

function DeviceRow({ device }: { readonly device: DeviceInfo }) {
  const { t } = useTranslation(DEVICES_NS);
  const driver = [device.driverProvider, device.driverVersion, device.driverDate]
    .filter((x) => x !== null)
    .join(' · ');

  return (
    <li
      className={cn(
        'px-3 py-1.5',
        (device.status === 'notPresent' || device.hidden) && 'opacity-70',
      )}
      title={device.instanceId}
    >
      <div className="flex flex-wrap items-center gap-1.5">
        <StatusIcon device={device} />
        <span className="text-sm">{device.name}</span>
        {device.status === 'disabled' && <Badge tone="neutral">{t('tree.disabled')}</Badge>}
        {device.status === 'notPresent' && <Badge tone="neutral">{t('tree.notConnected')}</Badge>}
        {device.enumerator !== null && (
          <span className="font-mono text-2xs text-[var(--color-fg-subtle)]">
            {device.enumerator}
          </span>
        )}
      </div>
      {device.status === 'problem' && (
        <p className="text-2xs text-[var(--color-status-warn)]">
          {t('tree.problem', {
            code: device.problemCode ?? 0,
            message: device.problem ?? '',
          })}
        </p>
      )}
      {(driver !== '' || device.location !== null) && (
        <p className="truncate text-2xs text-[var(--color-fg-muted)]">
          {driver !== '' && `${t('tree.driver')}: ${driver}`}
          {driver !== '' && device.location !== null && ' · '}
          {device.location}
        </p>
      )}
    </li>
  );
}

function StatusIcon({ device }: { readonly device: DeviceInfo }) {
  const { t } = useTranslation(DEVICES_NS);
  switch (device.status) {
    case 'problem':
      return (
        <AlertTriangle
          role="img"
          aria-label={t('tree.hasProblem')}
          className="size-3.5 text-[var(--color-status-warn)]"
        />
      );
    case 'disabled':
      return <CircleOff role="img" aria-label={t('tree.disabled')} className="size-3.5" />;
    case 'notPresent':
      return <Unplug role="img" aria-label={t('tree.notConnected')} className="size-3.5" />;
    default:
      return null;
  }
}
