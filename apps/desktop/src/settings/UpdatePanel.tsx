/**
 * The update section of the About panel.
 *
 * Deliberately shows one sentence and at most one button per state. An update
 * dialog that offers "check", "download" and "restart" simultaneously makes
 * the user reason about which one applies; the state machine already knows,
 * so the UI just renders its answer.
 */

import { useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { Button, ProgressBar } from '@vitals/ui';

import {
  checkForUpdate,
  downloadAndInstall,
  restartIntoUpdate,
  type UpdateState,
} from '../lib/updater';

/** What `checkForUpdate` hands back, kept only for the duration of one flow. */
type PendingUpdate = Awaited<ReturnType<typeof checkForUpdate>>['update'];

export function UpdateSection() {
  const { t } = useTranslation();
  const [state, setState] = useState<UpdateState>({ kind: 'idle' });

  // The handle is not state: nothing renders from it, and putting it in
  // `useState` would re-render the panel on every progress tick for a value
  // no element reads.
  const pending = useRef<PendingUpdate>(null);

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
    const failure = await restartIntoUpdate();
    // `null` means the process is on its way out; leave the installing state
    // on screen rather than flashing something else during teardown.
    if (failure !== null) setState(failure);
  }

  return (
    <div className="space-y-2 py-2" data-testid="update-section">
      <p className="text-sm font-medium">{t('settings.about.update.title')}</p>
      <UpdateMessage state={state} />
      <UpdateControls
        state={state}
        busy={busy}
        onCheck={() => void check()}
        onDownload={() => void download()}
        onRestart={() => void restart()}
      />
    </div>
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
        <p className="text-2xs text-[var(--color-status-danger)]">
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
