/**
 * Guided move of a WSL distro's disk, or Docker Desktop's, to another drive
 * (ADR-0040). Opened from the clean-up wizard next to a virtual disk on a
 * drive that is low on space; loaded lazily so the wizard pays nothing for
 * it until it is opened.
 *
 * # Every drive is listed, and the ones that cannot take the disk say why
 *
 * A picker that silently hides drives leaves the user wondering where their
 * USB disk went. Ineligible drives are shown disabled with the reason.
 *
 * # The move is confirmed with what stops, and that it cannot be stopped
 *
 * The confirm step names the distro that stops, says copying cannot be
 * interrupted, says it is reversible, and needs the backup box ticked.
 * The dialog cannot be closed while the disk is copying.
 *
 * # Docker Desktop moves its own disk
 *
 * For Docker the dialog is a guide: the exact setting and folder, a button
 * to open Docker Desktop, and a check of where the disk is afterwards.
 */

import { ArrowLeft, ArrowRight, HardDriveUpload, RefreshCw } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  BrandMark,
  Button,
  Checkbox,
  DialogContent,
  DialogRoot,
  ProgressBar,
  formatBytes,
} from '@vitals/ui';

import { errorMessage } from '../../../../lib/commandError';
import { STORAGE_NS } from '../../strings';
import {
  canMoveHere,
  dockerMovedOff,
  eligibleTargets,
  initialDisk,
  isKnown,
  moveRefusals,
  moveStages,
  moveUnavailables,
  type DockerDisk,
  type MovableDisk,
  type MovePlan,
  type MoveProgress,
  type MoveReport,
  type MoveTarget,
} from './model';
import { ensureMoveStrings } from './strings';
import { tauriMoveSource, useDriveMove, type DriveMoveSource } from './useDriveMove';

function useStorageT() {
  return useTranslation(STORAGE_NS).t;
}
type T = ReturnType<typeof useStorageT>;

export interface MoveDiskDialogProps {
  /** The drive the disk is on, `C:`. */
  readonly mount: string;
  /** Opens on this disk (a distro name or `docker-desktop`) when present. */
  readonly focus: string | null;
  readonly locale: string;
  readonly onClose: () => void;
  /** Injectable so tests need no Tauri host. */
  readonly source?: DriveMoveSource;
}

function diskName(disk: MovableDisk, t: T): string {
  return disk.owner === 'dockerDesktop'
    ? t('move.disk.dockerDesktop')
    : t('move.disk.distro', { name: disk.name });
}

export function MoveDiskDialog({
  mount,
  focus,
  locale,
  onClose,
  source = tauriMoveSource,
}: MoveDiskDialogProps): React.JSX.Element {
  ensureMoveStrings();
  const t = useStorageT();
  const state = useDriveMove(mount, source);
  const phase = state.phase;
  const [view, setView] = useState<'choose' | 'confirm'>('choose');
  const [diskId, setDiskId] = useState<string | null>(null);
  const [target, setTarget] = useState<string | null>(null);
  const [backedUp, setBackedUp] = useState(false);
  const [seenPlan, setSeenPlan] = useState<number | null>(null);

  // The first choice is applied while rendering, guarded by the plan id,
  // rather than in an effect that would paint one frame with nothing chosen.
  if (phase.step === 'plan' && seenPlan !== phase.plan.planId) {
    setSeenPlan(phase.plan.planId);
    const first = initialDisk(phase.plan, focus);
    setDiskId(first?.id ?? null);
    setTarget(first === null ? null : (eligibleTargets(first)[0]?.mount ?? null));
  }

  const moving = phase.step === 'moving';
  const plan = 'plan' in phase ? phase.plan : null;
  const disk = plan?.disks.find((d) => d.id === diskId) ?? null;

  let body: React.ReactNode = null;
  let footer: React.ReactNode = null;
  const closeButton = (label: string, variant: 'primary' | 'ghost' = 'primary') => (
    <Button variant={variant} size="sm" onClick={onClose}>
      {label}
    </Button>
  );

  if (phase.step === 'loading') {
    body = (
      <p role="status" className="flex items-center gap-2">
        <BrandMark size={20} state="thinking" />
        {t('move.loading', { mount })}
      </p>
    );
    footer = closeButton(t('move.cancel'), 'ghost');
  } else if (phase.step === 'loadFailed') {
    body = (
      <p role="alert" className="text-sm text-[var(--color-danger-text)]">
        {t('move.loadFailed', { message: phase.message })}
      </p>
    );
    footer = closeButton(t('move.close'));
  } else if (phase.step === 'plan' && (view === 'choose' || disk === null)) {
    body =
      phase.plan.disks.length === 0 ? (
        <p>{t('move.nothing', { mount })}</p>
      ) : (
        <Choose
          plan={phase.plan}
          disk={disk}
          target={target}
          locale={locale}
          t={t}
          source={source}
          onDisk={(next) => {
            setDiskId(next.id);
            setTarget(eligibleTargets(next)[0]?.mount ?? null);
          }}
          onTarget={setTarget}
        />
      );
    footer = (
      <>
        {closeButton(t('move.cancel'), 'ghost')}
        {disk !== null && canMoveHere(disk) && (
          <Button
            variant="primary"
            size="sm"
            disabled={target === null}
            onClick={() => {
              setBackedUp(false);
              setView('confirm');
            }}
          >
            {t('move.next')}
            <ArrowRight aria-hidden className="size-4" />
          </Button>
        )}
      </>
    );
  } else if ((phase.step === 'plan' || phase.step === 'moving') && disk !== null) {
    const chosen = phase.step === 'moving' ? phase.target : (target ?? '');
    body = (
      <Confirm
        disk={disk}
        target={chosen}
        backedUp={backedUp}
        moving={moving}
        progress={phase.step === 'moving' ? phase.progress : null}
        locale={locale}
        t={t}
        onBackedUp={setBackedUp}
      />
    );
    footer = (
      <>
        <Button
          variant="ghost"
          size="sm"
          disabled={moving}
          onClick={() => {
            setView('choose');
          }}
        >
          <ArrowLeft aria-hidden className="size-4" />
          {t('move.back')}
        </Button>
        <Button
          variant="danger"
          size="sm"
          disabled={moving || !backedUp || chosen === ''}
          onClick={() => {
            void state.move(disk.id, chosen);
          }}
        >
          <HardDriveUpload aria-hidden className="size-4" />
          {moving ? t('move.run.working') : t('move.confirm.run')}
        </Button>
      </>
    );
  } else if (phase.step === 'report') {
    body = (
      <Report
        report={phase.report}
        name={disk?.name ?? ''}
        source={mount}
        target={phase.target}
        locale={locale}
        t={t}
      />
    );
    footer = closeButton(t('move.report.done'));
  } else if (phase.step === 'failed') {
    body = (
      <p role="alert" className="text-sm text-[var(--color-warn-text)]">
        {phase.refused
          ? t('move.report.refused', { message: phase.message })
          : t('move.report.failed', { message: phase.message })}
      </p>
    );
    footer = closeButton(t('move.report.done'));
  }

  return (
    <DialogRoot
      open
      onOpenChange={(next) => {
        if (!next && !moving) onClose();
      }}
    >
      <DialogContent
        size="lg"
        title={t('move.title', { mount })}
        closeLabel={t('move.close')}
        footer={footer}
      >
        <div className="flex flex-col gap-3 text-sm">{body}</div>
      </DialogContent>
    </DialogRoot>
  );
}

function Choose({
  plan,
  disk,
  target,
  locale,
  t,
  source,
  onDisk,
  onTarget,
}: {
  readonly plan: MovePlan;
  readonly disk: MovableDisk | null;
  readonly target: string | null;
  readonly locale: string;
  readonly t: T;
  readonly source: DriveMoveSource;
  readonly onDisk: (disk: MovableDisk) => void;
  readonly onTarget: (mount: string) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <fieldset className="flex flex-col gap-1">
        <legend className="mb-1 font-medium">{t('move.disk.heading')}</legend>
        {plan.disks.map((d) => (
          <label
            key={d.id}
            className="flex cursor-pointer items-start gap-2 rounded-md border border-[var(--color-border-subtle)] p-2 has-[:checked]:border-[var(--color-accent-border)]"
          >
            <input
              type="radio"
              name="move-disk"
              className="mt-1 accent-[var(--color-accent)]"
              checked={d.id === disk?.id}
              onChange={() => {
                onDisk(d);
              }}
            />
            <span className="flex flex-col gap-0.5">
              <span className="font-medium">{diskName(d, t)}</span>
              <span className="tnum text-2xs text-[var(--color-fg-muted)]">
                {t('move.disk.size', { size: formatBytes(d.size, locale) })}
                {d.running && <> · {t('move.disk.running')}</>}
              </span>
              <span className="font-mono text-2xs break-all text-[var(--color-fg-subtle)]">
                {d.path}
              </span>
              {isKnown(moveUnavailables, d.unavailable) && (
                <span className="text-2xs text-[var(--color-warn-text)]">
                  {t(`move.disk.unavailable.${d.unavailable}`)}
                </span>
              )}
            </span>
          </label>
        ))}
      </fieldset>

      {disk !== null && disk.owner === 'dockerDesktop' && (
        <DockerGuide docker={plan.docker} disk={disk} mount={plan.mount} t={t} source={source} />
      )}
      {disk !== null && disk.owner === 'distro' && disk.unavailable === null && (
        <Targets disk={disk} target={target} locale={locale} t={t} onTarget={onTarget} />
      )}
    </div>
  );
}

function refusalText(target: MoveTarget, t: T): string {
  return isKnown(moveRefusals, target.refusal) ? t(`move.target.refusal.${target.refusal}`) : '';
}

function Targets({
  disk,
  target,
  locale,
  t,
  onTarget,
}: {
  readonly disk: MovableDisk;
  readonly target: string | null;
  readonly locale: string;
  readonly t: T;
  readonly onTarget: (mount: string) => void;
}) {
  const eligible = eligibleTargets(disk);
  return (
    <fieldset className="flex flex-col gap-1">
      <legend className="mb-1 font-medium">{t('move.target.heading')}</legend>
      {disk.requiredBytes !== null && (
        <p className="tnum text-2xs text-[var(--color-fg-muted)]">
          {t('move.disk.needs', { size: formatBytes(disk.requiredBytes, locale) })}
        </p>
      )}
      {eligible.length === 0 && (
        <div role="note" className="rounded-md border border-[var(--color-status-warn)] p-2">
          <p>{t('move.target.none')}</p>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('move.target.noneHint')}</p>
        </div>
      )}
      {disk.targets.map((d) => {
        const ok = d.refusal === null;
        return (
          <label
            key={d.mount}
            className={`flex items-start gap-2 rounded-md border border-[var(--color-border-subtle)] p-2 ${
              ok
                ? 'cursor-pointer has-[:checked]:border-[var(--color-accent-border)]'
                : 'opacity-70'
            }`}
          >
            <input
              type="radio"
              name="move-target"
              className="mt-1 accent-[var(--color-accent)]"
              disabled={!ok}
              checked={ok && d.mount === target}
              onChange={() => {
                onTarget(d.mount);
              }}
            />
            <span className="flex flex-col gap-0.5">
              <span className="font-medium">
                {d.mount}
                {d.label !== null && <> {d.label}</>}
                {d.fileSystem !== null && (
                  <span className="text-2xs text-[var(--color-fg-muted)]"> · {d.fileSystem}</span>
                )}
              </span>
              <span className="tnum text-2xs text-[var(--color-fg-muted)]">
                {t('move.target.free', {
                  free: formatBytes(d.freeBytes, locale),
                  total: formatBytes(d.totalBytes, locale),
                })}
              </span>
              {!ok && (
                <span className="text-2xs text-[var(--color-fg-muted)]">
                  {t('move.target.whyNot', { reason: refusalText(d, t) })}
                </span>
              )}
            </span>
          </label>
        );
      })}
    </fieldset>
  );
}

function DockerGuide({
  docker,
  disk,
  mount,
  t,
  source,
}: {
  readonly docker: DockerDisk;
  readonly disk: MovableDisk;
  readonly mount: string;
  readonly t: T;
  readonly source: DriveMoveSource;
}) {
  const [checked, setChecked] = useState<DockerDisk | null>(null);
  const [checking, setChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const eligible = eligibleTargets(disk);
  const folder =
    docker.suggested.find((s) => s.mount === eligible[0]?.mount)?.folder ??
    docker.suggested[0]?.folder ??
    null;
  const now = checked ?? docker;
  const movedOff = checked === null ? null : dockerMovedOff(checked, mount);

  return (
    <section aria-label={t('move.docker.heading')} className="flex flex-col gap-2">
      <h4 className="font-medium">{t('move.docker.heading')}</h4>
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('move.docker.why')}</p>
      {!docker.installed && <p role="alert">{t('move.docker.notInstalled')}</p>}
      {docker.crossDriveOk === false && (
        <p role="alert" className="text-[var(--color-warn-text)]">
          {t('move.docker.oldVersion', { version: docker.version ?? '' })}
        </p>
      )}
      {folder === null ? (
        <p role="note">{t('move.docker.noTarget')}</p>
      ) : (
        <ol className="flex list-decimal flex-col gap-1 pl-5">
          <li>{t('move.docker.steps.one')}</li>
          <li>{t('move.docker.steps.two', { folder })}</li>
          <li>{t('move.docker.steps.three')}</li>
          <li>{t('move.docker.steps.four')}</li>
        </ol>
      )}
      <div className="flex flex-wrap gap-2">
        <Button
          variant="secondary"
          size="sm"
          disabled={!docker.installed}
          onClick={() => {
            void source.openDocker().catch((cause: unknown) => {
              setError(errorMessage(cause));
            });
          }}
        >
          {t('move.docker.open')}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          disabled={checking}
          onClick={() => {
            setChecking(true);
            setError(null);
            source
              .checkDocker()
              .then(setChecked)
              .catch((cause: unknown) => {
                setError(errorMessage(cause));
              })
              .finally(() => {
                setChecking(false);
              });
          }}
        >
          <RefreshCw aria-hidden className="size-4" />
          {checking ? t('move.docker.checking') : t('move.docker.check')}
        </Button>
      </div>
      <div aria-live="polite" className="flex flex-col gap-0.5 text-2xs">
        {now.path !== null && (
          <p className="font-mono break-all">{t('move.docker.current', { path: now.path })}</p>
        )}
        {movedOff === true && (
          <p className="text-[var(--color-ok-text)]">{t('move.docker.moved', { mount })}</p>
        )}
        {movedOff === false && <p>{t('move.docker.notYet', { mount })}</p>}
        {checked !== null && movedOff === null && <p>{t('move.docker.unknown')}</p>}
        {error !== null && <p role="alert">{error}</p>}
      </div>
    </section>
  );
}

function Confirm({
  disk,
  target,
  backedUp,
  moving,
  progress,
  locale,
  t,
  onBackedUp,
}: {
  readonly disk: MovableDisk;
  readonly target: string;
  readonly backedUp: boolean;
  readonly moving: boolean;
  readonly progress: MoveProgress | null;
  readonly locale: string;
  readonly t: T;
  readonly onBackedUp: (on: boolean) => void;
}) {
  const to = `${target}\\Vitals\\WSL\\${disk.name}`;
  return (
    <div className="flex flex-col gap-2">
      <p className="font-medium">{t('move.confirm.heading')}</p>
      <p className="font-mono text-2xs break-all">{t('move.confirm.from', { path: disk.path })}</p>
      <p className="font-mono text-2xs break-all">{t('move.confirm.to', { path: to })}</p>
      <p>
        {disk.running ? t('move.confirm.stops', { name: disk.name }) : t('move.confirm.noStop')}
      </p>
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('move.confirm.reversible')}</p>
      <div
        role="note"
        className="flex flex-col gap-2 rounded-md border border-[var(--color-status-danger)] p-2 text-2xs"
      >
        <p>{t('move.confirm.noInterrupt')}</p>
        <Checkbox
          checked={backedUp}
          disabled={moving}
          label={t('move.confirm.backup', { name: disk.name })}
          onCheckedChange={(next) => {
            onBackedUp(next === true);
          }}
        />
      </div>
      {moving && <Moving disk={disk} target={target} progress={progress} locale={locale} t={t} />}
    </div>
  );
}

export function moveStageText(
  progress: MoveProgress | null,
  name: string,
  mount: string,
  t: T,
): string {
  if (progress === null) return t('move.run.working');
  return isKnown(moveStages, progress.stage)
    ? t(`move.run.stage.${progress.stage}`, { name, mount })
    : t('move.run.stage.other');
}

function Moving({
  disk,
  target,
  progress,
  locale,
  t,
}: {
  readonly disk: MovableDisk;
  readonly target: string;
  readonly progress: MoveProgress | null;
  readonly locale: string;
  readonly t: T;
}) {
  const copying = progress?.stage === 'moving' && progress.done !== null && progress.total !== null;
  return (
    <div role="status" className="flex flex-col gap-1">
      <p className="flex items-center gap-2 font-medium">
        <BrandMark size={20} state="thinking" />
        {moveStageText(progress, disk.name, target, t)}
      </p>
      {copying && progress.total !== null && progress.total > 0 && progress.done !== null ? (
        <ProgressBar
          value={Math.min(progress.done, progress.total)}
          max={progress.total}
          label={t('move.run.working')}
          valueText={t('move.run.copied', {
            done: formatBytes(progress.done, locale),
            total: formatBytes(progress.total, locale),
          })}
        />
      ) : (
        <ProgressBar indeterminate label={t('move.run.working')} />
      )}
      <p className="text-2xs text-[var(--color-fg-muted)]">{t('move.run.cannotCancel')}</p>
    </div>
  );
}

const CHECKS = ['registeredAtTarget', 'fileAtTarget', 'oldFileGone', 'starts'] as const;

function Report({
  report,
  name,
  source,
  target,
  locale,
  t,
}: {
  readonly report: MoveReport;
  readonly name: string;
  readonly source: string;
  readonly target: string;
  readonly locale: string;
  readonly t: T;
}) {
  const status =
    report.status === 'moved' || report.status === 'movedWithProblems' ? report.status : 'notMoved';
  const tone =
    status === 'moved'
      ? 'text-[var(--color-ok-text)]'
      : status === 'movedWithProblems'
        ? 'text-[var(--color-warn-text)]'
        : 'text-[var(--color-danger-text)]';
  const accessDenied = report.message?.includes('E_ACCESSDENIED') ?? false;
  return (
    <div className="flex flex-col gap-2">
      <p role="status" className={`font-medium ${tone}`}>
        {t(`move.report.status.${status}`, { name, mount: target })}
      </p>
      {status !== 'notMoved' && (
        <ul className="flex flex-col gap-0.5 text-2xs">
          {CHECKS.map((check) => {
            const value = report.verification[check];
            return (
              <li key={check} className="flex justify-between gap-2">
                <span>{t(`move.report.check.${check}`)}</span>
                <span>
                  {value === null
                    ? t('move.report.notChecked')
                    : value
                      ? t('move.report.pass')
                      : t('move.report.fail')}
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {report.verification.freedBytes !== null && (
        <p className="tnum">
          {t('move.report.freed', {
            mount: source,
            size: formatBytes(report.verification.freedBytes, locale),
          })}
        </p>
      )}
      {report.message !== null && (
        <p className="text-2xs break-all">
          {t('move.report.message', { message: report.message })}
        </p>
      )}
      {accessDenied && <p className="text-2xs">{t('move.report.accessDenied')}</p>}
      {report.restarted.length > 0 && (
        <div className="text-2xs">
          <p>{t('move.report.restarted')}</p>
          <ul className="pl-4">
            {report.restarted.map((b) => (
              <li key={`${b.kind}:${b.name}`}>{b.name}</li>
            ))}
          </ul>
        </div>
      )}
      {report.restartFailed.length > 0 && (
        <div className="text-2xs text-[var(--color-danger-text)]">
          <p>{t('move.report.restartFailed')}</p>
          <ul className="pl-4">
            {report.restartFailed.map((b) => (
              <li key={`${b.kind}:${b.name}`}>
                {b.name} — {t(`drive.blockers.advice.${b.kind}`, { name: b.name })}
              </li>
            ))}
          </ul>
        </div>
      )}
      {report.command !== null && (
        <p className="font-mono text-2xs break-all text-[var(--color-fg-subtle)]">
          {t('move.report.command', { command: report.command })}
        </p>
      )}
    </div>
  );
}
