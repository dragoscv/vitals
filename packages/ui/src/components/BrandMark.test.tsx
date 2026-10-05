import { render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { BrandMark } from './BrandMark';
import { MARK_GLYPH } from './brandGeometry';

const line = (container: HTMLElement) => container.querySelector('.vm-line');

describe('BrandMark', () => {
  it('draws the pixel-snapped micro variant at 16 px rather than a shrunken master', () => {
    const { container } = render(<BrandMark size={16} />);
    expect(line(container)?.getAttribute('d')).toBe(MARK_GLYPH.micro.d);
    expect(line(container)?.getAttribute('d')).not.toBe(MARK_GLYPH.master.d);
  });

  it('uses the master geometry at icon sizes so the app matches the installed icon', () => {
    const { container } = render(<BrandMark size={128} />);
    expect(line(container)?.getAttribute('d')).toBe(MARK_GLYPH.master.d);
  });

  it('is hidden from assistive technology unless it is given a label', () => {
    const quiet = render(<BrandMark />).container.querySelector('svg');
    expect(quiet?.getAttribute('aria-hidden')).toBe('true');
    const named = render(<BrandMark label="Vitals" />).container.querySelector('svg');
    expect(named?.getAttribute('role')).toBe('img');
    expect(named?.getAttribute('aria-label')).toBe('Vitals');
  });

  it('gives two marks on one page distinct gradient ids so neither paints the other', () => {
    const { container } = render(
      <>
        <BrandMark />
        <BrandMark />
      </>,
    );
    const ids = [...container.querySelectorAll('linearGradient')].map((g) => g.id);
    expect(new Set(ids).size).toBe(2);
  });

  it('drops the tile and paints in currentColor when bare', () => {
    const { container } = render(<BrandMark bare />);
    expect(container.querySelector('.vm-tile')).toBeNull();
    expect(line(container)?.getAttribute('stroke')).toBe('currentColor');
  });

  it('exposes its state to the stylesheet that animates it', () => {
    const { container } = render(<BrandMark state="thinking" />);
    expect(container.querySelector('svg')?.dataset['state']).toBe('thinking');
  });
});
