import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';
import type { Frame, SystemMetrics } from '@vitals/protocol';

import { HudApp } from './HudApp';
import type { FrameSource } from './lib/live';
import type { HudWindow } from './lib/window';

/**
 * A machine with a CPU temperature and no GPU that reports utilisation.
 *
 * Only the fields the overlay reads are populated; the cast keeps this to the
 * three readings under test rather than a forty-field structure whose other
 * values would be noise nobody maintains.
 */
function system(overrides: Partial<SystemMetrics> = {}): SystemMetrics {
  return {
    cpu: { total: 42.4, temperature: 61.2 },
    memory: { used: 8 * 1024 ** 3, total: 16 * 1024 ** 3 },
    gpus: [],
    ...overrides,
  } as unknown as SystemMetrics;
}

function frame(metrics: SystemMetrics): Frame {
  return {
    seq: 1,
    timestampMs: 0,
    elapsedMs: 1000,
    payload: { kind: 'keyframe', system: metrics, processes: [] },
  } as unknown as Frame;
}

/** A source that pushes one frame synchronously on subscribe. */
function oneFrame(metrics: SystemMetrics): FrameSource {
  return (onFrame) => {
    onFrame(frame(metrics));
    return () => {};
  };
}

function fakeWindow(): HudWindow & { readonly calls: string[] } {
  const calls: string[] = [];
  return {
    calls,
    startDragging: vi.fn(async () => {
      calls.push('drag');
    }),
    setIgnoreCursorEvents: vi.fn(async (ignore: boolean) => {
      calls.push(`ignore:${String(ignore)}`);
    }),
    setAlwaysOnTop: vi.fn(async (onTop: boolean) => {
      calls.push(`onTop:${String(onTop)}`);
    }),
    close: vi.fn(async () => {
      calls.push('close');
    }),
  };
}

beforeAll(async () => {
  await initI18n('en');
});

describe('HudApp', () => {
  it('shows an em dash for a GPU no adapter reports, never a zero', () => {
    render(<HudApp source={oneFrame(system())} window={fakeWindow()} />);

    // 42% CPU and 50% memory are measured; the GPU is not. An unmeasured
    // reading rendered as "0%" is the defect this assertion exists for — it
    // shipped once, on the phone app, beside a real GPU at 16%.
    expect(screen.getByText('42%')).toBeTruthy();
    expect(screen.getByText('50%')).toBeTruthy();
    expect(screen.getByText('—')).toBeTruthy();
    expect(screen.queryByText('0%')).toBeNull();
  });

  it('shows the busiest adapter when at least one reports utilisation', () => {
    render(
      <HudApp
        source={oneFrame(
          system({
            gpus: [
              { utilization: null },
              { utilization: 16.4 },
            ] as unknown as SystemMetrics['gpus'],
          }),
        )}
        window={fakeWindow()}
      />,
    );

    expect(screen.getByText('16%')).toBeTruthy();
  });

  it('shows the CPU temperature inline when the sensor reports one', () => {
    render(<HudApp source={oneFrame(system())} window={fakeWindow()} />);
    expect(screen.getByText('61°C')).toBeTruthy();
  });

  it('shows no temperature at all when the sensor reports none', () => {
    render(
      <HudApp
        source={oneFrame(
          system({ cpu: { total: 42.4, temperature: null } as unknown as SystemMetrics['cpu'] }),
        )}
        window={fakeWindow()}
      />,
    );

    // An em dash where a temperature would go reads as a broken sensor on a
    // machine that simply has none. The row shows the percentage and stops.
    expect(screen.queryByText('61°C')).toBeNull();
    expect(screen.queryByText(/°C/)).toBeNull();
  });

  it('renders em dashes before the first frame rather than zeros', () => {
    render(<HudApp source={() => () => {}} window={fakeWindow()} />);
    expect(screen.getAllByText('—')).toHaveLength(3);
  });

  it('passes click-through to the window and marks the panel so the user can tell', () => {
    const win = fakeWindow();
    const { container } = render(<HudApp source={oneFrame(system())} window={win} />);
    const panel = container.firstElementChild;

    expect(panel?.className).not.toContain('--color-accent)]');
    fireEvent.click(screen.getByRole('button', { name: 'Click-through' }));

    expect(win.calls).toContain('ignore:true');
    // Without the border a window that ignores the cursor is indistinguishable
    // from one that does not, and the user has no way to work out why their
    // clicks are landing somewhere else.
    expect(container.firstElementChild?.className).toContain('border-[var(--color-accent)]');
  });

  it('drops click-through when the backend reports it re-showed the overlay', () => {
    // Ctrl+Shift+H is the documented way out of click-through. The backend
    // clears the cursor flag when it shows the window; the toolbar must
    // follow, or the icon says "clicks pass through" over a window that
    // receives them.
    const win = fakeWindow();
    let fireShown: (() => void) | undefined;
    const shown = (onShown: () => void): (() => void) => {
      fireShown = onShown;
      return () => {};
    };
    const { container } = render(<HudApp source={oneFrame(system())} window={win} shown={shown} />);
    fireEvent.click(screen.getByRole('button', { name: 'Click-through' }));
    expect(container.firstElementChild?.className).toContain('border-[var(--color-accent)]');

    act(() => fireShown?.());

    expect(container.firstElementChild?.className).not.toContain('border-[var(--color-accent)]');
    expect(screen.getByRole('button', { name: 'Click-through' })).toBeTruthy();
  });

  it('unpins and repins through the window', () => {
    const win = fakeWindow();
    render(<HudApp source={oneFrame(system())} window={win} />);

    fireEvent.click(screen.getByRole('button', { name: 'Unpin from the top' }));
    expect(win.calls).toContain('onTop:false');

    fireEvent.click(screen.getByRole('button', { name: 'Keep on top' }));
    expect(win.calls).toContain('onTop:true');
  });

  it('closes through the window', () => {
    const win = fakeWindow();
    render(<HudApp source={oneFrame(system())} window={win} />);
    fireEvent.click(screen.getByRole('button', { name: 'Hide the overlay' }));
    expect(win.calls).toContain('close');
  });
});
