import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { useNavVisibility } from './navigation';
import { registerShellStrings } from './strings';
import { TitleBar, TRAFFIC_LIGHT_INSET } from './TitleBar';
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
      hide: () => {
        calls.push('hide');
        return Promise.resolve();
      },
      quit: () => {
        calls.push('quit');
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

// The platform is module state; a test that sets it must not leak into the
// next one, which asserts the Windows caption buttons.
afterEach(() => {
  cleanup();
  useNavVisibility.getState().setPlatform(null);
});

describe('TitleBar', () => {
  it('offers a skip link first that focuses the content without changing the route', () => {
    const { controls } = harness();
    const main = document.createElement('main');
    main.id = 'main-content';
    main.tabIndex = -1;
    document.body.append(main);
    try {
      const before = window.location.hash;
      const { container } = render(<TitleBar controls={controls}>Dashboard</TitleBar>);
      const link = screen.getByRole('link', { name: 'Skip to content' });
      // First in the tab order: nothing focusable precedes it.
      expect(container.querySelector('a, button')).toBe(link);
      fireEvent.click(link);
      expect(document.activeElement).toBe(main);
      // Routes live in the fragment; following the href would navigate.
      expect(window.location.hash).toBe(before);
    } finally {
      main.remove();
    }
  });

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

  it('right-clicking the bar offers the window menu, and Quit quits rather than hiding', async () => {
    const h = harness();
    render(<TitleBar controls={h.controls}>Dashboard</TitleBar>);
    fireEvent.contextMenu(screen.getByRole('banner'), { button: 2, clientX: 40, clientY: 10 });
    await act(async () => {});
    const menu = await screen.findByRole('menu');
    for (const name of ['Minimise', 'Maximise', 'Hide to tray', 'Close', 'Quit Vitals']) {
      expect(within(menu).getByRole('menuitem', { name })).toBeTruthy();
    }
    fireEvent.click(within(menu).getByRole('menuitem', { name: 'Quit Vitals' }));
    expect(h.calls).toEqual(['quit']);
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

  it('on macOS leaves the traffic lights to the system and draws no caption buttons', () => {
    // `titleBarStyle: Overlay` keeps the native frame, so the system draws the
    // three controls; a second, non-native set here would sit beside them, and
    // the brand mark needs room cleared for the real ones.
    useNavVisibility.getState().setPlatform('macos');
    const { controls } = harness();
    render(<TitleBar controls={controls}>Dashboard</TitleBar>);

    for (const name of ['Minimise', 'Maximise', 'Close']) {
      expect(screen.queryByRole('button', { name })).toBeNull();
    }
    expect(screen.getByRole('banner').style.paddingLeft).toBe(`${TRAFFIC_LIGHT_INSET}px`);
    // The bar is still draggable, which is what makes the overlay window move.
    expect(document.querySelector('[data-tauri-drag-region]')).not.toBeNull();
  });
});
