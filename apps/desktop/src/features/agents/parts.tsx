/**
 * Small building blocks for the AI page: KPI tiles, state chips, client
 * marks, the slot strip and the per-hour sparkline.
 *
 * Motion comes from `m.*` under the app's LazyMotion (ADR-0017); reduced
 * motion is honoured by the shared MotionConfig, and every animated value
 * also has a static accessible label.
 */

import * as m from 'motion/react-m';
import { AnimatePresence } from 'motion/react';
import { Bot, Code2, Cpu, Sparkles, Terminal, type LucideIcon } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import type { AgentClient, Liveness, SessionState } from '@vitals/protocol';
import { AnimatedValue, cn } from '@vitals/ui';

import { AGENTS_NS } from './strings';

export const CLIENT_ICON: Record<AgentClient, LucideIcon> = {
  vscodeCopilot: Code2,
  copilotCli: Terminal,
  codai: Sparkles,
  codex: Cpu,
  claudeCode: Bot,
  other: Bot,
};

/** Hue per client so a mixed list scans by colour as well as by icon. */
export const CLIENT_HUE: Record<AgentClient, string> = {
  vscodeCopilot: 'oklch(0.68 0.15 245)',
  copilotCli: 'oklch(0.70 0.14 300)',
  codai: 'oklch(0.72 0.16 160)',
  codex: 'oklch(0.72 0.13 80)',
  claudeCode: 'oklch(0.70 0.14 40)',
  other: 'oklch(0.65 0.02 260)',
};

const STATE_TONE: Record<SessionState, string> = {
  working: 'var(--color-status-ok)',
  waiting: 'var(--color-status-warn)',
  idle: 'var(--color-accent)',
  ended: 'var(--color-fg-subtle)',
};

/**
 * A state chip that morphs between states: the label cross-fades and the
 * pill re-tints in place instead of the row re-rendering a new badge.
 */
export function StateChip({ state }: { readonly state: SessionState }) {
  const { t } = useTranslation(AGENTS_NS);
  const tone = STATE_TONE[state];
  return (
    <m.span
      className="relative inline-flex h-6 items-center gap-1.5 overflow-hidden rounded-full border px-2.5 text-2xs font-medium"
      animate={{ borderColor: tone, color: tone }}
      transition={{ duration: 0.35 }}
    >
      <span className="relative flex size-2" aria-hidden>
        {state === 'working' && (
          <m.span
            className="absolute inset-0 rounded-full"
            style={{ background: tone }}
            animate={{ scale: [1, 2.2], opacity: [0.6, 0] }}
            transition={{ duration: 1.4, repeat: Infinity, ease: 'easeOut' }}
          />
        )}
        <span className="relative size-2 rounded-full" style={{ background: tone }} />
      </span>
      <AnimatePresence mode="popLayout" initial={false}>
        <m.span
          key={state}
          initial={{ y: 8, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          exit={{ y: -8, opacity: 0 }}
          transition={{ duration: 0.25 }}
        >
          {t(`state.${state}`)}
        </m.span>
      </AnimatePresence>
    </m.span>
  );
}

const LIVENESS_TONE: Record<Liveness, string> = {
  alive: 'var(--color-status-ok)',
  stale: 'var(--color-status-warn)',
  dead: 'var(--color-status-danger)',
  unknown: 'var(--color-fg-subtle)',
};

export function LivenessDot({ value }: { readonly value: Liveness }) {
  const { t } = useTranslation(AGENTS_NS);
  return (
    <span className="inline-flex items-center gap-1 text-2xs text-[var(--color-fg-muted)]">
      <span
        aria-hidden
        className="size-1.5 rounded-full"
        style={{ background: LIVENESS_TONE[value] }}
      />
      {t(`liveness.${value}`)}
    </span>
  );
}

export function ClientMark({
  client,
  size = 'md',
}: {
  readonly client: AgentClient;
  readonly size?: 'sm' | 'md';
}) {
  const Icon = CLIENT_ICON[client];
  const hue = CLIENT_HUE[client];
  return (
    <span
      aria-hidden
      className={cn(
        'inline-grid shrink-0 place-items-center rounded-lg',
        size === 'sm' ? 'size-6' : 'size-8',
      )}
      style={{ background: `color-mix(in oklch, ${hue} 18%, transparent)`, color: hue }}
    >
      <Icon className={size === 'sm' ? 'size-3.5' : 'size-4'} />
    </span>
  );
}

/** One headline number. The digits roll; the tile lifts on change. */
export function Kpi({
  label,
  value,
  tone = 'neutral',
  hint,
}: {
  readonly label: string;
  readonly value: number | string;
  readonly tone?: 'neutral' | 'ok' | 'warn' | 'danger' | 'accent';
  readonly hint?: string;
}) {
  const color = {
    neutral: 'var(--color-fg-default)',
    ok: 'var(--color-ok-text)',
    warn: 'var(--color-warn-text)',
    danger: 'var(--color-danger-text)',
    accent: 'var(--color-accent)',
  }[tone];
  return (
    <m.div
      className="flex min-w-[8.5rem] flex-1 flex-col gap-1 rounded-xl border border-[var(--color-border-subtle)] bg-[var(--surface-card)] px-3 py-2.5"
      initial={{ opacity: 0, y: 6 }}
      animate={{ opacity: 1, y: 0 }}
    >
      <span className="text-2xs text-[var(--color-fg-muted)]">{label}</span>
      <span className="text-xl font-semibold tabular-nums" style={{ color }}>
        <AnimatedValue value={String(value)} />
      </span>
      {hint !== undefined && <span className="text-2xs text-[var(--color-fg-subtle)]">{hint}</span>}
    </m.div>
  );
}

/**
 * Capacity as a strip of slots: filled = held, outlined = free, and a queue
 * of dots for waiters. Slots grow and shrink with the adaptive cap.
 */
export function SlotStrip({
  held,
  capacity,
  waiting,
  label,
}: {
  readonly held: number;
  readonly capacity: number;
  readonly waiting: number;
  readonly label: string;
}) {
  const slots = Array.from({ length: Math.max(capacity, held) }, (_, i) => i < held);
  return (
    <div className="flex items-center gap-1.5" role="img" aria-label={label}>
      <AnimatePresence initial={false}>
        {slots.map((filled, i) => (
          <m.span
            key={`s${String(i)}`}
            initial={{ scale: 0.4, opacity: 0 }}
            animate={{
              scale: 1,
              opacity: 1,
              backgroundColor: filled ? 'var(--color-accent)' : 'transparent',
            }}
            exit={{ scale: 0.4, opacity: 0 }}
            transition={{ type: 'spring', stiffness: 420, damping: 28 }}
            className="h-3 w-6 rounded-sm border border-[var(--color-accent)]"
          />
        ))}
      </AnimatePresence>
      {waiting > 0 && (
        <span className="ml-1 flex items-center gap-0.5" aria-hidden>
          {Array.from({ length: Math.min(waiting, 12) }, (_, i) => (
            <m.span
              key={`w${String(i)}`}
              className="size-1.5 rounded-full bg-[var(--color-status-warn)]"
              initial={{ opacity: 0, x: 6 }}
              animate={{ opacity: 1, x: 0 }}
              transition={{ delay: i * 0.03 }}
            />
          ))}
          {waiting > 12 && (
            <span className="text-2xs text-[var(--color-fg-muted)]">+{waiting - 12}</span>
          )}
        </span>
      )}
    </div>
  );
}

/** Bars for runs per hour; the newest hour is on the right. */
export function HourBars({
  values,
  label,
}: {
  readonly values: readonly number[];
  readonly label: string;
}) {
  const max = Math.max(1, ...values);
  return (
    <div className="flex h-16 items-end gap-[3px]" role="img" aria-label={label}>
      {values.map((v, i) => (
        <m.span
          key={`h${String(i)}`}
          className="flex-1 rounded-t-sm bg-[var(--color-accent)]"
          initial={{ height: 0 }}
          animate={{ height: `${Math.max(4, (v / max) * 100)}%`, opacity: v === 0 ? 0.25 : 1 }}
          transition={{ duration: 0.5, delay: i * 0.01 }}
        />
      ))}
    </div>
  );
}

/** A thin bar for "share of time active". */
export function ShareBar({ value, label }: { readonly value: number; readonly label: string }) {
  return (
    <div
      className="h-1.5 w-full overflow-hidden rounded-full bg-[var(--color-bg-subtle)]"
      role="meter"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(value * 100)}
    >
      <m.div
        className="h-full rounded-full bg-[var(--color-status-ok)]"
        initial={{ width: 0 }}
        animate={{ width: `${(value * 100).toFixed(1)}%` }}
        transition={{ duration: 0.6, ease: 'easeOut' }}
      />
    </div>
  );
}
