/**
 * The resume pack: what a session is carrying, so another machine or window can
 * pick the work up from this page alone.
 *
 * It draws the plan `track.mjs` keeps for the session in hand — its goal, the
 * steps still open — together with the tasks this session and its subagents
 * have claimed. The tracker index above it answers "what do repositories say";
 * this answers "what is this session doing", which is the fact a resume needs
 * and the transcript cannot give across a context reset.
 *
 * # The page's one write
 *
 * A task row is a button where this session may act: **Claim** on an open task
 * nobody holds, **Complete** on one it holds itself. It is the only control on
 * the page that writes, and it writes the task store alone — never a tracker
 * file (ADR-0051). The claim button is disabled, with the reason shown, when
 * there is no session to claim as; a claim attributed to nobody would be worse
 * than no claim at all. No button is drawn where the store would refuse the
 * action anyway, so nothing on this page is a control that cannot work.
 */

import { Bot, Check, Hand, ListTree, Target } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  Card,
  CardBody,
  CardHeader,
  CardTitle,
  Separator,
  Skeleton,
} from '@vitals/ui';

import {
  isKnownTaskStatus,
  openNextSteps,
  sessionClaims,
  statusTone,
  taskOpen,
  taskTone,
  type PlanNextStep,
  type PlanTask,
  type ResumePack,
} from './model';
import { TRACKING_NS } from './strings';
import type { ClaimTaskRequest } from './useTracking';

export interface ResumePackPanelProps {
  /** The pack, or `null` until the first read resolves. */
  readonly pack: ResumePack | null;
  /** A failed read, so the panel says so rather than looking empty. */
  readonly error: string | null;
  /** The id of the task a claim is in flight for, or `null`. */
  readonly pendingClaim: string | null;
  /** The reason the last claim was refused, or `null`. */
  readonly claimError: string | null;
  readonly claim: (request: ClaimTaskRequest) => Promise<void>;
}

export function ResumePackPanel({
  pack,
  error,
  pendingClaim,
  claimError,
  claim,
}: ResumePackPanelProps): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);

  if (error !== null) {
    return (
      <Card regionLabel={t('resume.title')} className="px-4 py-3">
        <p className="text-sm text-[var(--color-fg-muted)]">
          {t('resume.error', { message: error })}
        </p>
      </Card>
    );
  }

  if (pack === null) {
    return (
      <Card regionLabel={t('resume.title')}>
        <CardBody>
          <Skeleton className="h-24 w-full" />
        </CardBody>
      </Card>
    );
  }

  const plan = pack.plan;
  if (plan === null) {
    return (
      <Card regionLabel={t('resume.title')} className="px-4 py-3">
        <div className="flex items-start gap-2">
          <Target aria-hidden className="mt-0.5 size-4 text-[var(--color-fg-subtle)]" />
          <div>
            <p className="text-sm text-[var(--color-fg-default)]">{t('resume.noPlan.title')}</p>
            <p className="text-2xs text-[var(--color-fg-muted)]">{t('resume.noPlan.body')}</p>
          </div>
        </div>
      </Card>
    );
  }

  const next = openNextSteps(plan);
  const claims = sessionClaims(pack.tasks, plan, pack.sessionId);
  // What nobody holds and nobody has finished: the work this page can hand out.
  // Listed even with no session, so the reason it cannot be claimed is shown
  // rather than the control quietly disappearing.
  const claimable = pack.tasks.filter((task) => task.owner === undefined && taskOpen(task));
  const writable = pack.sessionId !== null;

  return (
    <Card regionLabel={t('resume.title')}>
      <CardHeader
        actions={
          writable ? (
            <Badge tone="accent">{t('resume.session', { id: pack.sessionId })}</Badge>
          ) : (
            <Badge tone="warn">{t('resume.disabled.noSession')}</Badge>
          )
        }
      >
        <CardTitle
          level={3}
          className="flex items-center gap-2 text-sm tracking-normal normal-case"
        >
          <Target aria-hidden className="size-4 text-[var(--color-accent)]" />
          {t('resume.title')}
        </CardTitle>
      </CardHeader>

      <CardBody className="flex flex-col gap-3">
        <div>
          <h4 className="text-2xs font-semibold tracking-[0.06em] text-[var(--color-fg-subtle)] uppercase">
            {t('resume.goal')}
          </h4>
          <p className="mt-0.5 text-sm text-[var(--color-fg-default)]">{plan.goal}</p>
        </div>

        <NextSteps steps={next} />

        <Separator />

        <TaskGroup
          label={t('resume.claims')}
          empty={t('resume.claimsEmpty')}
          tasks={claims}
          sessionId={pack.sessionId}
          pendingClaim={pendingClaim}
          onClaim={(id) => claim({ id, action: 'claim', by: pack.sessionId ?? '' })}
          onComplete={(id) => claim({ id, action: 'complete', by: pack.sessionId ?? '' })}
        />

        <TaskGroup
          label={t('resume.available')}
          empty={t('resume.availableEmpty')}
          tasks={claimable}
          sessionId={pack.sessionId}
          pendingClaim={pendingClaim}
          onClaim={(id) => claim({ id, action: 'claim', by: pack.sessionId ?? '' })}
          onComplete={(id) => claim({ id, action: 'complete', by: pack.sessionId ?? '' })}
        />

        {plan.roster.length > 0 && (
          <>
            <Separator />
            <div className="flex flex-col gap-1">
              <h4 className="text-2xs font-semibold tracking-[0.06em] text-[var(--color-fg-subtle)] uppercase">
                {t('resume.roster')}
              </h4>
              <ul className="flex flex-col divide-y divide-[var(--color-border-subtle)]">
                {plan.roster.map((entry, i) => (
                  <li
                    key={`${entry.agentName}-${String(i)}`}
                    className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 py-1.5"
                  >
                    <Bot aria-hidden className="size-3.5 text-[var(--color-fg-subtle)]" />
                    <span className="text-2xs font-medium text-[var(--color-fg-default)]">
                      {entry.agentName}
                    </span>
                    {entry.purpose !== '' && (
                      <span className="min-w-0 flex-1 text-2xs text-[var(--color-fg-muted)]">
                        {entry.purpose}
                      </span>
                    )}
                    {entry.status !== '' && (
                      <Badge tone={statusTone(entry.status)}>{entry.status}</Badge>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          </>
        )}

        {claimError !== null && (
          <p role="alert" className="text-2xs text-[var(--color-danger-text)]">
            {t('resume.claimError', { message: claimError })}
          </p>
        )}
      </CardBody>
    </Card>
  );
}

/** The plan steps that are not yet done — what a resume would do next. */
function NextSteps({ steps }: { readonly steps: readonly PlanNextStep[] }): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  if (steps.length === 0) {
    return <p className="text-2xs text-[var(--color-fg-subtle)]">{t('resume.nextEmpty')}</p>;
  }
  return (
    <div className="flex flex-col gap-1">
      <h4 className="flex items-center gap-1.5 text-2xs font-semibold tracking-[0.06em] text-[var(--color-fg-subtle)] uppercase">
        <ListTree aria-hidden className="size-3" />
        {t('resume.next')}
      </h4>
      <ul className="flex flex-col divide-y divide-[var(--color-border-subtle)]">
        {steps.map((step, i) => (
          <li
            key={`${step.title}-${String(i)}`}
            className="flex flex-wrap items-baseline gap-x-2 gap-y-0.5 py-1.5"
          >
            <span className="min-w-0 flex-1 text-2xs text-[var(--color-fg-default)]">
              {step.title}
            </span>
            {step.owner !== undefined && (
              <span className="text-2xs text-[var(--color-fg-subtle)]">{step.owner}</span>
            )}
            {step.status !== '' && <Badge tone={statusTone(step.status)}>{step.status}</Badge>}
          </li>
        ))}
      </ul>
    </div>
  );
}

/** One labelled list of tasks, each row carrying the action it may take. */
function TaskGroup({
  label,
  empty,
  tasks,
  sessionId,
  pendingClaim,
  onClaim,
  onComplete,
}: {
  readonly label: string;
  readonly empty: string;
  readonly tasks: readonly PlanTask[];
  readonly sessionId: string | null;
  readonly pendingClaim: string | null;
  readonly onClaim: (id: string) => void;
  readonly onComplete: (id: string) => void;
}): React.JSX.Element {
  return (
    <div className="flex flex-col gap-1">
      <h4 className="text-2xs font-semibold tracking-[0.06em] text-[var(--color-fg-subtle)] uppercase">
        {label}
      </h4>
      {tasks.length === 0 ? (
        <p className="text-2xs text-[var(--color-fg-subtle)]">{empty}</p>
      ) : (
        <ul className="flex flex-col divide-y divide-[var(--color-border-subtle)]">
          {tasks.map((task) => (
            <TaskRow
              key={task.id}
              task={task}
              sessionId={sessionId}
              pending={pendingClaim === task.id}
              onClaim={onClaim}
              onComplete={onComplete}
            />
          ))}
        </ul>
      )}
    </div>
  );
}

function TaskRow({
  task,
  sessionId,
  pending,
  onClaim,
  onComplete,
}: {
  readonly task: PlanTask;
  readonly sessionId: string | null;
  readonly pending: boolean;
  readonly onClaim: (id: string) => void;
  readonly onComplete: (id: string) => void;
}): React.JSX.Element {
  const { t } = useTranslation(TRACKING_NS);
  const open = taskOpen(task);
  // Held by this session: the one task it may finish. `mine` implies a session,
  // so this button is never the disabled one.
  const mine = sessionId !== null && task.owner === sessionId;
  // Unowned: the store accepts a claim for it from any session. A task held by
  // a subagent is deliberately not offered — `track.mjs` refuses a claim whose
  // owner is someone else, and a button that cannot succeed is a lie.
  const free = task.owner === undefined;
  const writable = sessionId !== null;
  const reason = t('resume.disabled.noSession');

  return (
    <li className="flex flex-wrap items-center gap-x-2 gap-y-0.5 py-1.5">
      <span className="font-mono text-2xs text-[var(--color-fg-subtle)]">{task.id}</span>
      <span className="min-w-0 flex-1 text-2xs text-[var(--color-fg-default)]">{task.title}</span>
      {task.owner !== undefined && (
        <span className="text-2xs text-[var(--color-fg-muted)]">{task.owner}</span>
      )}
      <Badge tone={taskTone(task.status)}>
        {isKnownTaskStatus(task.status) ? t(`status.${task.status}`) : task.status}
      </Badge>
      {open && mine && (
        <Button
          variant="secondary"
          size="sm"
          loading={pending}
          loadingLabel={t('resume.working')}
          onClick={() => {
            onComplete(task.id);
          }}
        >
          <Check aria-hidden className="size-3.5" />
          {t('resume.complete')}
        </Button>
      )}
      {open && free && (
        <Button
          variant="secondary"
          size="sm"
          loading={pending}
          loadingLabel={t('resume.working')}
          disabled={!writable}
          // `exactOptionalPropertyTypes` is on: the reason is added only when
          // there is one to show, never passed as `undefined`.
          {...(!writable ? { title: reason } : {})}
          onClick={() => {
            onClaim(task.id);
          }}
        >
          <Hand aria-hidden className="size-3.5" />
          {t('resume.claim')}
        </Button>
      )}
      {open && free && !writable && (
        <span className="text-2xs text-[var(--color-fg-subtle)]">{reason}</span>
      )}
    </li>
  );
}
