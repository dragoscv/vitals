import { useEffect, useRef, useState } from 'react';

import { useReducedMotion } from '../lib/useReducedMotion';

/**
 * A formatted reading whose digits roll to the new value instead of jumping.
 *
 * Takes the already-formatted string ("43.2%", "1.06 MB/s", "12,340") and
 * tweens only its first number, keeping every other character — the unit, the
 * locale's separators, the decimal count — exactly as the formatter produced
 * them. That is why it takes a string rather than a number and a format: the
 * formatting rules live in `lib/format`, and a second copy here is how "1.5 GB"
 * and "1536 MB" end up on the same screen.
 *
 * The tween only runs when the unit text is unchanged. "980 KB/s" to
 * "1.02 MB/s" is not a movement from 980 to 1.02, and animating it would pass
 * through nonsense like "500 MB/s" on the way.
 *
 * Screen readers get the final value only: the rolling text is aria-hidden and
 * a visually hidden copy carries the real one, so an update is one
 * announcement (if the region is live at all), not thirty.
 */
export function AnimatedValue({
  value,
  duration = 420,
  live = false,
}: {
  readonly value: string;
  readonly duration?: number;
  /**
   * Announce the final value. Lives on the accessible copy, not the rolling
   * one: a live region on the tween would announce every frame.
   */
  readonly live?: boolean;
}) {
  const reduced = useReducedMotion();
  const [shown, setShown] = useState(value);
  const from = useRef(value);

  useEffect(() => {
    const previous = parse(from.current);
    const next = parse(value);
    from.current = value;

    if (
      reduced ||
      previous === null ||
      next === null ||
      previous.prefix !== next.prefix ||
      previous.suffix !== next.suffix ||
      previous.number === next.number ||
      typeof requestAnimationFrame !== 'function'
    ) {
      setShown(value);
      return;
    }

    const start = performance.now();
    let frame = requestAnimationFrame(function step(now) {
      const t = Math.min((now - start) / duration, 1);
      // Ease-out quart, matching --ease-out-quart in theme.css.
      const eased = 1 - (1 - t) ** 4;
      const current = previous.number + (next.number - previous.number) * eased;
      setShown(t >= 1 ? value : next.render(current));
      if (t < 1) frame = requestAnimationFrame(step);
    });

    return () => {
      cancelAnimationFrame(frame);
    };
  }, [value, reduced, duration]);

  return (
    <>
      <span aria-hidden="true">{shown}</span>
      <span className="sr-only" aria-live={live ? 'polite' : undefined}>
        {value}
      </span>
    </>
  );
}

interface Parsed {
  readonly prefix: string;
  readonly suffix: string;
  readonly number: number;
  readonly render: (n: number) => string;
}

/**
 * Splits a formatted string into prefix, number and suffix.
 *
 * Handles both separator conventions the app ships: English `1,234.5` and
 * Romanian `1.234,5`. Which is which is decided by the last separator —
 * a decimal separator is always the rightmost — and the decimal count is
 * preserved so the rolling text is the same width as the final one.
 */
export function parse(text: string): Parsed | null {
  const match = /^(\D*?)(-?\d[\d.,\u00a0\u202f ]*\d|-?\d)(.*)$/su.exec(text);
  if (match === null) return null;
  const [, prefix = '', digits = '', suffix = ''] = match;

  const decimalIndex = findDecimal(digits);
  const decimalChar = decimalIndex === -1 ? '' : (digits[decimalIndex] ?? '');
  const head = decimalIndex === -1 ? digits : digits.slice(0, decimalIndex);
  const tail = decimalIndex === -1 ? '' : digits.slice(decimalIndex + 1);
  const decimals = tail.length;
  const groupChar = /[.,\u00a0\u202f ]/.exec(head)?.[0] ?? '';

  const sign = digits.startsWith('-') ? '-' : '';
  const number = Number(`${sign}${head.replace(/\D/g, '')}.${tail === '' ? '0' : tail}`);
  if (!Number.isFinite(number)) return null;

  const render = (n: number): string => {
    const fixed = Math.abs(n).toFixed(decimals);
    const [int = '0', frac] = fixed.split('.');
    const grouped = groupChar === '' ? int : int.replace(/\B(?=(\d{3})+(?!\d))/g, groupChar);
    const sign = n < 0 ? '-' : '';
    return `${prefix}${sign}${grouped}${frac !== undefined ? `${decimalChar}${frac}` : ''}${suffix}`;
  };

  return { prefix, suffix, number, render };
}

/**
 * Index of the decimal separator in a digit run, or -1.
 *
 * With both `.` and `,` present the rightmost is the decimal. With one kind:
 * repeated means grouping ("1,234,567"); once and followed by exactly three
 * digits means grouping too ("1,351" processes). Anything else is a decimal
 * ("43.2", Romanian "50,9"). Spaces are only ever grouping.
 */
function findDecimal(digits: string): number {
  const dot = digits.lastIndexOf('.');
  const comma = digits.lastIndexOf(',');
  if (dot !== -1 && comma !== -1) return Math.max(dot, comma);
  const index = Math.max(dot, comma);
  if (index === -1) return -1;
  const char = digits[index] ?? '';
  if (digits.indexOf(char) !== index) return -1;
  return digits.length - index - 1 === 3 ? -1 : index;
}
