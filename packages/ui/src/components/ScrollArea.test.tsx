import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it } from 'vitest';
import { ScrollArea } from './ScrollArea';
import { CheckGlyph, ChevronRightGlyph, CloseGlyph, SearchGlyph } from './menuGlyphs';

afterEach(cleanup);

describe('ScrollArea', () => {
  it('names the focusable viewport, so landing in it is not announced as a bare "group"', () => {
    render(
      <ScrollArea ariaLabel="Process list">
        <p>row</p>
      </ScrollArea>,
    );

    const viewport = screen.getByLabelText('Process list');
    expect(viewport.contains(screen.getByText('row'))).toBe(true);
  });

  it('forwards the ref to the viewport — the element that actually scrolls — not the outer frame', () => {
    let captured: HTMLDivElement | null = null;
    render(
      <ScrollArea
        ariaLabel="Panel"
        ref={(el) => {
          captured = el;
        }}
      >
        <p>row</p>
      </ScrollArea>,
    );

    // Callers use the ref to `scrollTo`; on the frame that call is a no-op
    // because the frame is `overflow: hidden`.
    expect(captured).not.toBeNull();
    expect(screen.getByLabelText('Panel')).toBe(captured);
  });
});

describe('menu glyphs', () => {
  it('are hidden from assistive technology, since the parent already exposes the state they draw', () => {
    const { container } = render(
      <>
        <CheckGlyph />
        <ChevronRightGlyph />
        <CloseGlyph />
        <SearchGlyph />
      </>,
    );

    const svgs = container.querySelectorAll('svg');
    expect(svgs).toHaveLength(4);
    for (const svg of svgs) {
      expect(svg.getAttribute('aria-hidden')).toBe('true');
    }
  });
});
