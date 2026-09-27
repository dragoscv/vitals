import { cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { Badge } from './Badge';
import { Card, CardBody, CardHeader, CardTitle } from './Card';
import { EmptyState } from './EmptyState';
import { IconButton } from './IconButton';
import { Meter } from './Meter';
import { ProgressBar } from './ProgressBar';
import { Separator } from './Separator';
import { Skeleton } from './Skeleton';
import { Spinner } from './Spinner';

afterEach(cleanup);

describe('Meter', () => {
  it('uses role="meter", because a CPU reading is not a task making progress', () => {
    render(<Meter label="CPU" accessibleLabel="CPU usage" value={43} valueText="43%" />);
    const meter = screen.getByRole('meter', { name: 'CPU usage' });

    expect(meter.getAttribute('aria-valuenow')).toBe('43');
    expect(
      meter.getAttribute('aria-valuetext'),
      'without valuetext a screen reader reads a bare ratio, which is meaningless for bytes',
    ).toBe('43%');
  });

  it('clamps an overshooting sample so the fill cannot escape its track', () => {
    render(<Meter label="CPU" accessibleLabel="CPU usage" value={137} valueText="137%" />);
    expect(
      screen.getByTestId('meter-fill').style.width,
      'a sampled value above max rendered a fill wider than the track',
    ).toBe('100.00%');
  });

  it('stays silent by default, so a 1 Hz metric does not narrate forever', () => {
    render(<Meter label="CPU" accessibleLabel="CPU usage" value={12} valueText="12%" />);
    // The value renders twice: a rolling copy hidden from assistive
    // technology and the accessible one. Only the accessible copy may ever
    // be a live region.
    expect(
      screen.getByText('12%', { ignore: '[aria-hidden]' }).getAttribute('aria-live'),
      'a continuously sampled meter must not be a live region',
    ).toBeNull();
  });

  it('announces politely when the caller opts in', () => {
    render(<Meter label="CPU" accessibleLabel="CPU usage" value={12} valueText="12%" live />);
    expect(screen.getByText('12%', { ignore: '[aria-hidden]' }).getAttribute('aria-live')).toBe(
      'polite',
    );
    expect(
      screen.getByText('12%', { ignore: '.sr-only' }).getAttribute('aria-live'),
      'the rolling copy must never announce — it changes every frame',
    ).toBeNull();
  });
});

describe('ProgressBar', () => {
  it('omits aria-valuenow when indeterminate, since there is no value to report', () => {
    render(<ProgressBar indeterminate label="Scanning" />);
    const bar = screen.getByRole('progressbar', { name: 'Scanning' });
    expect(
      bar.getAttribute('aria-valuenow'),
      'an indeterminate bar claimed a concrete value',
    ).toBeNull();
  });

  it('reports a determinate value against its own max', () => {
    render(<ProgressBar label="Copying" value={3} max={12} valueText="3 of 12 files" />);
    const bar = screen.getByRole('progressbar', { name: 'Copying' });
    expect(bar.getAttribute('aria-valuenow')).toBe('3');
    expect(bar.getAttribute('aria-valuemax')).toBe('12');
    expect(bar.getAttribute('aria-valuetext')).toBe('3 of 12 files');
    expect(screen.getByTestId('progress-indicator').style.width).toBe('25.00%');
  });
});

describe('Spinner', () => {
  it('is hidden from assistive technology when it has no name', () => {
    render(<Spinner />);
    expect(
      screen.queryByRole('status'),
      'an unlabelled spinner inside a labelled control should not add a second announcement',
    ).toBeNull();
  });

  it('becomes a status when given a caller-supplied label', () => {
    render(<Spinner label="Sampling" />);
    expect(screen.getByRole('status', { name: 'Sampling' })).toBeTruthy();
  });
});

describe('Skeleton', () => {
  it('is always hidden, because a grey rectangle has nothing to announce', () => {
    render(<Skeleton data-testid="sk" />);
    expect(
      screen.getByTestId('sk').getAttribute('aria-hidden'),
      'skeletons must be aria-hidden or a loading list becomes a burst of noise',
    ).toBe('true');
  });

  it('drops the pulse when the user has asked for reduced motion', () => {
    vi.stubGlobal('matchMedia', (query: string) => ({
      matches: query.includes('reduce'),
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    }));

    render(<Skeleton data-testid="sk" />);
    expect(
      screen.getByTestId('sk').className.includes('animate-pulse'),
      'the skeleton kept animating despite prefers-reduced-motion',
    ).toBe(false);

    vi.unstubAllGlobals();
  });
});

describe('EmptyState', () => {
  it('is a status, so filtering a list to nothing is actually announced', () => {
    render(<EmptyState title="No matching processes" description="Try a different filter." />);
    const state = screen.getByRole('status');
    expect(state.textContent, 'the empty state did not render the caller-supplied copy').toContain(
      'No matching processes',
    );
  });
});

describe('IconButton', () => {
  it('always has an accessible name, since there is no visible text to fall back on', () => {
    render(<IconButton icon={<svg />} label="End task" />);
    expect(
      screen.getByRole('button', { name: 'End task' }),
      'an icon-only button without a name is announced as just "button"',
    ).toBeTruthy();
  });
});

describe('Badge', () => {
  it('hides its decorative icon so the tone is not read twice', () => {
    render(
      <Badge tone="danger" icon={<svg data-testid="glyph" />}>
        Not responding
      </Badge>,
    );
    expect(screen.getByTestId('glyph').parentElement?.getAttribute('aria-hidden')).toBe('true');
  });
});

describe('Separator', () => {
  it('is decorative by default, so a dense toolbar is not read as a list of separators', () => {
    const { container } = render(<Separator />);
    expect(
      container.firstElementChild?.getAttribute('role'),
      'a decorative rule was exposed as a semantic separator',
    ).toBe('none');
  });

  it('becomes semantic on request', () => {
    render(<Separator decorative={false} />);
    expect(screen.getByRole('separator')).toBeTruthy();
  });
});

describe('Card', () => {
  it('becomes a named landmark only when a label is supplied', () => {
    const { rerender } = render(<Card>plain</Card>);
    expect(
      screen.queryByRole('region'),
      'an unlabelled card should not add an anonymous landmark',
    ).toBeNull();

    rerender(<Card regionLabel="GPU">labelled</Card>);
    expect(screen.getByRole('region', { name: 'GPU' })).toBeTruthy();
  });

  it('lets the caller pick the heading rank so the outline is not broken', () => {
    render(
      <Card>
        <CardHeader>
          <CardTitle level={2}>Memory</CardTitle>
        </CardHeader>
        <CardBody>body</CardBody>
      </Card>,
    );
    expect(
      screen.getByRole('heading', { level: 2, name: 'Memory' }),
      'the heading level prop was ignored, which produces skipped levels when nested',
    ).toBeTruthy();
  });
});
