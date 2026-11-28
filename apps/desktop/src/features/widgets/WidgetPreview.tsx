/**
 * The one renderer a widget is drawn by.
 *
 * Both surfaces use it — the window and the configuration screen's preview —
 * because a preview that is a separate component is a preview that starts
 * lying the first time either side is edited. Everything it needs is in the
 * spec and the description, so it has no idea which window it is in.
 *
 * Motion comes from `m.*` under a `LazyMotion` with `domAnimation` (ADR-0017),
 * and the morph uses height and opacity only: layout animations need the
 * larger feature set, so a `layout` prop here would silently do nothing under
 * the runtime the app installs.
 *
 * Reduced motion is honoured twice over — `useReducedMotion` collapses the
 * transition duration, and `motion-reduce:` classes cover the CSS transitions
 * that never go through Motion at all.
 */

import { AnimatePresence, type Transition } from 'motion/react';
import * as m from 'motion/react-m';
import { useTranslation } from 'react-i18next';

import { AnimatedValue, cn, useReducedMotion } from '@vitals/ui';

import type { WidgetDescription, WidgetReading, WidgetTone } from './describe';
import type { WidgetInstance } from './model';

/** Each tone names the chart token it is drawn in, so a reading keeps its colour. */
const TONE_COLOUR: Record<WidgetTone, string> = {
  cpu: 'var(--color-chart-cpu)',
  memory: 'var(--color-chart-memory)',
  gpu: 'var(--color-chart-gpu)',
  disk: 'var(--color-chart-disk)',
  network: 'var(--color-chart-network)',
  thermal: 'var(--color-chart-thermal)',
  neutral: 'var(--color-chart-neutral)',
};

export interface WidgetPreviewProps {
  readonly spec: WidgetInstance;
  readonly description: WidgetDescription;
  /** Whether the readings past the lead one are shown. */
  readonly expanded: boolean;
  /**
   * A handle on the body, for the window only: the window puts its drag region
   * and its toolbar over the preview, and needs to know where the content ends.
   */
  readonly className?: string | undefined;
}

const W = 60;
const H = 18;

/**
 * How the compact-to-expanded change is animated.
 *
 * Zero, not a short spring: the state change is the information, and a
 * preference for less motion is a request for it to arrive without the travel.
 * Exported because "a widget respects the reduced-motion preference" is a
 * promise worth a test, and a transition object buried in a component is not
 * something one can assert.
 */
export function morphTransition(reduced: boolean): Transition {
  return reduced ? { duration: 0 } : { duration: 0.22, ease: [0.2, 0.8, 0.2, 1] };
}

/**
 * A trace beside a reading.
 *
 * Two scales, because the readings have two kinds of number. A percentage is
 * pinned to 0–100 so cpu, memory and gpu stay comparable at a glance; a
 * throughput has no ceiling, so it scales to its own window and a quiet
 * machine's line is still readable.
 */
function Trace({
  values,
  percent,
  stroke,
}: {
  readonly values: readonly number[];
  readonly percent: boolean;
  readonly stroke: string;
}): React.JSX.Element {
  if (values.length < 2) {
    return (
      // A blank strip reads as a rendering fault. The baseline says the trace
      // belongs here and is empty for now.
      <svg viewBox={`0 0 ${W} ${H}`} className="h-4 w-full" aria-hidden="true">
        <line
          x1="0"
          y1={H - 1}
          x2={W}
          y2={H - 1}
          stroke="var(--color-border-subtle)"
          strokeWidth="1"
          vectorEffect="non-scaling-stroke"
        />
      </svg>
    );
  }

  const ceiling = percent ? 100 : Math.max(1, ...values);
  const step = W / Math.max(values.length - 1, 1);
  const points = values
    .map((value, index) => {
      const clamped = Math.min(ceiling, Math.max(0, value));
      const x = index * step;
      const y = H - (clamped / ceiling) * (H - 2) - 1;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(' ');

  return (
    <svg
      viewBox={`0 0 ${W} ${H}`}
      className="h-4 w-full"
      aria-hidden="true"
      preserveAspectRatio="none"
    >
      <polyline
        points={points}
        fill="none"
        stroke={stroke}
        strokeWidth="1.25"
        strokeLinejoin="round"
        strokeLinecap="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

/** The lead reading drawn as a ring, for a layout that has room for one number. */
function Ring({
  percent,
  stroke,
  label,
  value,
}: {
  /** 0–100, or `null` when nothing has been measured yet. */
  readonly percent: number | null;
  readonly stroke: string;
  readonly label: string;
  readonly value: string;
}): React.JSX.Element {
  const r = 26;
  const circumference = 2 * Math.PI * r;
  // `null` draws the track and nothing else. An arc pinned at zero would be a
  // reading — "this machine is using none of it" — and the honest one is that
  // nothing has been measured yet, which the em dash in `value` already says.
  const filled = percent === null ? 0 : Math.min(100, Math.max(0, percent));
  return (
    <div className="relative grid size-[68px] shrink-0 place-items-center">
      <svg viewBox="0 0 64 64" className="size-[68px]" aria-hidden="true">
        <circle
          cx="32"
          cy="32"
          r={r}
          fill="none"
          stroke="var(--color-border-subtle)"
          strokeWidth="5"
        />
        <circle
          cx="32"
          cy="32"
          r={r}
          fill="none"
          stroke={stroke}
          strokeWidth="5"
          strokeLinecap="round"
          strokeDasharray={`${((filled / 100) * circumference).toFixed(2)} ${circumference.toFixed(2)}`}
          // Start at twelve o'clock rather than three, which is where every
          // other gauge in the product starts.
          transform="rotate(-90 32 32)"
        />
      </svg>
      <div className="absolute flex flex-col items-center">
        <span className="font-mono text-sm leading-none font-semibold tabular-nums">{value}</span>
        <span className="text-[9px] leading-tight text-[var(--color-fg-muted)]">{label}</span>
      </div>
    </div>
  );
}

/** One reading in a stacked layout: label, number, unit detail, trace. */
function Row({
  reading,
  label,
}: {
  readonly reading: WidgetReading;
  readonly label: string;
}): React.JSX.Element {
  const stroke = TONE_COLOUR[reading.tone];
  return (
    // A group per reading, so a screen reader announces "Memory, 12 GB of 32 GB"
    // as one thing rather than as four loose numbers in a list of six.
    <div className="flex items-center gap-2" role="group" aria-label={label}>
      <span className="w-12 shrink-0 truncate text-[10px] leading-none font-medium tracking-wide text-[var(--color-fg-muted)] uppercase">
        {label}
      </span>
      <span className="w-[3.5rem] shrink-0 text-right font-mono text-sm leading-none font-semibold tabular-nums">
        <AnimatedValue value={reading.value} />
      </span>
      <span className="w-10 shrink-0 text-right font-mono text-[10px] leading-none text-[var(--color-fg-muted)] tabular-nums">
        {reading.detail ?? ''}
      </span>
      {reading.trace.length > 1 ? (
        <div className="min-w-0 flex-1">
          <Trace values={reading.trace} percent={reading.percent !== null} stroke={stroke} />
        </div>
      ) : (
        <span className="min-w-0 flex-1" />
      )}
    </div>
  );
}

export function WidgetPreview({
  spec,
  description,
  expanded,
  className,
}: WidgetPreviewProps): React.JSX.Element {
  const { t } = useTranslation();
  const reduced = useReducedMotion();

  const title = description.titleKey === null ? null : t(description.titleKey);
  const lead = description.readings[0] ?? null;
  const rest = description.readings.slice(1);

  // Zero, not a short spring: the state change is the information, and a
  // preference for less motion is a request for it to arrive without the
  // travel.
  const morph = morphTransition(reduced);

  // The ring is drawn whenever the layout asks for it, even before the reading
  // has a percentage: a widget placed on a screen seconds after boot must show
  // "—" inside a gauge, not an empty panel that looks broken until the first
  // sample arrives.
  const body =
    spec.layout === 'ring' && lead !== null ? (
      <Ring
        percent={lead.percent}
        stroke={TONE_COLOUR[lead.tone]}
        label={lead.detail ?? t(lead.labelKey)}
        value={lead.value}
      />
    ) : null;

  return (
    <div
      className={cn(
        // rgb(…/0.92) rather than a backdrop filter: blur on a transparent
        // Windows webview is expensive and renders inconsistently over a
        // fullscreen game, which is exactly where an overlay earns its keep.
        'flex h-full w-full flex-col gap-1.5 overflow-hidden rounded-xl border px-3 py-2',
        'text-[var(--color-fg-default)] select-none',
        spec.style === 'glass'
          ? // 0.84, not 0.55, and measured rather than guessed: at 0.55 the
            // muted label over a white wallpaper came out at 1.64:1 and the
            // default text at 3.51:1 — both under WCAG AA. Muted text needs 0.84
            // and default text 0.63 to clear 4.5:1 over the worst case, so the
            // single value that keeps every label legible is the larger one.
            // A translucent panel can never be guaranteed against an arbitrary
            // wallpaper; `plain` is the style that is.
            'border-[rgb(255_255_255/0.16)] bg-[rgb(20_20_24/0.84)]'
          : 'border-[rgb(255_255_255/0.08)] bg-[rgb(20_20_24/0.92)]',
        'motion-reduce:transition-none',
        className,
      )}
      // The window itself is transparent, so the stored opacity is applied
      // here rather than by the window manager — one value, one meaning.
      style={{ opacity: spec.opacity }}
      role="group"
      {...(title !== null && { 'aria-label': title })}
    >
      {title !== null && (
        <span className="shrink-0 text-[10px] leading-none font-semibold tracking-wide text-[var(--color-fg-muted)]">
          {title}
        </span>
      )}

      {spec.layout === 'split' && lead !== null ? (
        <div className="flex min-h-0 flex-1 items-center gap-3">
          <div className="flex min-w-0 flex-col">
            <span className="truncate text-[10px] leading-none font-medium text-[var(--color-fg-muted)] uppercase">
              {t(lead.labelKey)}
            </span>
            <span className="font-mono text-2xl leading-tight font-semibold tabular-nums">
              <AnimatedValue value={lead.value} />
            </span>
            {lead.detail !== null && (
              <span className="truncate text-[10px] leading-tight text-[var(--color-fg-muted)]">
                {lead.detail}
              </span>
            )}
          </div>
          <div className="min-w-0 flex-1 self-end">
            <Trace
              values={lead.trace}
              percent={lead.percent !== null}
              stroke={TONE_COLOUR[lead.tone]}
            />
          </div>
        </div>
      ) : (
        lead !== null && (
          <div className="flex min-h-0 flex-1 flex-col justify-center gap-1.5">
            <div className="flex items-center gap-2">
              {spec.layout === 'stack' && (
                <span className="w-12 shrink-0 truncate text-[10px] leading-none font-medium tracking-wide text-[var(--color-fg-muted)] uppercase">
                  {t(lead.labelKey)}
                </span>
              )}
              {body}
              {spec.layout === 'stack' && (
                <>
                  <span className="w-[3.5rem] shrink-0 text-right font-mono text-sm leading-none font-semibold tabular-nums">
                    <AnimatedValue value={lead.value} />
                  </span>
                  <span className="w-10 shrink-0 text-right font-mono text-[10px] leading-none text-[var(--color-fg-muted)] tabular-nums">
                    {lead.detail ?? ''}
                  </span>
                  <div className="min-w-0 flex-1">
                    <Trace
                      values={lead.trace}
                      percent={lead.percent !== null}
                      stroke={TONE_COLOUR[lead.tone]}
                    />
                  </div>
                </>
              )}
            </div>

            {/* The morph: the panel grows from nothing rather than appearing,
                so the widget keeps its shape and the user keeps their place. */}
            <AnimatePresence initial={false}>
              {expanded && rest.length > 0 && (
                <m.div
                  key="readings"
                  initial={{ height: 0, opacity: 0 }}
                  animate={{ height: 'auto', opacity: 1 }}
                  exit={{ height: 0, opacity: 0 }}
                  transition={morph}
                  className="overflow-hidden"
                >
                  <div className="flex flex-col gap-1.5 pt-1">
                    {rest.map((reading) => (
                      <Row key={reading.id} reading={reading} label={t(reading.labelKey)} />
                    ))}
                  </div>
                </m.div>
              )}
            </AnimatePresence>
          </div>
        )
      )}
    </div>
  );
}
