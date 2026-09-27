import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { AnimatedValue, parse } from './AnimatedValue';

describe('parse', () => {
  it('round-trips every shape the formatters produce, unchanged', () => {
    // If render(number) of a value is not the original string, the tween's
    // last frame would differ from the settled value and the text would jump.
    for (const text of [
      '43.2%',
      '1,351',
      '141 GB',
      '50,9 GB',
      '1.06 MB/s',
      '0 B/s',
      '12.340,5 MB',
      '1.234.567',
    ]) {
      const parsed = parse(text);
      expect(parsed, text).not.toBeNull();
      expect(parsed?.render(parsed.number), text).toBe(text);
    }
  });

  it('reads a single three-digit group as thousands, not a decimal', () => {
    expect(parse('1,351')?.number).toBe(1351);
    expect(parse('1.351')?.number).toBe(1351);
  });

  it('reads a Romanian decimal comma as a decimal', () => {
    expect(parse('50,9 GB')?.number).toBeCloseTo(50.9);
  });

  it('keeps the unit apart from the number, so a unit change can refuse to tween', () => {
    expect(parse('980 KB/s')?.suffix).toBe(' KB/s');
    expect(parse('1.02 MB/s')?.suffix).toBe(' MB/s');
  });

  it('finds no number in text that has none', () => {
    expect(parse('—')).toBeNull();
    expect(parse('Not reported')).toBeNull();
  });
});

describe('AnimatedValue', () => {
  it('gives assistive technology the settled value, never an intermediate one', () => {
    const { rerender } = render(<AnimatedValue value="10%" />);
    rerender(<AnimatedValue value="90%" />);
    // The hidden copy is the one a screen reader reads; the visible roll is
    // aria-hidden. It must be the final value from the first render.
    const readable = screen.getByText('90%', { selector: '.sr-only' });
    expect(readable).toBeTruthy();
  });

  it('shows non-numeric text as is', () => {
    render(<AnimatedValue value="—" />);
    expect(screen.getAllByText('—')).toHaveLength(2);
  });
});
