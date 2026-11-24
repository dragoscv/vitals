/**
 * The action builder: every agentq verb this page can run, in one place.
 *
 * # Why a builder instead of fifteen buttons
 *
 * Eleven of the verbs need at least one name the snapshot may not have seen —
 * a resource, a ticket, a branch, a clone path — and a button that only works
 * for the things already on screen would be less capable than the command line
 * it fronts. So the verb is picked, its fields appear, and the exact command
 * is shown back before anything runs.
 *
 * # What is guarded
 *
 * Verbs whose effect is not obvious from their name (`land`, `commit`, `break`,
 * `run`, `unlease`, `killJob`, and a sweep that is not a preview) ask once,
 * behind the same dialog, showing the same preview. Nothing here is
 * irreversible by accident: `agentq` itself refuses the cases that would be,
 * and the Rust planner validates the arguments again before the process starts.
 */

import * as m from 'motion/react-m';
import { AnimatePresence } from 'motion/react';
import { Play, Sparkles, Terminal, TriangleAlert } from 'lucide-react';
import { useId, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { AgentqAction, AgentsSnapshot } from '@vitals/protocol';
import { Badge, Button, DialogContent, DialogRoot, Input, Select, Switch, cn } from '@vitals/ui';

import {
  actionPreview,
  buildAction,
  initialFields,
  repoCandidates,
  suggestions,
  verbSpec,
  verbs,
  type FieldSpec,
  type Suggestion,
  type VerbId,
} from './model';
import { AGENTS_NS } from './strings';

export interface ActionsPanelProps {
  readonly snapshot: AgentsSnapshot;
  /** True while any action is in flight, so the form cannot be double-sent. */
  readonly busy: boolean;
  readonly onRun: (action: AgentqAction) => Promise<boolean>;
}

/** How loud a verb is: what it can cost if it turns out to be the wrong one. */
const TONE_RING: Record<'safe' | 'careful' | 'danger', string> = {
  safe: 'border-[var(--color-border-default)]',
  careful: 'border-[var(--color-status-warn)]/50',
  danger: 'border-[var(--color-danger-text)]/50',
};

const TONE_CHIP: Record<'safe' | 'careful' | 'danger', string> = {
  safe: 'text-[var(--color-fg-muted)]',
  careful: 'text-[var(--color-warn-text)]',
  danger: 'text-[var(--color-danger-text)]',
};

export function ActionsPanel({ snapshot, busy, onRun }: ActionsPanelProps): React.JSX.Element {
  const { t } = useTranslation(AGENTS_NS);
  const [verb, setVerb] = useState<VerbId>('propose');
  const [fields, setFields] = useState<Record<string, string>>(() => initialFields('propose'));
  const [confirming, setConfirming] = useState(false);
  const datalistId = useId();

  const spec = verbSpec(verb);
  const action = useMemo(() => buildAction(verb, fields), [verb, fields]);
  const preview = action === null ? null : actionPreview(action);

  const choices = useMemo(
    () => ({
      resource: snapshot.resources.map((r) => r.name),
      ticket: snapshot.resources.flatMap((r) => r.tickets.map((x) => x.id)),
      path: snapshot.leases.map((l) => l.path).filter((p): p is string => p !== null),
      leaseId: snapshot.leases.map((l) => l.leaseId),
      job: snapshot.jobs.map((j) => j.id),
      repo: repoCandidates(snapshot),
    }),
    [snapshot],
  );

  const set = (key: string, value: string): void => {
    setFields((f) => ({ ...f, [key]: value }));
  };

  function pick(next: VerbId): void {
    setVerb(next);
    setFields(initialFields(next));
  }

  function prefill(s: Suggestion): void {
    const a = s.action;
    switch (a.kind) {
      case 'propose':
        pick('propose');
        setFields({ reason: a.reason ?? '' });
        break;
      case 'sweep':
        pick('sweep');
        setFields({ dryRun: 'false' });
        break;
      case 'bump':
        pick('bump');
        setFields({ ticket: a.ticket ?? '', by: String(a.by), reason: a.reason });
        break;
      case 'clear':
        pick('clear');
        setFields({ resource: a.resource, reason: a.reason });
        break;
      default:
        break;
    }
  }

  async function run(): Promise<void> {
    if (action === null) return;
    if (spec.confirm && !confirming) {
      setConfirming(true);
      return;
    }
    const ok = await onRun(action);
    setConfirming(false);
    // The fields are cleared only on success: a refusal leaves what was typed
    // in place, so the reason it was refused can be corrected rather than
    // retyped.
    if (ok) setFields(initialFields(verb));
  }

  const list = suggestions(snapshot, snapshot.takenAtMs);

  return (
    <div className="flex flex-col gap-4">
      <AnimatePresence initial={false}>
        {list.length > 0 && (
          <m.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: 'auto' }}
            exit={{ opacity: 0, height: 0 }}
            transition={{ duration: 0.2 }}
            className="overflow-hidden"
          >
            <div className="flex flex-wrap items-center gap-2">
              <span className="inline-flex items-center gap-1 text-2xs text-[var(--color-fg-muted)]">
                <Sparkles aria-hidden className="size-3.5 text-[var(--color-accent)]" />
                {t('suggest.title')}
              </span>
              {list.map((s) => (
                <Button
                  key={s.id}
                  variant="ghost"
                  size="sm"
                  onClick={() => prefill(s)}
                  className="rounded-full border border-[var(--color-border-default)]"
                >
                  {t(`suggest.${s.id}`, s.params)}
                </Button>
              ))}
            </div>
          </m.div>
        )}
      </AnimatePresence>

      <div className="flex flex-wrap gap-1.5" role="group" aria-label={t('builder.verbs')}>
        {verbs.map((v) => (
          <button
            key={v.id}
            type="button"
            aria-pressed={v.id === verb}
            onClick={() => pick(v.id)}
            className={cn(
              'rounded-[var(--radius-control)] border px-2.5 py-1 text-2xs transition-colors',
              'duration-(--duration-fast) hover:bg-[var(--color-bg-subtle)]',
              TONE_RING[v.tone],
              v.id === verb
                ? 'bg-[var(--color-bg-subtle)] font-medium text-[var(--color-fg)]'
                : TONE_CHIP[v.tone],
            )}
          >
            {t(v.labelKey)}
          </button>
        ))}
      </div>

      <div className="flex flex-col gap-3 rounded-xl border border-[var(--color-border-default)] bg-[var(--color-bg-raised)] p-3">
        <div className="flex flex-wrap items-baseline justify-between gap-2">
          <h3 className="text-sm font-medium">{t('builder.title')}</h3>
          <Badge
            tone={spec.tone === 'danger' ? 'danger' : spec.tone === 'careful' ? 'warn' : 'neutral'}
          >
            {t(`tone.${spec.tone}`)}
          </Badge>
        </div>
        <p className="text-2xs text-[var(--color-fg-muted)]">{t(spec.hintKey)}</p>

        <AnimatePresence mode="wait" initial={false}>
          <m.div
            key={verb}
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -4 }}
            transition={{ duration: 0.16 }}
            className="grid grid-cols-1 gap-3 sm:grid-cols-2"
          >
            {spec.fields.map((field) => (
              <Field
                key={field.key}
                field={field}
                value={fields[field.key] ?? ''}
                listId={`${datalistId}-${field.key}`}
                onChange={(v) => set(field.key, v)}
              />
            ))}
          </m.div>
        </AnimatePresence>

        <datalist id={`${datalistId}-resource`}>
          {choices.resource.map((v) => (
            <option key={v} value={v} />
          ))}
        </datalist>
        <datalist id={`${datalistId}-ticket`}>
          {choices.ticket.map((v) => (
            <option key={v} value={v} />
          ))}
        </datalist>
        <datalist id={`${datalistId}-path`}>
          {choices.path.map((v) => (
            <option key={v} value={v} />
          ))}
        </datalist>
        <datalist id={`${datalistId}-leaseId`}>
          {choices.leaseId.map((v) => (
            <option key={v} value={v} />
          ))}
        </datalist>
        <datalist id={`${datalistId}-job`}>
          {choices.job.map((v) => (
            <option key={v} value={v} />
          ))}
        </datalist>
        <datalist id={`${datalistId}-repo`}>
          {choices.repo.map((v) => (
            <option key={v} value={v} />
          ))}
        </datalist>

        <div
          className={cn(
            'flex items-start gap-2 rounded-[var(--radius-control)] border bg-[var(--color-bg-inset)] p-2',
            TONE_RING[spec.tone],
          )}
        >
          <Terminal
            aria-hidden
            className="mt-0.5 size-3.5 shrink-0 text-[var(--color-fg-subtle)]"
          />
          <code className="min-w-0 font-mono text-2xs break-all text-[var(--color-fg-muted)]">
            {preview ?? t('builder.incomplete')}
          </code>
        </div>

        <div className="flex items-center justify-between gap-3">
          <span className="inline-flex items-center gap-1 text-2xs text-[var(--color-fg-subtle)]">
            {spec.confirm && <TriangleAlert aria-hidden className="size-3.5" />}
            {spec.confirm ? t('builder.confirmHint') : t('builder.directHint')}
          </span>
          <Button disabled={busy || action === null} onClick={() => void run()}>
            <Play aria-hidden className="size-4" />
            {t('action.run')}
          </Button>
        </div>
      </div>

      <DialogRoot open={confirming} onOpenChange={(open) => !open && setConfirming(false)}>
        {confirming && (
          <DialogContent
            title={t(spec.labelKey)}
            closeLabel={t('action.cancel')}
            footer={
              <div className="flex justify-end gap-2">
                <Button variant="ghost" onClick={() => setConfirming(false)}>
                  {t('action.cancel')}
                </Button>
                <Button disabled={busy} onClick={() => void run()}>
                  {t('action.run')}
                </Button>
              </div>
            }
          >
            <p className="text-2xs text-[var(--color-fg-muted)]">{t(spec.hintKey)}</p>
            <code className="mt-2 block rounded-[var(--radius-control)] border border-[var(--color-border)] bg-[var(--color-bg-inset)] p-2 font-mono text-2xs break-all">
              {preview}
            </code>
          </DialogContent>
        )}
      </DialogRoot>
    </div>
  );
}

/**
 * One field of the form.
 *
 * A data-driven switch rather than a component per kind: the verb catalogue in
 * `model.ts` already names the kind, and keeping the rendering beside it means
 * a new kind is a compile error here instead of a field that silently never
 * appears.
 */
function Field({
  field,
  value,
  listId,
  onChange,
}: {
  readonly field: FieldSpec;
  readonly value: string;
  readonly listId: string;
  readonly onChange: (value: string) => void;
}): React.JSX.Element {
  const { t } = useTranslation(AGENTS_NS);
  const hint = field.helpKey === undefined ? undefined : t(field.helpKey);
  const className = field.wide === true ? 'sm:col-span-2' : undefined;

  if (field.kind === 'toggle') {
    return (
      <div className={className}>
        <Switch
          checked={value !== 'false'}
          onCheckedChange={(on) => onChange(on ? 'true' : 'false')}
          label={t(field.labelKey)}
          {...(hint !== undefined && { description: hint })}
        />
      </div>
    );
  }

  if (field.kind === 'select') {
    return (
      <div className={cn('flex flex-col gap-1', className)}>
        <span className="text-2xs font-medium text-[var(--color-fg-muted)]">
          {t(field.labelKey)}
        </span>
        <Select
          value={value}
          onValueChange={onChange}
          options={(field.options ?? []).map((o) => ({
            value: o,
            label: o === '' ? t('field.unset') : o,
          }))}
        />
      </div>
    );
  }

  if (field.kind === 'area') {
    const id = `${listId}-area`;
    return (
      <div className={cn('flex flex-col gap-1', className)}>
        <label htmlFor={id} className="text-2xs font-medium text-[var(--color-fg-muted)]">
          {t(field.labelKey)}
        </label>
        <textarea
          id={id}
          rows={field.rows ?? 3}
          value={value}
          spellCheck={false}
          onChange={(e) => onChange(e.target.value)}
          {...(hint !== undefined && { 'aria-describedby': `${id}-hint` })}
          className={cn(
            'resize-y rounded-[var(--radius-control)] border border-[var(--color-border)]',
            'bg-[var(--color-bg-inset)] p-2 font-mono text-2xs',
            'focus-visible:ring-2 focus-visible:ring-[var(--color-accent)] focus-visible:outline-none',
          )}
        />
        {hint !== undefined && (
          <p id={`${id}-hint`} className="text-2xs text-[var(--color-fg-subtle)]">
            {hint}
          </p>
        )}
      </div>
    );
  }

  return (
    <Input
      label={t(field.labelKey)}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      {...(field.placeholder !== undefined && { placeholder: field.placeholder })}
      {...(hint !== undefined && { hint })}
      {...(field.kind === 'number' && { inputMode: 'numeric' as const })}
      {...(className !== undefined && { className })}
      // The datalist is a convenience, never a constraint: a name that is not
      // in the snapshot yet is exactly the case a free-text field is for.
      {...(field.kind === 'text' && { list: listId })}
    />
  );
}
