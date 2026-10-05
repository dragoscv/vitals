import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { isVitalsError, type ControlRequest } from '@vitals/client';
import { Button } from '@vitals/ui';
import { displayName, type Process } from '@vitals/protocol';

/** What the sheet reports back after a request settled. */
export interface ActionOutcome {
  /** True when the host refused because the token cannot control. */
  readonly readOnly: boolean;
}

export interface ActionSheetProps {
  readonly process: Process;
  readonly control: (request: ControlRequest) => Promise<void>;
  readonly onClose: () => void;
  readonly onSettled: (outcome: ActionOutcome) => void;
  /** Set once a 403 has been seen for this machine; the destructive path is hidden. */
  readonly readOnly: boolean;
}

type Phase = 'idle' | 'arming' | 'working' | 'done';

/**
 * The bottom sheet for acting on a process.
 *
 * A phone has no hover, no right-click and no undo, and a mis-tap here ends
 * somebody's unsaved work. So "End task" arms a second, full-width button
 * that must be tapped again; the first tap can never terminate anything.
 * Suspend and resume are reversible and go through on one tap.
 */
export function ActionSheet({ process, control, onClose, onSettled, readOnly }: ActionSheetProps) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>('idle');
  const [message, setMessage] = useState<string | null>(null);

  async function send(request: ControlRequest): Promise<void> {
    setPhase('working');
    setMessage(null);
    try {
      await control(request);
      setPhase('done');
      onSettled({ readOnly: false });
      onClose();
    } catch (error) {
      setPhase('idle');
      // Both come back as 403, so read the body: `access-denied` is Windows
      // refusing THIS process (protected, or another user's), not the token.
      // Checking `forbidden` first marked the whole phone read-only after a
      // single failed kill of a system process, hiding every action forever.
      if (isVitalsError(error) && error.detail?.kind === 'access-denied') {
        setMessage(t('mobile.action.denied'));
        onSettled({ readOnly: false });
        return;
      }
      if (isVitalsError(error) && error.kind === 'forbidden') {
        setMessage(t('mobile.action.readOnlyBody'));
        onSettled({ readOnly: true });
        return;
      }
      if (isVitalsError(error) && error.detail?.kind === 'not-found') {
        setMessage(t('mobile.action.notFound'));
      } else {
        setMessage(t('mobile.action.failed'));
      }
      onSettled({ readOnly: false });
    }
  }

  const busy = phase === 'working';

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={t('mobile.action.title', { name: displayName(process) })}
      className="fixed inset-0 z-50 flex flex-col justify-end"
    >
      {/* Tap-outside-to-dismiss. Not a button: the visible Cancel below is
          the accessible path, and two controls named "Cancel" read as one
          duplicated control to a screen reader. */}
      <div
        aria-hidden="true"
        onClick={onClose}
        className="sheet-backdrop absolute inset-0 bg-black/40"
      />
      <div className="sheet-panel safe-bottom relative rounded-t-[var(--radius-widget)] border-t border-[var(--color-border-subtle)] bg-[var(--color-bg-raised)] p-4">
        <h2 className="truncate text-base font-semibold">
          {t('mobile.action.title', { name: displayName(process) })}
        </h2>

        {readOnly ? (
          <div className="mt-3 rounded-[var(--radius-control)] bg-[var(--color-bg-inset)] p-3">
            <p className="text-sm font-medium">{t('mobile.action.readOnly')}</p>
            <p className="mt-1 text-2xs text-[var(--color-fg-muted)]">
              {t('mobile.action.readOnlyBody')}
            </p>
          </div>
        ) : (
          <div className="mt-4 space-y-2">
            {phase === 'arming' ? (
              <>
                <Button
                  variant="danger"
                  size="lg"
                  fullWidth
                  loading={busy}
                  loadingLabel={t('mobile.action.working')}
                  onClick={() => void send({ action: 'terminate', key: process.key })}
                  className="min-h-[52px]"
                >
                  {t('mobile.action.confirmEnd', { name: displayName(process) })}
                </Button>
                <p className="text-center text-2xs text-[var(--color-fg-muted)]">
                  {t('mobile.action.confirmHint')}
                </p>
              </>
            ) : (
              <Button
                variant="danger"
                size="lg"
                fullWidth
                onClick={() => setPhase('arming')}
                className="min-h-[52px]"
              >
                {t('mobile.action.end')}
              </Button>
            )}

            {/* One of the pair, chosen by the process's current state.
                Offering "Resume" on something that is running is a button
                that can only fail, and on a phone every needless control is
                one more thing to mis-tap. */}
            {process.state === 'suspended' ? (
              <Button
                size="lg"
                fullWidth
                disabled={busy}
                onClick={() => void send({ action: 'resume', key: process.key })}
                className="min-h-[52px]"
              >
                {t('mobile.action.resume')}
              </Button>
            ) : (
              <Button
                size="lg"
                fullWidth
                disabled={busy}
                onClick={() => void send({ action: 'suspend', key: process.key })}
                className="min-h-[52px]"
              >
                {t('mobile.action.suspend')}
              </Button>
            )}
          </div>
        )}

        {message !== null && (
          <p role="alert" className="mt-3 text-2xs text-[var(--color-status-danger)]">
            {message}
          </p>
        )}

        <Button variant="ghost" size="lg" fullWidth onClick={onClose} className="mt-3 min-h-[52px]">
          {t('mobile.action.cancel')}
        </Button>
      </div>
    </div>
  );
}
