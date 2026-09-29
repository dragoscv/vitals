/**
 * The handful of building blocks every screen uses. Deliberately plain: a TV
 * is read from three metres away, so the design is large type, generous
 * spacing and one unmistakable focus ring, not visual detail.
 */

import type { ReactNode } from 'react';

import { DASH } from '../lib/format';
import { Palette, levelColour, type Level } from './theme';

/** A circular gauge. `fraction` is 0–1, or `null` for unmeasured (drawn as an empty track). */
export function Ring({
  fraction,
  colour,
  value,
  label,
  level = 'ok',
  size = 'large',
}: {
  fraction: number | null;
  colour: string;
  value: string;
  label: string;
  level?: Level;
  size?: 'large' | 'small';
}) {
  const r = 42;
  const c = 2 * Math.PI * r;
  const f = fraction === null || Number.isNaN(fraction) ? null : Math.min(1, Math.max(0, fraction));
  const stroke = levelColour(level, colour);
  return (
    <figure className={`ring ring-${size}`} aria-label={`${label}: ${value}`}>
      <svg viewBox="0 0 100 100" aria-hidden="true">
        <circle cx="50" cy="50" r={r} className="ring-track" />
        {f !== null && (
          <circle
            cx="50"
            cy="50"
            r={r}
            className="ring-value"
            stroke={stroke}
            strokeDasharray={`${f * c} ${c}`}
            transform="rotate(-90 50 50)"
          />
        )}
      </svg>
      <span className="ring-number">{value}</span>
      <figcaption>{label}</figcaption>
    </figure>
  );
}

/**
 * A line of recent values, 0–100. `NaN` breaks the line into a gap rather
 * than dropping it to the floor: an unmeasured stretch is not a quiet one.
 */
export function Spark({
  values,
  colour,
  max = 100,
  height = 'small',
  label,
}: {
  values: readonly number[];
  colour: string;
  max?: number;
  height?: 'small' | 'large';
  label: string;
}) {
  const w = 240;
  const h = 60;
  const n = Math.max(values.length, 2);
  const segments: string[] = [];
  let current = '';
  values.forEach((v, i) => {
    if (Number.isNaN(v)) {
      if (current !== '') segments.push(current);
      current = '';
      return;
    }
    const x = (i / (n - 1)) * w;
    const y = h - (Math.min(max, Math.max(0, v)) / max) * (h - 2) - 1;
    current += `${current === '' ? 'M' : 'L'}${x.toFixed(1)},${y.toFixed(1)}`;
  });
  if (current !== '') segments.push(current);
  return (
    <svg
      className={`spark spark-${height}`}
      viewBox={`0 0 ${w} ${h}`}
      preserveAspectRatio="none"
      role="img"
      aria-label={label}
    >
      {segments.map((d, i) => (
        <path key={i} d={d} stroke={colour} fill="none" vectorEffect="non-scaling-stroke" />
      ))}
    </svg>
  );
}

/** A horizontal bar, for per-core load and disk space. */
export function Bar({ fraction, colour }: { fraction: number | null; colour: string }) {
  const f = fraction === null || Number.isNaN(fraction) ? 0 : Math.min(1, Math.max(0, fraction));
  return (
    <div className="bar" aria-hidden="true">
      {fraction !== null && (
        <div className="bar-fill" style={{ width: `${f * 100}%`, background: colour }} />
      )}
    </div>
  );
}

export function InfoRow({
  label,
  value,
  tone,
}: {
  label: string;
  value: string | null;
  tone?: Level | 'plain';
}) {
  return (
    <div className="info-row">
      <span className="info-label">{label}</span>
      <span className={`info-value tone-${tone ?? 'plain'}`}>{value ?? DASH}</span>
    </div>
  );
}

export function Panel({
  title,
  accent = Palette.accent,
  children,
  className,
}: {
  title?: string;
  accent?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <section className={`panel ${className ?? ''}`} style={{ borderTopColor: accent }}>
      {title !== undefined && <h3>{title}</h3>}
      {children}
    </section>
  );
}

export function Hint({ children, tone }: { children: ReactNode; tone?: Level }) {
  return <p className={`hint tone-${tone ?? 'plain'}`}>{children}</p>;
}

export function Choice({
  label,
  selected,
  onPress,
  autoFocus,
}: {
  label: string;
  selected: boolean;
  onPress: () => void;
  autoFocus?: boolean;
}) {
  return (
    <button
      type="button"
      className={`choice ${selected ? 'is-selected' : ''}`}
      aria-pressed={selected}
      onClick={onPress}
      {...(autoFocus === true && { 'data-autofocus': '' })}
    >
      {label}
    </button>
  );
}

export function Action({
  label,
  onPress,
  danger,
  disabled,
  autoFocus,
}: {
  label: string;
  onPress: () => void;
  danger?: boolean;
  disabled?: boolean;
  autoFocus?: boolean;
}) {
  return (
    <button
      type="button"
      className={`action ${danger === true ? 'is-danger' : ''}`}
      onClick={onPress}
      disabled={disabled === true}
      {...(autoFocus === true && { 'data-autofocus': '' })}
    >
      {label}
    </button>
  );
}

export function ScreenTitle({ children }: { children: ReactNode }) {
  return <h1 className="screen-title">{children}</h1>;
}
