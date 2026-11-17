/**
 * The motion-bearing primitives that are not overlays.
 *
 * Everything here is a guarantee about *how* a change is drawn rather than what
 * it looks like: a bar that scales instead of re-laying out, a pill that travels
 * between segments, a panel that folds, a popup that stays inside the window.
 * happy-dom cannot render any of it, so each test asserts the mechanism that
 * produces the behaviour — which is why they name the failure they prevent.
 */
import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { settle, stubBoxes } from '../test/layout';
import { Button } from './Button';
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from './Collapsible';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from './DropdownMenu';
import { Meter } from './Meter';
import { ProgressBar } from './ProgressBar';
import { SegmentedControl } from './SegmentedControl';
import { Select } from './Select';

afterEach(cleanup);

describe('meters and bars', () => {
  it('scales the meter fill rather than re-laying out the track on every sample', () => {
    render(<Meter label="CPU" accessibleLabel="CPU" value={43} valueText="43%" />);
    const fill = screen.getByTestId('meter-fill');

    expect(
      fill.style.transform,
      'the fill is not scaled, so it is not positioned by the ratio',
    ).toContain('scaleX(0.43');
    expect(
      fill.style.width,
      'the fill is still sized by width, which relayouts the track — and the rail holds one of these per core, redrawing every second',
    ).toBe('');
    expect(
      fill.className,
      'no transform origin: the fill would grow from its centre, out of the track',
    ).toContain('origin-left');
  });

  it('draws no meter fill for an unmeasured reading, rather than a zero-width one', () => {
    render(<Meter label="GPU" accessibleLabel="GPU" value={null} valueText="—" />);
    expect(screen.getByTestId('meter-fill').style.transform).toContain('scaleX(0');
    expect(
      screen.getByRole('meter').getAttribute('aria-valuenow'),
      'an unmeasured reading must not be announced as a value',
    ).toBeNull();
  });

  it('scales the determinate progress fill and leaves the indeterminate one unscaled', () => {
    const { unmount } = render(<ProgressBar label="Copying" value={25} valueText="25%" />);
    const determinate = screen.getByTestId('progress-indicator');
    expect(determinate.style.transform).toContain('scaleX(0.25');
    expect(determinate.style.width).toBe('');
    expect(determinate.className).toContain('origin-left');
    unmount();

    render(<ProgressBar label="Copying" indeterminate />);
    const sweeping = screen.getByTestId('progress-indicator');
    expect(
      sweeping.style.transform,
      'the indeterminate bar is scaled by a ratio it does not have, so it would sit at zero width',
    ).toBe('');
    expect(sweeping.className).toContain('w-1/3');
  });
});

describe('segmented control', () => {
  function Fixture() {
    return (
      <SegmentedControl
        ariaLabel="Time range"
        value="60s"
        onValueChange={vi.fn()}
        options={[
          { value: '60s', label: '60s' },
          { value: '5m', label: '5m' },
        ]}
      />
    );
  }

  it('draws the selected segment once — as a pill that travels, not as a fill per segment', async () => {
    const restore = stubBoxes();
    try {
      render(<Fixture />);
      await settle();

      const segments = screen.getAllByRole('radio');
      const [first, second] = segments;
      expect(first).toBeDefined();
      expect(second).toBeDefined();

      // `options` has no passthrough for arbitrary attributes, so the geometry
      // is written on to the rendered segments and the hook's own resize path
      // re-measures. That also covers the case this window is really about: the
      // strip following a box that changed size without the selection changing.
      first?.setAttribute('data-left', '0');
      first?.setAttribute('data-w', '40');
      first?.setAttribute('data-h', '24');
      second?.setAttribute('data-left', '44');
      second?.setAttribute('data-w', '40');
      second?.setAttribute('data-h', '24');
      window.dispatchEvent(new Event('resize'));
      await settle();

      const pill = document.querySelector('[data-segment-indicator]');
      expect(pill, 'no pill was rendered, so nothing marks the selection').not.toBeNull();
      expect(
        pill?.getAttribute('aria-hidden'),
        'the pill is announced, so the selected segment is read out twice',
      ).toBe('true');
      expect((pill as HTMLElement).style.transform).toContain('0px');

      expect(
        first?.className,
        'the segment still paints the raised fill, so the selection is drawn twice while the pill is still travelling',
      ).not.toContain('data-[state=on]:bg-');
    } finally {
      restore();
    }
  });
});

describe('collapsible', () => {
  it('keeps a closed panel out of the tab order, not merely out of sight', () => {
    render(
      <Collapsible>
        <CollapsibleTrigger asChild>
          <Button>Advanced</Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          <Button>Inner</Button>
        </CollapsibleContent>
      </Collapsible>,
    );

    expect(screen.getByRole('button', { name: 'Advanced' }).getAttribute('aria-expanded')).toBe(
      'false',
    );
    expect(
      screen.queryByRole('button', { name: 'Inner' }),
      'a closed panel is still reachable by keyboard — the user tabs into something they cannot see',
    ).toBeNull();
  });

  it('wraps its content in the clip layer the row animation needs', async () => {
    render(
      <Collapsible defaultOpen>
        <CollapsibleTrigger asChild>
          <Button>Advanced</Button>
        </CollapsibleTrigger>
        <CollapsibleContent data-testid="panel">
          <Button>Inner</Button>
        </CollapsibleContent>
      </Collapsible>,
    );
    await settle();

    const panel = screen.getByTestId('panel');
    expect(panel.className, 'the panel does not carry the animated class').toContain(
      'vitals-collapsible',
    );
    expect(panel.getAttribute('data-state')).toBe('open');
    expect(
      panel.querySelector('div > button'),
      'no clip wrapper inside the grid, so the collapsing row has nothing to hide its content with',
    ).not.toBeNull();
  });
});

describe('popup height caps', () => {
  it('lets a long dropdown menu scroll instead of running off the bottom of the window', async () => {
    render(
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button>Columns</Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent>
          <DropdownMenuItem>Reset</DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>,
    );
    const trigger = screen.getByRole('button', { name: 'Columns' });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: 'Enter' });
    await settle();

    const menu = screen.getByRole('menu');
    expect(
      menu.className,
      'the menu has no cap: on a short viewport it is laid out past the window and its last items cannot be reached',
    ).toContain('max-h-(--radix-popper-available-height)');
    expect(menu.className).toContain('overflow-y-auto');
  });

  it('caps the select popup to the room left before the window edge', async () => {
    render(
      <Select
        ariaLabel="Refresh rate"
        value="auto"
        onValueChange={vi.fn()}
        options={[{ value: 'auto', label: 'Automatic' }]}
      />,
    );
    fireEvent.keyDown(screen.getByRole('combobox'), { key: 'Enter' });
    await settle();

    const content = document.querySelector<HTMLElement>('.vitals-select-content');
    expect(content, 'the select content does not carry the cap class').not.toBeNull();
    // The cap itself is a `min()` of two custom properties, which only the
    // stylesheet can express; `styles.test.ts` asserts the rule.
  });
});
