/**
 * The per-drive clean-up wizard: analyse, choose, confirm and run.
 *
 * # Three steps, and the last one is never skipped
 *
 * "Run my usual clean-up" preselects a remembered profile and jumps to the
 * confirmation; it never runs anything by itself. Every run goes through the
 * step that lists what will happen, what stops, and what cannot be undone.
 *
 * # Danger is opted into item by item
 *
 * Select-all never ticks a `danger` item, and a ticked one needs its own
 * "I understand" box before Next works. A remembered profile never carries a
 * danger kind either.
 *
 * # What stops is said before, and what did not restart is said after
 *
 * Compacting a virtual disk stops WSL and Docker Desktop. The choose and
 * confirm steps list exactly which distros, containers, tasks and VMs will be
 * stopped; the report lists what came back and, in the danger colour with
 * what to do, what did not.
 *
 * # Not closable while it runs
 *
 * The report is the only place the user learns what was removed and what is
 * still stopped, so Escape and the close button do nothing mid-run.
 */

import { ArrowLeft, ArrowRight, Sparkles, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  BrandMark,
  Button,
  Checkbox,
  DialogContent,
  DialogRoot,
  ProgressBar,
  Tooltip,
  TooltipProvider,
  formatBytes,
  formatCount,
} from '@vitals/ui';

import { useSettings } from '../../../settings/store';
import { STORAGE_NS } from '../strings';
import {
  analyseStages,
  blockersFor,
  driveCleanStages,
  groupCheckState,
  groupSelectable,
  initialSelection,
  isAvailable,
  isKnownKind,
  isUnknownHyperV,
  mountKey,
  profileKinds,
  profileSelection,
  roleReasonKeys,
  selectedItems,
  sizeTotal,
  unacknowledged,
  type Blocker,
  type DriveCleanProgress,
  type DriveCleanupPlan,
  type DriveCleanupReport,
  type PlanGroup,
  type PlanItem,
} from './model';
import { useDriveCleanup, type DriveCleanupSource } from './useDriveCleanup';

function useStorageT() {
  return useTranslation(STORAGE_NS).t;
}
type T = ReturnType<typeof useStorageT>;

export interface DriveCleanupDialogProps {
  readonly mount: string;
  readonly locale: string;
  readonly onClose: () => void;
  /** Injectable so tests need no Tauri host. */
  readonly source?: DriveCleanupSource;
}

export function itemName(item: PlanItem, t: T): string {
  const name = isKnownKind(item.kind)
    ? t(`drive.item.${item.kind}.name`)
    : t('drive.item.other.name');
  return item.label === null ? name : `${name} (${item.label})`;
}

function itemWhat(item: PlanItem, t: T): string {
  return isKnownKind(item.kind) ? t(`drive.item.${item.kind}.what`) : t('drive.item.other.what');
}

/** A size, or an em dash with the reason on hover and focus — never 0. */
function Size({ bytes, locale, t }: { bytes: number | null; locale: string; t: T }) {
  if (bytes !== null) {
    return <span className="tnum shrink-0 text-2xs">{formatBytes(bytes, locale)}</span>;
  }
  return (
    <Tooltip content={t('drive.notMeasured')}>
      {/* Focusable so the reason is reachable without a mouse. */}
      <span
        tabIndex={0}
        aria-label={t('drive.notMeasured')}
        className="tnum shrink-0 text-2xs text-[var(--color-fg-muted)]"
      >
        —
      </span>
    </Tooltip>
  );
}

function totalText(items: readonly PlanItem[], locale: string) {
  const total = sizeTotal(items);
  return { total, size: formatBytes(total.bytes, locale) };
}

export function DriveCleanupDialog({
  mount,
  locale,
  onClose,
  source,
}: DriveCleanupDialogProps): React.JSX.Element {
  const t = useStorageT();
  const state = useDriveCleanup(mount, source);
  const key = mountKey(mount);
  const profile = useSettings((s) => s.settings.driveCleanupProfiles[key]);
  const patch = useSettings((s) => s.patch);
  const profiles = useSettings((s) => s.settings.driveCleanupProfiles);

  const [view, setView] = useState<'choose' | 'confirm'>('choose');
  const [selected, setSelected] = useState<ReadonlySet<string>>(() => new Set());
  const [acknowledged, setAcknowledged] = useState<ReadonlySet<string>>(() => new Set());
  const [understood, setUnderstood] = useState(false);
  const [remember, setRemember] = useState(profile !== undefined);
  const [seenScan, setSeenScan] = useState<number | null>(null);

  const phase = state.phase;
  // The first selection is applied while rendering, guarded by the scan id,
  // rather than in an effect that would paint one frame with nothing ticked.
  if (phase.step === 'plan' && seenScan !== phase.plan.scanId) {
    setSeenScan(phase.plan.scanId);
    setSelected(initialSelection(phase.plan, profile));
  }

  const running = phase.step === 'running';
  const finished = phase.step === 'report' || phase.step === 'runFailed';

  const toggle = (item: PlanItem, on: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      if (on) next.add(item.id);
      else next.delete(item.id);
      return next;
    });
    if (!on) {
      setAcknowledged((current) => {
        const next = new Set(current);
        next.delete(item.id);
        return next;
      });
    }
  };

  const setGroup = (group: PlanGroup, on: boolean) => {
    setSelected((current) => {
      const next = new Set(current);
      for (const id of groupSelectable(group)) {
        if (on) next.add(id);
        else next.delete(id);
      }
      return next;
    });
  };

  const acknowledge = (item: PlanItem, on: boolean) => {
    setAcknowledged((current) => {
      const next = new Set(current);
      if (on) next.add(item.id);
      else next.delete(item.id);
      return next;
    });
  };

  const startRun = (plan: DriveCleanupPlan) => {
    const items = selectedItems(plan, selected);
    // Saved before the run so a failed run still keeps the choice the user
    // asked to keep. Unticking removes a profile saved earlier.
    const others = Object.fromEntries(Object.entries(profiles).filter(([mount]) => mount !== key));
    patch({
      driveCleanupProfiles: remember ? { ...others, [key]: profileKinds(items) } : others,
    });
    void state.run(items.map((item) => item.id));
  };

  const title = finished
    ? phase.step === 'report'
      ? t('drive.report.title')
      : t('drive.report.notRunTitle')
    : t('drive.title', { mount });

  const stepIndex =
    phase.step === 'analysing' || phase.step === 'analyseFailed'
      ? 1
      : view === 'choose' && !running
        ? 2
        : 3;
  const stepName = t(
    `drive.steps.${(['analyse', 'choose', 'confirm'] as const)[stepIndex - 1] ?? 'analyse'}`,
  );

  let body: React.ReactNode = null;
  let footer: React.ReactNode = null;

  if (phase.step === 'analysing') {
    body = <Analysing mount={mount} progress={phase.progress} t={t} />;
    footer = (
      <Button variant="ghost" size="sm" onClick={state.cancelAnalysis}>
        {t('drive.analyse.stop')}
      </Button>
    );
  } else if (phase.step === 'analyseFailed') {
    body = (
      <p role="alert" className="text-sm text-[var(--color-danger-text)]">
        {t('drive.analyse.failed', { message: phase.message })}
      </p>
    );
    footer = (
      <Button variant="primary" size="sm" onClick={onClose}>
        {t('drive.close')}
      </Button>
    );
  } else if (phase.step === 'plan' && view === 'choose') {
    const plan = phase.plan;
    const items = selectedItems(plan, selected);
    const pending = unacknowledged(items, acknowledged);
    body = (
      <Choose
        plan={plan}
        selected={selected}
        acknowledged={acknowledged}
        profile={profile}
        locale={locale}
        t={t}
        onToggle={toggle}
        onGroup={setGroup}
        onAcknowledge={acknowledge}
        onUsual={(kinds) => {
          setSelected(new Set(profileSelection(plan, kinds)));
          setAcknowledged(new Set());
          setRemember(true);
          setView('confirm');
        }}
      />
    );
    footer = (
      <>
        <Button variant="ghost" size="sm" onClick={onClose}>
          {t('drive.cancel')}
        </Button>
        <Button
          variant="primary"
          size="sm"
          disabled={items.length === 0 || pending.length > 0}
          onClick={() => {
            setUnderstood(false);
            setView('confirm');
          }}
        >
          {t('drive.next')}
          <ArrowRight aria-hidden className="size-4" />
        </Button>
      </>
    );
  } else if (phase.step === 'plan' || phase.step === 'running') {
    const plan = phase.plan;
    const items = selectedItems(plan, selected);
    const irreversible = items.some((item) => item.irreversible || item.safety === 'danger');
    body = (
      <Confirm
        plan={plan}
        items={items}
        irreversible={irreversible}
        understood={understood}
        remember={remember}
        running={running}
        progress={phase.step === 'running' ? phase.progress : null}
        locale={locale}
        t={t}
        onUnderstood={setUnderstood}
        onRemember={setRemember}
      />
    );
    footer = (
      <>
        <Button
          variant="ghost"
          size="sm"
          disabled={running}
          onClick={() => {
            setView('choose');
          }}
        >
          <ArrowLeft aria-hidden className="size-4" />
          {t('drive.back')}
        </Button>
        <Button
          variant="danger"
          size="sm"
          disabled={running || (irreversible && !understood)}
          onClick={() => {
            startRun(plan);
          }}
        >
          <Trash2 aria-hidden className="size-4" />
          {running ? t('drive.run.working') : t('drive.confirm.run')}
        </Button>
      </>
    );
  } else if (phase.step === 'report') {
    body = <Report plan={phase.plan} report={phase.report} locale={locale} t={t} />;
    footer = (
      <Button variant="primary" size="sm" onClick={onClose}>
        {t('drive.report.done')}
      </Button>
    );
  } else if (phase.step === 'runFailed') {
    body = (
      <p role="alert" className="text-sm text-[var(--color-warn-text)]">
        {phase.refused
          ? t('drive.report.refused', { message: phase.message })
          : t('drive.report.failed', { message: phase.message })}
      </p>
    );
    footer = (
      <Button variant="primary" size="sm" onClick={onClose}>
        {t('drive.report.done')}
      </Button>
    );
  }

  return (
    <DialogRoot
      open
      onOpenChange={(next) => {
        if (!next && !running) onClose();
      }}
    >
      <DialogContent size="xl" title={title} closeLabel={t('drive.close')} footer={footer}>
        <TooltipProvider>
          <div className="flex flex-col gap-3 text-sm">
            {!finished && (
              <p className="text-2xs text-[var(--color-fg-muted)]">
                {t('drive.stepOf', { n: stepIndex, name: stepName })}
              </p>
            )}
            {body}
          </div>
        </TooltipProvider>
      </DialogContent>
    </DialogRoot>
  );
}

function Analysing({
  mount,
  progress,
  t,
}: {
  readonly mount: string;
  readonly progress: { stage: string; done: number; total: number } | null;
  readonly t: T;
}) {
  const label = t('drive.analyse.running', { mount });
  const stage =
    progress === null
      ? t('drive.analyse.starting')
      : (analyseStages as readonly string[]).includes(progress.stage)
        ? t(`drive.analyse.stage.${progress.stage}`)
        : t('drive.analyse.stage.other');
  return (
    <div role="status" className="flex flex-col gap-1.5">
      <p className="flex items-center gap-2">
        <BrandMark size={20} state="thinking" />
        {label}
      </p>
      <p className="text-2xs text-[var(--color-fg-muted)]">{stage}</p>
      {progress === null || progress.total <= 0 ? (
        <ProgressBar indeterminate label={label} />
      ) : (
        <ProgressBar
          value={Math.min(progress.done, progress.total)}
          max={progress.total}
          label={label}
          valueText={t('drive.analyse.progress', { done: progress.done, total: progress.total })}
        />
      )}
    </div>
  );
}

function RoleHeader({ plan, locale, t }: { plan: DriveCleanupPlan; locale: string; t: T }) {
  // A reason a newer backend adds is skipped rather than shown as its key;
  // contract.test.ts keeps the known list equal to drive_role.rs.
  const known = new Set<string>(roleReasonKeys.map((key) => `role.reason.${key}`));
  const reasons = plan.roleReasons
    .filter((reason) => known.has(reason))
    .map((reason) => t(`drive.${reason}`));
  return (
    <div className="flex flex-col gap-1">
      <p className="flex flex-wrap items-center gap-2">
        <Badge tone="accent">{t(`drive.role.${plan.role}`)}</Badge>
        {plan.totalBytes > 0 && (
          <span className="text-2xs text-[var(--color-fg-muted)]">
            {t('drive.free', {
              free: formatBytes(plan.freeBytes, locale),
              total: formatBytes(plan.totalBytes, locale),
            })}
          </span>
        )}
      </p>
      {reasons.length > 0 && (
        <div className="text-2xs text-[var(--color-fg-muted)]">
          <p>{t('drive.role.why')}</p>
          <ul className="list-disc pl-4">
            {reasons.map((reason) => (
              <li key={reason}>{reason}</li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

/** "These will be stopped…", split by whether each comes back by itself. */
export function BlockersPreview({ blockers, t }: { blockers: readonly Blocker[]; t: T }) {
  if (blockers.length === 0) return null;
  const back = blockers.filter((b) => b.restartAfter);
  const stay = blockers.filter((b) => !b.restartAfter);
  return (
    <div
      role="note"
      className="flex flex-col gap-1 rounded-md border border-[var(--color-status-warn)] p-2 text-2xs"
    >
      {back.length > 0 && (
        <BlockerList title={t('drive.blockers.restartTitle')} blockers={back} t={t} />
      )}
      {stay.length > 0 && (
        <BlockerList title={t('drive.blockers.stayTitle')} blockers={stay} t={t} />
      )}
    </div>
  );
}

function BlockerList({
  title,
  blockers,
  t,
}: {
  title: string;
  blockers: readonly Blocker[];
  t: T;
}) {
  return (
    <div>
      <p className="font-medium">{title}</p>
      <ul className="list-disc pl-4">
        {blockers.map((blocker) => (
          <li key={`${blocker.kind}:${blocker.name}`}>{blockerName(blocker, t)}</li>
        ))}
      </ul>
    </div>
  );
}

/** "Virtual machine unknown" would read as a machine of that name. */
function blockerName(blocker: Blocker, t: T): string {
  return isUnknownHyperV(blocker)
    ? t('drive.blockers.unknownHyperV')
    : t(`drive.blockers.kind.${blocker.kind}`, { name: blocker.name });
}

function blockerAdvice(blocker: Blocker, t: T): string {
  return isUnknownHyperV(blocker)
    ? t('drive.blockers.unknownHyperVAdvice')
    : t(`drive.blockers.advice.${blocker.kind}`, { name: blocker.name });
}

function Choose({
  plan,
  selected,
  acknowledged,
  profile,
  locale,
  t,
  onToggle,
  onGroup,
  onAcknowledge,
  onUsual,
}: {
  readonly plan: DriveCleanupPlan;
  readonly selected: ReadonlySet<string>;
  readonly acknowledged: ReadonlySet<string>;
  readonly profile: readonly string[] | undefined;
  readonly locale: string;
  readonly t: T;
  readonly onToggle: (item: PlanItem, on: boolean) => void;
  readonly onGroup: (group: PlanGroup, on: boolean) => void;
  readonly onAcknowledge: (item: PlanItem, on: boolean) => void;
  readonly onUsual: (kinds: readonly string[]) => void;
}) {
  const groups = plan.groups.filter((group) => group.items.length > 0);
  const items = selectedItems(plan, selected);
  const { total, size } = totalText(items, locale);
  const blockers = blockersFor(plan, items);

  return (
    <div className="flex flex-col gap-3">
      <RoleHeader plan={plan} locale={locale} t={t} />

      {profile !== undefined && profile.length > 0 && (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-[var(--color-accent-border)] bg-[var(--color-accent-subtle)] p-2">
          <div>
            <p className="font-medium">{t('drive.usual.title')}</p>
            <p className="text-2xs text-[var(--color-fg-muted)]">
              {t('drive.usual.body', {
                count: profile.length,
                n: formatCount(profile.length, locale),
              })}
            </p>
          </div>
          <Button
            variant="secondary"
            size="sm"
            onClick={() => {
              onUsual(profile);
            }}
          >
            <Sparkles aria-hidden className="size-4" />
            {t('drive.usual.run')}
          </Button>
        </div>
      )}

      {groups.length === 0 ? (
        <p>{t('drive.nothingFound')}</p>
      ) : (
        <div className="flex max-h-[50vh] flex-col gap-3 overflow-auto pr-1">
          {groups.map((group) => (
            <Group
              key={group.key}
              group={group}
              selected={selected}
              acknowledged={acknowledged}
              locale={locale}
              t={t}
              onToggle={onToggle}
              onGroup={onGroup}
              onAcknowledge={onAcknowledge}
            />
          ))}
        </div>
      )}

      <div className="flex flex-col gap-1" aria-live="polite">
        <p className="tnum font-medium">
          {t('drive.selection', { count: total.count, n: formatCount(total.count, locale), size })}
          {total.unmeasured > 0 && (
            <> {t('drive.selectionUnmeasured', { n: formatCount(total.unmeasured, locale) })}</>
          )}
        </p>
        {plan.totalBytes > 0 && total.count > 0 && (
          <p className="tnum text-2xs text-[var(--color-fg-muted)]">
            {t('drive.freeAfter', {
              size: formatBytes(Math.min(plan.totalBytes, plan.freeBytes + total.bytes), locale),
            })}
          </p>
        )}
      </div>
      <BlockersPreview blockers={blockers} t={t} />
    </div>
  );
}

function Group({
  group,
  selected,
  acknowledged,
  locale,
  t,
  onToggle,
  onGroup,
  onAcknowledge,
}: {
  readonly group: PlanGroup;
  readonly selected: ReadonlySet<string>;
  readonly acknowledged: ReadonlySet<string>;
  readonly locale: string;
  readonly t: T;
  readonly onToggle: (item: PlanItem, on: boolean) => void;
  readonly onGroup: (group: PlanGroup, on: boolean) => void;
  readonly onAcknowledge: (item: PlanItem, on: boolean) => void;
}) {
  const name = t(`drive.group.${group.key}`);
  const state = groupCheckState(group, selected);
  const gain = sizeTotal(group.items);
  const gainSize = formatBytes(gain.bytes, locale);
  return (
    <section aria-label={name} className="flex flex-col gap-1">
      <div className="flex items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          {groupSelectable(group).length > 0 && (
            <Checkbox
              checked={state}
              ariaLabel={t('drive.tickGroup', { group: name })}
              onCheckedChange={() => {
                onGroup(group, state !== true);
              }}
            />
          )}
          <h4 className="font-medium">{name}</h4>
        </div>
        <span className="tnum text-2xs text-[var(--color-fg-muted)]">
          {gain.unmeasured > 0
            ? t('drive.groupGainFloor', {
                size: gainSize,
                n: formatCount(gain.unmeasured, locale),
              })
            : t('drive.groupGain', { size: gainSize })}
        </span>
      </div>
      <ul className="flex flex-col gap-1">
        {group.items.map((item) => (
          <Item
            key={item.id}
            item={item}
            checked={selected.has(item.id)}
            acknowledged={acknowledged.has(item.id)}
            locale={locale}
            t={t}
            onToggle={onToggle}
            onAcknowledge={onAcknowledge}
          />
        ))}
      </ul>
    </section>
  );
}

function ItemBadges({ item, t }: { item: PlanItem; t: T }) {
  return (
    <>
      {item.safety === 'review' && <Badge tone="warn">{t('drive.badge.review')}</Badge>}
      {item.safety === 'danger' && <Badge tone="danger">{t('drive.badge.danger')}</Badge>}
      {item.irreversible && item.safety !== 'danger' && (
        <Badge tone="neutral">{t('drive.badge.irreversible')}</Badge>
      )}
      {item.needsElevation && <Badge tone="info">{t('drive.badge.elevation')}</Badge>}
      {!isAvailable(item) && <Badge tone="neutral">{t('drive.badge.unavailable')}</Badge>}
    </>
  );
}

function Item({
  item,
  checked,
  acknowledged,
  locale,
  t,
  onToggle,
  onAcknowledge,
}: {
  readonly item: PlanItem;
  readonly checked: boolean;
  readonly acknowledged: boolean;
  readonly locale: string;
  readonly t: T;
  readonly onToggle: (item: PlanItem, on: boolean) => void;
  readonly onAcknowledge: (item: PlanItem, on: boolean) => void;
}) {
  const name = itemName(item, t);
  return (
    <li
      className={`rounded-md border p-2 ${
        item.safety === 'danger'
          ? 'border-[var(--color-status-danger)]'
          : 'border-[var(--color-border-subtle)]'
      }`}
    >
      <div className="flex items-start justify-between gap-2">
        <Checkbox
          checked={checked}
          disabled={!isAvailable(item)}
          label={name}
          onCheckedChange={(next) => {
            onToggle(item, next === true);
          }}
        />
        <Size bytes={item.size} locale={locale} t={t} />
      </div>
      <div className="mt-1 flex flex-wrap gap-1 pl-6">
        <ItemBadges item={item} t={t} />
      </div>
      <p className="pl-6 text-2xs text-[var(--color-fg-muted)]">{itemWhat(item, t)}</p>
      {item.detail !== null && (
        <p className="pl-6 font-mono text-2xs break-all text-[var(--color-fg-subtle)]">
          {item.detail}
        </p>
      )}
      {item.safety === 'danger' && checked && (
        <div className="mt-1 flex flex-col gap-1 pl-6 text-2xs">
          <p className="text-[var(--color-danger-text)]">{t('drive.dangerHint')}</p>
          <Checkbox
            checked={acknowledged}
            label={t('drive.dangerAck', { name })}
            onCheckedChange={(next) => {
              onAcknowledge(item, next === true);
            }}
          />
        </div>
      )}
    </li>
  );
}

function Confirm({
  plan,
  items,
  irreversible,
  understood,
  remember,
  running,
  progress,
  locale,
  t,
  onUnderstood,
  onRemember,
}: {
  readonly plan: DriveCleanupPlan;
  readonly items: readonly PlanItem[];
  readonly irreversible: boolean;
  readonly understood: boolean;
  readonly remember: boolean;
  readonly running: boolean;
  readonly progress: DriveCleanProgress | null;
  readonly locale: string;
  readonly t: T;
  readonly onUnderstood: (on: boolean) => void;
  readonly onRemember: (on: boolean) => void;
}) {
  const elevation = items.some((item) => item.needsElevation);
  const { total, size } = totalText(items, locale);
  return (
    <div className="flex flex-col gap-2">
      <p className="font-medium">{t('drive.confirm.heading')}</p>
      <ul className="flex max-h-60 flex-col gap-1 overflow-auto">
        {items.map((item) => (
          <li
            key={item.id}
            className="flex items-baseline justify-between gap-2 rounded-md border border-[var(--color-border-subtle)] p-2"
          >
            <span className="flex flex-wrap items-center gap-1">
              {itemName(item, t)}
              <ItemBadges item={item} t={t} />
            </span>
            <Size bytes={item.size} locale={locale} t={t} />
          </li>
        ))}
      </ul>
      <p className="tnum">
        {t('drive.selection', { count: total.count, n: formatCount(total.count, locale), size })}
        {total.unmeasured > 0 && (
          <> {t('drive.selectionUnmeasured', { n: formatCount(total.unmeasured, locale) })}</>
        )}
      </p>
      <BlockersPreview blockers={blockersFor(plan, items)} t={t} />
      {(irreversible || elevation) && (
        <div
          role="note"
          className="flex flex-col gap-2 rounded-md border border-[var(--color-status-danger)] p-2 text-2xs"
        >
          {irreversible && <p>{t('drive.confirm.irreversible')}</p>}
          {elevation && <p>{t('drive.confirm.elevation')}</p>}
          {irreversible && (
            <Checkbox
              checked={understood}
              disabled={running}
              label={t('drive.confirm.understood')}
              onCheckedChange={(next) => {
                onUnderstood(next === true);
              }}
            />
          )}
        </div>
      )}
      <div className="flex flex-col gap-0.5">
        <Checkbox
          checked={remember}
          disabled={running}
          label={t('drive.confirm.remember')}
          onCheckedChange={(next) => {
            onRemember(next === true);
          }}
        />
        <p className="pl-6 text-2xs text-[var(--color-fg-muted)]">
          {t('drive.confirm.rememberHint')}
        </p>
      </div>
      {running && <Running plan={plan} progress={progress} t={t} />}
    </div>
  );
}

export function stageText(progress: DriveCleanProgress, t: T): string {
  const stage = progress.stage;
  if (stage === 'compacting') {
    return progress.diskIndex !== null && progress.diskCount !== null && progress.diskCount > 0
      ? t('drive.run.stage.compacting', {
          // Already 1-based from the backend.
          index: Math.min(Math.max(progress.diskIndex, 1), progress.diskCount),
          count: progress.diskCount,
        })
      : t('drive.run.stage.compactingAll');
  }
  return (driveCleanStages as readonly string[]).includes(stage)
    ? t(`drive.run.stage.${stage}`)
    : t('drive.run.stage.other');
}

function Running({
  plan,
  progress,
  t,
}: {
  readonly plan: DriveCleanupPlan;
  readonly progress: DriveCleanProgress | null;
  readonly t: T;
}) {
  const text = progress === null ? t('drive.run.working') : stageText(progress, t);
  const currentItem =
    progress?.current == null
      ? null
      : (plan.groups.flatMap((g) => g.items).find((item) => item.id === progress.current) ?? null);
  const current =
    progress?.current == null
      ? null
      : currentItem === null
        ? progress.current.replace(/^sparse:/, '')
        : itemName(currentItem, t);
  return (
    <div role="status" className="flex flex-col gap-1">
      <p className="flex items-center gap-2 font-medium">
        <BrandMark size={20} state="thinking" />
        {text}
      </p>
      {progress === null || progress.total <= 0 ? (
        <ProgressBar indeterminate label={t('drive.run.working')} />
      ) : (
        <ProgressBar
          value={Math.min(progress.done, progress.total)}
          max={progress.total}
          label={t('drive.run.working')}
          valueText={t('drive.run.progress', { done: progress.done, total: progress.total })}
        />
      )}
      {current !== null && (
        <p className="text-2xs text-[var(--color-fg-subtle)]">
          {t('drive.run.current', { name: current })}
        </p>
      )}
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('drive.run.keepOpen')}</p>
    </div>
  );
}

const STATUS_TONE = { done: 'ok', skipped: 'neutral', failed: 'danger' } as const;

function Report({
  plan,
  report,
  locale,
  t,
}: {
  readonly plan: DriveCleanupPlan;
  readonly report: DriveCleanupReport;
  readonly locale: string;
  readonly t: T;
}) {
  const items = new Map(plan.groups.flatMap((g) => g.items).map((item) => [item.id, item]));
  const freed = report.driveFreedBytes;
  return (
    <div className="flex flex-col gap-2">
      <p className="font-medium">
        {freed === null
          ? t('drive.report.driveUnknown', { mount: plan.mount })
          : freed >= 0
            ? t('drive.report.drive', { mount: plan.mount, size: formatBytes(freed, locale) })
            : t('drive.report.driveLost', { mount: plan.mount, size: formatBytes(-freed, locale) })}
      </p>
      <ul className="flex max-h-72 flex-col gap-1 overflow-auto">
        {report.items.map((line) => {
          const item = items.get(line.id);
          return (
            <li
              key={line.id}
              className="flex flex-col gap-0.5 rounded-md border border-[var(--color-border-subtle)] p-2"
            >
              <div className="flex items-baseline justify-between gap-2">
                <span className="flex items-center gap-2">
                  <Badge tone={STATUS_TONE[line.status]}>
                    {t(`drive.report.status.${line.status}`)}
                  </Badge>
                  {item === undefined ? line.id : itemName(item, t)}
                </span>
                {line.freed === null ? (
                  <Size bytes={null} locale={locale} t={t} />
                ) : (
                  <span className="tnum text-2xs">
                    {t('drive.report.freed', {
                      size: formatBytes(Math.max(0, line.freed), locale),
                    })}
                  </span>
                )}
              </div>
              {line.message !== null && (
                <p className="text-2xs text-[var(--color-fg-muted)]">{line.message}</p>
              )}
            </li>
          );
        })}
      </ul>
      {report.restarted.length > 0 && (
        <BlockerList title={t('drive.report.restarted')} blockers={report.restarted} t={t} />
      )}
      {report.restartFailed.length > 0 && (
        <div role="alert" className="text-[var(--color-danger-text)]">
          <p className="font-medium">{t('drive.report.restartFailed')}</p>
          <ul className="list-disc pl-4">
            {report.restartFailed.map((blocker) => (
              <li key={`${blocker.kind}:${blocker.name}`}>
                {blockerName(blocker, t)}
                {' — '}
                {blockerAdvice(blocker, t)}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}
