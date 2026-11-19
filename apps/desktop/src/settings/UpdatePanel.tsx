/**
 * The update section of the About panel.
 *
 * Deliberately shows one sentence and at most one button per state. An update
 * dialog that offers "check", "download" and "restart" simultaneously makes
 * the user reason about which one applies; the state machine already knows,
 * so the UI just renders its answer.
 */

import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { TaskManagerReplacement } from '@vitals/protocol';
import { Button, ProgressBar } from '@vitals/ui';

import {
  checkForUpdate,
  downloadAndInstall,
  installBackgroundUpdateNow,
  leftoverInstall,
  migrateToUserInstall,
  pendingBackgroundUpdate,
  quitInstallMode,
  removeLeftoverInstall,
  updateStatus,
  type QuitInstall,
  type UpdateState,
  type UpdateStatus,
} from '../lib/updater';
import { reportFailure } from '../lib/reportFailure';
import { reportSuccess } from '../lib/reportSuccess';
import { useTaskManagerReplacement } from './useTaskManagerReplacement';

/** What `checkForUpdate` hands back, kept only for the duration of one flow. */
type PendingUpdate = Awaited<ReturnType<typeof checkForUpdate>>['update'];

/**
 * Whether moving this copy to the account would take the Task Manager
 * replacement away.
 *
 * True only on a per-machine copy — the only kind that can move, so the only
 * kind that loses anything — whose replacement is currently on. A feature that
 * is already off loses nothing, so nothing is said: a warning nobody needs is
 * noise. Named and exported rather than inlined so the rule has one home and
 * can be tested directly.
 */
export function losesTaskManagerOnMove(
  status: UpdateStatus | null,
  replacement: TaskManagerReplacement | null,
): boolean {
  return status?.canMigrateToUser === true && replacement?.enabled === true;
}

export function UpdateSection() {
  const { t } = useTranslation();
  const [state, setState] = useState<UpdateState>({ kind: 'idle' });
  // Read here, not in the About panel above: the warning is part of this
  // section, and the registry is the truth for whether the replacement is on.
  const taskManager = useTaskManagerReplacement();

  // The handle is not state: nothing renders from it, and putting it in
  // `useState` would re-render the panel on every progress tick for a value
  // no element reads.
  const pending = useRef<PendingUpdate>(null);
  // What the background updater already downloaded. Shown instead of the
  // idle sentence, so "Check for updates" is not offered for a version that
  // is sitting on disk waiting for the user to quit.
  const [background, setBackground] = useState<string | null>(null);
  // Whether that download installs on quit, or needs a click because the
  // copy lives in Program Files and this session is not elevated (DEV-3).
  const [quitMode, setQuitMode] = useState<QuitInstall>('silent');
  // A Program Files copy left behind after moving to a per-user install.
  const [leftover, setLeftover] = useState<string | null>(null);
  // What the background thread is doing: when it last ran, why it is not
  // running, what it last failed with. Read once, on mount: the panel is
  // open for seconds and the values change on the scale of hours.
  const [status, setStatus] = useState<UpdateStatus | null>(null);
  // The clock is read once, in the effect below, and kept in state rather
  // than called from render: `Date.now()` in a render is impure, and the
  // React Compiler rejects it.
  const [statusReadAtMs, setStatusReadAtMs] = useState<number | null>(null);

  useEffect(() => {
    let live = true;
    void pendingBackgroundUpdate().then((version) => {
      if (live) setBackground(version);
    });
    void quitInstallMode().then((mode) => {
      if (live) setQuitMode(mode);
    });
    void leftoverInstall().then((path) => {
      if (live) setLeftover(path);
    });
    void updateStatus().then((next) => {
      if (live) {
        setStatus(next);
        setStatusReadAtMs(Date.now());
      }
    });
    return () => {
      live = false;
    };
  }, []);

  const busy = state.kind === 'checking' || state.kind === 'installing';

  async function check() {
    setState({ kind: 'checking' });
    const result = await checkForUpdate();
    pending.current = result.update;
    setState(result.state);
  }

  async function download() {
    const update = pending.current;
    if (update === null) return;

    setState({ kind: 'downloading', version: update.version, percent: null });
    const next = await downloadAndInstall(update, (percent) => {
      setState({ kind: 'downloading', version: update.version, percent });
    });
    setState(next);
  }

  async function restart() {
    const version = state.kind === 'ready' ? state.version : '';
    setState({ kind: 'installing', version });
    // The download sits where the background updater keeps its own, so the
    // install is the same backend path, with its teardown and its way back.
    const failure = await installBackgroundUpdateNow();
    // `null` means the process is on its way out; leave the installing state
    // on screen rather than flashing something else during teardown.
    if (failure !== null) setState(failure);
  }

  async function installBackground() {
    setState({ kind: 'installing', version: background ?? '' });
    const failure = await installBackgroundUpdateNow();
    if (failure !== null) setState(failure);
  }

  async function removeLeftover() {
    const removal = removeLeftoverInstall();
    // reportFailure swallows the rejection after toasting it; the second
    // handler only decides whether the offer goes away.
    void reportFailure(removal, t('settings.about.update.leftoverFailed'));
    const removed = await removal.then(
      () => true,
      () => false,
    );
    if (!removed) return;
    setLeftover(null);
    void reportSuccess(t('settings.about.update.leftoverRemoved'));
  }

  async function moveToThisAccount() {
    // The process exits on success, so there is no "done" state to render and
    // none is invented: the setup restarts the app from its new home.
    const migration = migrateToUserInstall();
    setState({ kind: 'installing', version: background ?? '' });
    void reportFailure(migration, t('settings.about.update.moveFailed'));
    await migration.catch(() => undefined);
  }

  const showBackground = background !== null && state.kind === 'idle';

  return (
    <div className="space-y-2 py-2" data-testid="update-section">
      <p className="text-sm font-medium">{t('settings.about.update.title')}</p>
      {showBackground ? (
        <p className="text-2xs">
          {quitMode === 'needsApproval'
            ? t('settings.about.update.pendingApproval', { version: background })
            : t('settings.about.update.pending', { version: background })}
        </p>
      ) : (
        <UpdateMessage state={state} />
      )}
      {showBackground ? (
        <Button onClick={() => void installBackground()}>
          {t('settings.about.update.restart')}
        </Button>
      ) : (
        <UpdateControls
          state={state}
          busy={busy}
          onCheck={() => void check()}
          onDownload={() => void download()}
          onRestart={() => void restart()}
        />
      )}
      {leftover !== null && (
        <div className="space-y-1 pt-2" data-testid="update-leftover">
          <p className="text-2xs">{t('settings.about.update.leftover', { path: leftover })}</p>
          <Button variant="ghost" onClick={() => void removeLeftover()}>
            {t('settings.about.update.leftoverRemove')}
          </Button>
        </div>
      )}
      {/* Offered only while an update is waiting, because a migration is a
          reinstall and there is nothing to reinstall without one. The button
          exists for the copy whose every update needs a UAC prompt it cannot
          explain; moving it to the account ends that. */}
      {status?.canMigrateToUser === true && background !== null && (
        <div className="space-y-1 pt-2" data-testid="update-migrate">
          <p className="text-2xs">{t('settings.about.update.moveHint')}</p>
          {/* The move ends the Task Manager replacement, and only a copy in
              Program Files may name the hook — a per-account copy lives in
              the user's own folder, where the loader would start whatever an
              ordinary account put there. Said before the click, and only
              while the setting is actually on: warning about a feature that
              is already off is noise. */}
          {losesTaskManagerOnMove(status, taskManager.status) && (
            <p
              className="text-2xs text-[var(--color-warn-text)]"
              data-testid="update-migrate-taskmgr"
              role="status"
            >
              {t('settings.about.update.moveLosesTaskManager')}
            </p>
          )}
          <Button variant="ghost" onClick={() => void moveToThisAccount()}>
            {t('settings.about.update.move')}
          </Button>
        </div>
      )}
      {status !== null && statusReadAtMs !== null && (
        <UpdaterStatusLine status={status} readAtMs={statusReadAtMs} />
      )}
    </div>
  );
}

/**
 * One muted line about the background updater.
 *
 * Ordered by what the user can act on rather than by recency: a reason the
 * check is off beats an error, and an error beats the timestamp, because
 * "checked four minutes ago" is not the sentence someone needs when the
 * check is failing.
 */
function UpdaterStatusLine({
  status,
  readAtMs,
}: {
  readonly status: UpdateStatus;
  readonly readAtMs: number;
}) {
  const { t } = useTranslation();

  if (status.suppressed !== null) {
    return (
      <p className="text-2xs text-[var(--color-fg-muted)]" data-testid="updater-status">
        {t(`settings.about.update.suppressed.${status.suppressed}`)}
      </p>
    );
  }
  if (status.lastError !== null) {
    return (
      <p className="text-2xs text-[var(--color-fg-muted)]" data-testid="updater-status">
        {t('settings.about.update.lastCheckFailed', { message: status.lastError })}
      </p>
    );
  }
  if (status.lastCheckMs === 0) {
    return (
      <p className="text-2xs text-[var(--color-fg-muted)]" data-testid="updater-status">
        {t('settings.about.update.notCheckedYet')}
      </p>
    );
  }
  // Coarse on purpose: the first check is 20 s after launch and the rest are
  // twelve hours apart, so a minute count is as much detail as is ever true.
  const minutes = Math.max(0, Math.round((readAtMs - status.lastCheckMs) / 60_000));
  return (
    <p className="text-2xs text-[var(--color-fg-muted)]" data-testid="updater-status">
      {minutes < 60
        ? t('settings.about.update.checkedMinutesAgo', { count: minutes })
        : t('settings.about.update.checkedHoursAgo', { count: Math.round(minutes / 60) })}
    </p>
  );
}

function UpdateMessage({ state }: { readonly state: UpdateState }) {
  const { t } = useTranslation();

  switch (state.kind) {
    case 'idle':
      return (
        <p className="text-2xs text-[var(--color-fg-muted)]">{t('settings.about.update.idle')}</p>
      );

    case 'checking':
      return (
        <p className="text-2xs text-[var(--color-fg-muted)]">
          {t('settings.about.update.checking')}
        </p>
      );

    case 'upToDate':
      return (
        <p className="text-2xs text-[var(--color-fg-muted)]">
          {t('settings.about.update.upToDate')}
        </p>
      );

    case 'available':
      return (
        <div className="space-y-1">
          <p className="text-2xs">
            {t('settings.about.update.available', { version: state.version })}
          </p>
          {state.notes !== null && (
            <details className="text-2xs text-[var(--color-fg-muted)]">
              <summary>{t('settings.about.update.notesTitle')}</summary>
              <p className="pt-1 whitespace-pre-wrap">{state.notes}</p>
            </details>
          )}
        </div>
      );

    case 'downloading':
      return (
        <div className="space-y-1">
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t('settings.about.update.downloading', { version: state.version })}
          </p>
          {/* An unknown size is an indeterminate bar, never a zero. */}
          {state.percent === null ? (
            <ProgressBar indeterminate label={t('settings.about.update.downloadProgress')} />
          ) : (
            <ProgressBar
              value={state.percent}
              label={t('settings.about.update.downloadProgress')}
              valueText={`${Math.round(state.percent)}%`}
            />
          )}
        </div>
      );

    case 'ready':
      return (
        <p className="text-2xs">{t('settings.about.update.ready', { version: state.version })}</p>
      );

    case 'installing':
      return (
        <p className="text-2xs text-[var(--color-fg-muted)]">
          {t('settings.about.update.installing', { version: state.version })}
        </p>
      );

    case 'error':
      return (
        <p className="text-2xs text-[var(--color-danger-text)]">
          {t('settings.about.update.error', { message: state.message })}
        </p>
      );

    case 'unconfigured':
      return (
        <div className="space-y-1">
          <p className="text-2xs">{t('settings.about.update.unconfigured')}</p>
          <p className="text-2xs text-[var(--color-fg-muted)]">
            {t('settings.about.update.unconfiguredHint')}
          </p>
        </div>
      );
  }
}

function UpdateControls({
  state,
  busy,
  onCheck,
  onDownload,
  onRestart,
}: {
  readonly state: UpdateState;
  readonly busy: boolean;
  readonly onCheck: () => void;
  readonly onDownload: () => void;
  readonly onRestart: () => void;
}) {
  const { t } = useTranslation();

  if (state.kind === 'available') {
    return <Button onClick={onDownload}>{t('settings.about.update.download')}</Button>;
  }

  if (state.kind === 'ready') {
    return <Button onClick={onRestart}>{t('settings.about.update.restart')}</Button>;
  }

  if (state.kind === 'downloading' || state.kind === 'installing') return null;

  // Nothing to offer when the build cannot check at all — a disabled button
  // invites clicking; a sentence explaining why does not.
  if (state.kind === 'unconfigured') return null;

  return (
    <Button variant="ghost" disabled={busy} onClick={onCheck}>
      {state.kind === 'error' ? t('settings.about.update.retry') : t('settings.about.checkUpdates')}
    </Button>
  );
}
