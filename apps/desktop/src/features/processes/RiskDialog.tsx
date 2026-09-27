/**
 * The confirmation that states what will actually happen.
 *
 * The design constraint is negative: a bugcheck warning and a "you will lose
 * unsaved work" warning must not be able to look the same. If they do, people
 * learn the shape of the dialog rather than its content and click through
 * both. So the risk drives the title, the tone, the wording of the confirm
 * button, and — for `forbidden` — whether there is a confirm button at all.
 */

import { useTranslation } from 'react-i18next';

import { Badge, Button, DialogContent, DialogRoot, type BadgeProps } from '@vitals/ui';

import type { ActionPlan, ActionRisk } from './actions';
import { fallback } from './strings';

export type RiskAction = 'terminate' | 'terminate-tree' | 'suspend';

export interface RiskRequest {
  readonly action: RiskAction;
  readonly plan: ActionPlan;
  readonly processName: string;
  /** Descendants that will also be ended. Zero for a single-process action. */
  readonly childCount: number;
  /**
   * The action was already attempted and Windows refused it for lack of
   * rights. The plain confirm would only fail again, so it is replaced by
   * the administrator retry.
   */
  readonly denied?: boolean;
}

export interface RiskDialogProps {
  readonly request: RiskRequest | null;
  readonly onCancel: () => void;
  readonly onConfirm: () => void;
  /** Present only when the caller can actually re-run the action elevated. */
  readonly onElevate?: () => void;
  readonly busy?: boolean;
}

const TONE: Readonly<Record<ActionRisk, NonNullable<BadgeProps['tone']>>> = {
  safe: 'neutral',
  disruptive: 'warn',
  critical: 'danger',
  forbidden: 'danger',
};

export function RiskDialog({
  request,
  onCancel,
  onConfirm,
  onElevate,
  busy = false,
}: RiskDialogProps): React.JSX.Element {
  const { t } = useTranslation();

  const open = request !== null;
  const risk = request?.plan.risk ?? 'safe';
  const blocked = risk === 'forbidden';
  const denied = request?.denied === true;

  // Elevation is offered only when the backend says it could help. For a
  // protected process it cannot, and Task Manager's "try again as
  // administrator" there is a dead end that teaches people the prompt is
  // meaningless. We would rather show one honest sentence and no button.
  const canElevate =
    !blocked && request?.plan.elevationMightHelp === true && onElevate !== undefined;

  const title = ((): string => {
    if (request === null) return '';
    if (request.action === 'suspend') {
      return t('process.confirm.suspendTitle', fallback('process.confirm.suspendTitle'), {
        name: request.processName,
      });
    }
    if (request.action === 'terminate-tree' && request.childCount > 0) {
      return t('process.confirm.treeTitle', fallback('process.confirm.treeTitle'), {
        name: request.processName,
        count: request.childCount,
      });
    }
    return t('process.confirm.endTitle', { name: request.processName });
  })();

  return (
    <DialogRoot
      open={open}
      onOpenChange={(next) => {
        if (!next) onCancel();
      }}
    >
      <DialogContent
        size="sm"
        title={title}
        closeLabel={t('common.close')}
        data-testid="risk-dialog"
        data-risk={risk}
        footer={
          <>
            <Button variant="ghost" onClick={onCancel}>
              {blocked ? t('common.close') : t('common.cancel')}
            </Button>
            {canElevate && (
              <Button
                variant={denied ? 'danger' : 'secondary'}
                onClick={onElevate}
                {...(denied && { loading: busy, loadingLabel: t('common.loading') })}
                data-testid="risk-elevate"
              >
                {t('process.confirm.elevate', fallback('process.confirm.elevate'))}
              </Button>
            )}
            {!blocked && !denied && (
              <Button
                variant="danger"
                loading={busy}
                loadingLabel={t('common.loading')}
                onClick={onConfirm}
                data-testid="risk-confirm"
              >
                {request?.action === 'suspend'
                  ? t('process.confirm.proceedSuspend', fallback('process.confirm.proceedSuspend'))
                  : t('process.confirm.proceed', fallback('process.confirm.proceed'))}
              </Button>
            )}
          </>
        }
      >
        <div className="space-y-3">
          <Badge tone={TONE[risk]} data-testid="risk-badge">
            {t(`process.risk.${risk}`, fallback(`process.risk.${risk}` as never))}
          </Badge>

          {/* The consequence comes from the backend as a translation key, so
              the specific sentence — "this closes your desktop" versus "this
              will crash Windows immediately" — is decided by the code that
              actually knows which process this is. */}
          <p className="text-sm text-[var(--color-fg-default)]">
            {request === null ? '' : t(request.plan.consequence)}
          </p>
          {denied && (
            <p role="status" className="text-sm text-[var(--color-status-warn)]">
              {t('process.confirm.denied', fallback('process.confirm.denied'))}
            </p>
          )}
        </div>
      </DialogContent>
    </DialogRoot>
  );
}
