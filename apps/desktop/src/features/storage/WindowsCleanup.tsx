/**
 * Freeing Windows-managed space with Windows' own tool.
 *
 * # Vitals deletes nothing here
 *
 * The component store, the hibernation file, `Windows.old` and the update
 * caches are not files Vitals should touch. The dialog runs the tool Windows
 * provides for each — Disk Cleanup with one item ticked, DISM, or
 * `powercfg` — in an instance elevated for this one action.
 *
 * # What can't be undone says so, and has to be ticked
 *
 * Emptying the bin, removing the previous Windows and turning hibernation
 * off cannot be taken back. Those three state the consequence in their own
 * words and need a tick before the button works; the backend refuses them
 * unconfirmed as well.
 *
 * # Freed is what was measured
 *
 * The figure in the report is read before and after the tool ran. When the
 * location could not be read, the drive's free space is shown instead, and
 * said to be that.
 */

import { ShieldCheck } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  Checkbox,
  DialogContent,
  DialogRoot,
  ProgressBar,
  formatBytes,
} from '@vitals/ui';

import { measuredFreed, type CleanupCandidate, type WindowsCleanupReport } from './model';
import { STORAGE_NS } from './strings';
import type { WindowsRun } from './useStorage';

export interface WindowsCleanupDialogProps {
  readonly candidate: CleanupCandidate | null;
  readonly run: WindowsRun | null;
  readonly locale: string;
  readonly onConfirm: (confirmed: boolean) => void;
  readonly onClose: () => void;
}

export function WindowsCleanupDialog({
  candidate,
  run,
  locale,
  onConfirm,
  onClose,
}: WindowsCleanupDialogProps) {
  const { t } = useTranslation(STORAGE_NS);
  const [understood, setUnderstood] = useState(false);
  if (candidate === null || candidate.tool === null) return null;

  const mine = run !== null && run.path === candidate.path ? run : null;
  const running = mine?.running === true;
  const finished = mine !== null && !mine.running;
  const label = t(`kindLabel.${candidate.kind}`);
  const irreversible = candidate.irreversible;
  // "What Windows freed" only when Windows actually ran; a declined prompt
  // or a refusal freed nothing and must not be titled as if it had.
  const title = !finished
    ? t('windows.confirmTitle', { label })
    : mine.report !== null
      ? t('windows.reportTitle', { label })
      : t('windows.notRunTitle', { label });

  const close = () => {
    setUnderstood(false);
    onClose();
  };

  return (
    <DialogRoot
      open
      onOpenChange={(next) => {
        // Not closable while the tool runs: its report is the only place
        // the user learns what it did.
        if (!next && !running) close();
      }}
    >
      <DialogContent
        size="md"
        title={title}
        {...(!finished && { description: t(`windows.tool.${candidate.tool}`) })}
        closeLabel={t('windows.close')}
        footer={
          finished ? (
            <Button variant="primary" size="sm" onClick={close}>
              {t('windows.done')}
            </Button>
          ) : (
            <>
              <Button variant="ghost" size="sm" onClick={close} disabled={running}>
                {t('windows.cancel')}
              </Button>
              {/* Danger only where the effect is permanent; everything else
                  Windows regenerates or re-downloads. */}
              <Button
                size="sm"
                variant={irreversible ? 'danger' : 'primary'}
                disabled={running || (irreversible && !understood)}
                onClick={() => {
                  onConfirm(irreversible && understood);
                }}
              >
                <ShieldCheck aria-hidden className="size-4" />
                {running ? t('windows.running') : t(`windows.confirm.${candidate.tool}`)}
              </Button>
            </>
          )
        }
      >
        {finished ? (
          <Outcome run={mine} locale={locale} />
        ) : (
          <div className="flex flex-col gap-2 text-sm">
            <p className="font-mono text-2xs break-all text-[var(--color-fg-subtle)]">
              {candidate.path}
            </p>
            <p className="text-2xs text-[var(--color-fg-muted)]">{t('windows.elevation')}</p>
            {irreversible && (
              <div
                role="note"
                className="rounded-md border border-[var(--color-status-danger)] p-2 text-2xs"
              >
                <p className="mb-2">{t(`consequence.${candidate.kind}`)}</p>
                <Checkbox
                  checked={understood}
                  disabled={running}
                  onCheckedChange={(next) => {
                    setUnderstood(next === true);
                  }}
                  label={t('windows.understood')}
                />
              </div>
            )}
            {running && <Running run={mine} locale={locale} />}
          </div>
        )}
      </DialogContent>
    </DialogRoot>
  );
}

function Running({ run, locale }: { readonly run: WindowsRun; readonly locale: string }) {
  const { t } = useTranslation(STORAGE_NS);
  const progress = run.progress;
  const stage = progress?.stage ?? 'measuring';
  const seconds = Math.round((progress?.elapsedMs ?? 0) / 1000);
  return (
    <div role="status" className="flex flex-col gap-1">
      <p>{t(`windows.stage.${stage}`)}</p>
      <ProgressBar indeterminate label={t(`windows.stage.${stage}`)} />
      <p className="tnum text-2xs text-[var(--color-fg-muted)]">
        {t('windows.elapsed', { seconds })}
        {progress?.driveFreed !== null && progress?.driveFreed !== undefined && (
          <>
            {' · '}
            {t('windows.soFar', { size: formatBytes(Math.max(0, progress.driveFreed), locale) })}
          </>
        )}
      </p>
    </div>
  );
}

function Outcome({ run, locale }: { readonly run: WindowsRun; readonly locale: string }) {
  const { t } = useTranslation(STORAGE_NS);
  if (run.report === null) {
    return (
      <p role="alert" className="text-sm text-[var(--color-status-warn)]">
        {run.declined
          ? t('windows.declined', { message: run.error ?? '' })
          : t('windows.failed', { message: run.error ?? '' })}
      </p>
    );
  }
  return <Report report={run.report} locale={locale} />;
}

function Report({
  report,
  locale,
}: {
  readonly report: WindowsCleanupReport;
  readonly locale: string;
}) {
  const { t } = useTranslation(STORAGE_NS);
  const freed = measuredFreed(report);
  const size = (bytes: number) => formatBytes(bytes, locale);
  // Measured live: `powercfg /h off` exits 0 and leaves a 76.7 GB
  // hiberfil.sys where hibernation was already off. "Done" beside 0 B reads
  // as a success; this says what happened instead.
  const unchanged = report.outcome === 'done' && freed !== null && freed.bytes === 0;
  return (
    <div className="flex flex-col gap-2 text-sm">
      <p className="flex items-center gap-2">
        <Badge tone={report.outcome === 'toolFailed' ? 'warn' : unchanged ? 'neutral' : 'ok'}>
          {unchanged ? t('windows.unchangedBadge') : t(`windows.outcome.${report.outcome}`)}
        </Badge>
        <span className="font-medium" aria-live="polite">
          {freed === null
            ? t('windows.freedUnknown')
            : freed.from === 'location'
              ? t('windows.freed', { size: size(freed.bytes) })
              : t('windows.freedDrive', { size: size(freed.bytes) })}
        </span>
      </p>
      {unchanged && <p className="text-2xs">{t('windows.unchanged')}</p>}
      {report.outcome === 'needsRestart' && <p className="text-2xs">{t('windows.needsRestart')}</p>}
      {report.outcome === 'toolFailed' && (
        <p className="text-2xs">{t('windows.toolFailed', { code: hex(report.code ?? 0) })}</p>
      )}
      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-0.5 text-2xs text-[var(--color-fg-muted)]">
        {report.locationBefore !== null && report.locationAfter !== null && (
          <>
            <dt>{t('windows.location')}</dt>
            <dd className="tnum font-mono">
              {t('windows.beforeAfter', {
                before: size(report.locationBefore),
                after: size(report.locationAfter),
              })}
            </dd>
          </>
        )}
        {report.driveFreed !== null && (
          <>
            <dt>{t('windows.drive')}</dt>
            <dd className="tnum font-mono">
              {report.driveFreed >= 0
                ? t('windows.driveGained', { size: size(report.driveFreed) })
                : t('windows.driveLost', { size: size(-report.driveFreed) })}
            </dd>
          </>
        )}
      </dl>
      <p className="text-2xs text-[var(--color-fg-subtle)]">{t('windows.measuredNote')}</p>
    </div>
  );
}

function hex(code: number): string {
  return `0x${(code >>> 0).toString(16).toUpperCase().padStart(8, '0')}`;
}
