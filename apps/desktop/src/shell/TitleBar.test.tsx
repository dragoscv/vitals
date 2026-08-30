import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeAll, describe, expect, it } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { registerShellStrings } from './strings';
import { TitleBar } from './TitleBar';
import type { WindowControls } from './windowControls';

interface Harness {
  readonly controls: WindowControls;
  /** Fires the resize listener the component registered. */
  emitResize(): void;
  setMaximised(value: boolean): void;
  readonly calls: string[];
}

function harness(initiallyMaximised = false): Harness {
  let maximised = initiallyMaximised;
  let handler: (() => void) | undefined;
  const calls: string[] = [];

  return {
    calls,
    emitResize: () => handler?.(),
    setMaximised: (value) => {
      maximised = value;
    },
    controls: {
      minimize: () => {
        calls.push('minimize');
        return Promise.resolve();
      },
      toggleMaximize: () => {
        calls.push('toggleMaximize');
        return Promise.resolve();
      },
      close: () => {
        calls.push('close');
        return Promise.resolve();
      },
      isMaximized: () => Promise.resolve(maximised),
      onResized: (next) => {
        handler = next;
        return Promise.resolve(() => {
          handler = undefined;
        });
      },
    },
  };
}

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
});

describe('TitleBar', () => {
  it('exposes each caption button by its accessible name', () => {
    const { controls } = harness();
    render(<TitleBar controls={controls}>Dashboard</TitleBar>);

    expect(screen.getByRole('button', { name: 'Minimise' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Maximise' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Close' })).toBeTruthy();
  });

  it('invokes the matching window operation', () => {
    const h = harness();
    render(<TitleBar controls={h.controls}>Dashboard</TitleBar>);

    fireEvent.click(screen.getByRole('button', { name: 'Minimise' }));
    fireEvent.click(screen.getByRole('button', { name: 'Maximise' }));
    fireEvent.click(screen.getByRole('button', { name: 'Close' }));

    expect(h.calls).toEqual(['minimize', 'toggleMaximize', 'close']);
  });

  it('shows Restore when the window is already maximised', async () => {
    const h = harness(true);
    render(<TitleBar controls={h.controls}>Dashboard</TitleBar>);

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Restore down' })).toBeTruthy();
    });
  });

  it('follows a maximise it did not initiate', async () => {
    // Win+Up, a double-click on the drag region and an edge snap all change
    // the state without going through our button. If the label only tracked
    // clicks it would read "Maximise" on an already-maximised window.
    const h = harness(false);
    render(<TitleBar controls={h.controls}>Dashboard</TitleBar>);

    await waitFor(() => screen.getByRole('button', { name: 'Maximise' }));

    h.setMaximised(true);
    h.emitResize();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Restore down' })).toBeTruthy();
    });
  });

  it('keeps a drag region that is not a caption button', () => {
    const { controls } = harness();
    const { container } = render(<TitleBar controls={controls}>Dashboard</TitleBar>);

    const region = container.querySelector('[data-tauri-drag-region]');
    expect(region).not.toBeNull();
    // A caption button inside the drag region swallows its own mousedown and
    // stops responding to the first click.
    expect(region?.querySelector('button')).toBeNull();
  });

  it('unsubscribes from resize on unmount', async () => {
    const h = harness();
    const { unmount } = render(<TitleBar controls={h.controls}>Dashboard</TitleBar>);
    await waitFor(() => screen.getByRole('button', { name: 'Maximise' }));

    unmount();
    // A leaked listener would keep calling setState on an unmounted tree every
    // time the user resizes the window.
    expect(() => h.emitResize()).not.toThrow();
  });
});
