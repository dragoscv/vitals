/**
 * A placed widget window's guarantees.
 *
 * This window is the only surface the user touches directly, so what it has to
 * get right is not the drawing: it has to show something honest when the
 * backend cannot describe it, keep its four controls reachable while they are
 * invisible, tell the window manager what the settings screen has already been
 * told, mark a window whose clicks no longer reach it, and — the one that is
 * easy to get wrong — never treat the tail of a drag as a click.
 *
 * `window`, `bridge` and `source` are injected: a widget window has no test host
 * and the drag gesture cannot be produced by a real window manager here.
 */

import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { act } from 'react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { Frame, SystemMetrics } from '@vitals/protocol';

import { makeSystem } from '../features/dashboard/test-fixtures';
import type { FrameSource } from '../features/widgets/live';
import type { WidgetWindowSpec } from '../features/widgets/model';
import type { WidgetBridge, WidgetFlags } from '../features/widgets/windows';
import type { WidgetWindow } from './lib/window';
import { WidgetApp } from './WidgetApp';

beforeAll(async () => {
  await initI18n('en');
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

const SPEC: WidgetWindowSpec = {
  id: 'cpu-1',
  kind: 'cpu',
  size: 'medium',
  style: 'glass',
  layout: 'stack',
  opacity: 1,
  alwaysOnTop: true,
  remoteConnectionId: null,
  clickThrough: false,
  x: 40,
  y: 40,
  width: 240,
  height: 150,
};

interface Harness {
  readonly bridge: WidgetBridge;
  readonly window: WidgetWindow;
  readonly source: FrameSource;
  readonly calls: {
    readonly flags: WidgetFlags[];
    readonly opened: string[];
    readonly closed: string[];
    readonly alwaysOnTop: boolean[];
    readonly ignoreCursor: boolean[];
    readonly dragged: number;
    /** How often the window asked the backend for its spec. */
    readonly reads: number;
    /** How often the window subscribed to the frame broadcast. */
    readonly subscriptions: number;
  };
  /** The spec the backend answers with; `null` is a backend that cannot describe it. */
  readonly setSpec: (spec: WidgetWindowSpec | null) => void;
  /** Pushes a reading, as the broadcast would. */
  readonly frame: (system: SystemMetrics) => void;
  /** Pushes a spec, as the backend's echo would. */
  readonly configure: (spec: WidgetWindowSpec) => void;
}

function harness(): Harness {
  const configs = new Set<(spec: WidgetWindowSpec) => void>();
  const frames = new Set<(frame: Frame) => void>();
  let currentSpec: WidgetWindowSpec | null = null;
  const calls = {
    flags: [] as WidgetFlags[],
    opened: [] as string[],
    closed: [] as string[],
    alwaysOnTop: [] as boolean[],
    ignoreCursor: [] as boolean[],
    dragged: 0,
    reads: 0,
    subscriptions: 0,
  };

  const bridge: WidgetBridge = {
    sync: async () => [],
    get: async () => {
      calls.reads += 1;
      return currentSpec;
    },
    close: async (id) => {
      calls.closed.push(id);
    },
    setFlags: async (_id, flags) => {
      calls.flags.push(flags);
    },
    openSettings: async (id) => {
      calls.opened.push(id);
    },
    onMoved: () => () => {},
    onClosed: () => () => {},
    onChanged: () => () => {},
    onEdit: () => () => {},
    onConfig: (listener) => {
      configs.add(listener);
      return () => {
        configs.delete(listener);
      };
    },
  };

  const win: WidgetWindow = {
    label: async () => null,
    startDragging: async () => {
      calls.dragged += 1;
    },
    setIgnoreCursorEvents: async (ignore) => {
      calls.ignoreCursor.push(ignore);
    },
    setAlwaysOnTop: async (onTop) => {
      calls.alwaysOnTop.push(onTop);
    },
    close: async () => {},
  };

  const source: FrameSource = (onFrame) => {
    calls.subscriptions += 1;
    frames.add(onFrame);
    return () => {
      frames.delete(onFrame);
    };
  };

  return {
    bridge,
    window: win,
    source,
    calls,
    setSpec: (spec) => {
      currentSpec = spec;
    },
    frame: (system) => {
      act(() => {
        for (const listener of frames) {
          listener({
            seq: 1,
            timestampMs: 1_760_000_000_000,
            elapsedMs: 1000,
            payload: { kind: 'keyframe', system, processes: [] },
          });
        }
      });
    },
    configure: (spec) => {
      act(() => {
        for (const listener of configs) listener(spec);
      });
    },
  };
}

/** Renders one widget window. `null` is a backend that cannot describe it. */
function show(spec: WidgetWindowSpec | null = SPEC): Harness {
  const io = harness();
  io.setSpec(spec);
  render(
    <WidgetApp id="cpu-1" locale="en" bridge={io.bridge} window={io.window} source={io.source} />,
  );
  return io;
}

describe('a widget window', () => {
  it('says so rather than showing nothing when no backend can describe it', async () => {
    show(null);

    await waitFor(() => {
      expect(screen.getByText('This widget is not on screen.')).toBeDefined();
    });
    // No half-drawn panel behind it: a widget that rendered zeroes for a widget
    // it does not have would look like a working widget that happens to be idle.
    expect(screen.queryByText('50%')).toBeNull();
  });

  it('keeps every toolbar control in the tab order while the toolbar is invisible', async () => {
    show();
    await waitFor(() => {
      expect(screen.getByText('This widget is not on screen.')).toBeDefined();
    });

    for (const name of [
      'Stop keeping it on top',
      'Let clicks pass through',
      "Open this widget's settings",
      'Close this widget',
    ]) {
      const button = screen.getByRole('button', { name });
      expect(button.tabIndex).toBeGreaterThanOrEqual(0);
    }
  });

  it('opens its own settings and closes itself from the toolbar', async () => {
    const io = show();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: "Open this widget's settings" })).toBeDefined();
    });

    fireEvent.click(screen.getByRole('button', { name: "Open this widget's settings" }));
    fireEvent.click(screen.getByRole('button', { name: 'Close this widget' }));

    await waitFor(() => {
      expect(io.calls.opened).toEqual(['cpu-1']);
    });
    expect(io.calls.closed).toEqual(['cpu-1']);
  });

  it('never swallows Tab, because an always-on-top window must not trap the keyboard', async () => {
    show();
    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Close this widget' })).toBeDefined();
    });

    const root = document.querySelector('[data-tauri-drag-region]');
    expect(root).not.toBeNull();

    // Borderless and above everything: if this window cancelled Tab, the user
    // would have no way back to the rest of the desktop, and the only way out
    // would be the mouse. `fireEvent` answers false when the handler called
    // `preventDefault`, so this is a statement about the window, not the DOM.
    for (const key of ['Tab', 'Escape']) {
      expect(fireEvent.keyDown(root!, { key })).toBe(true);
    }
  });

  it('turns the pin off on the window as well as telling the backend', async () => {
    const io = show();
    const button = await screen.findByRole('button', { name: 'Stop keeping it on top' });
    fireEvent.click(button);

    await waitFor(() => {
      expect(io.calls.flags).toEqual([{ alwaysOnTop: false, clickThrough: false }]);
    });
    // The window follows immediately rather than on the round trip, so the
    // button cannot lag a frame and read as broken.
    expect(io.calls.alwaysOnTop).toEqual([false]);
    // And the button now offers the opposite action, so the user can undo it.
    expect(screen.getByRole('button', { name: 'Keep on top' })).toBeDefined();
  });

  it('marks a window whose clicks pass through, because nothing else can', async () => {
    const io = show();

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Let clicks pass through' })).toBeDefined();
    });
    const rings = (): number =>
      document.querySelectorAll('[class*="ring-[var(--color-accent)]"]').length;
    expect(rings()).toBe(0);

    io.configure({ ...SPEC, clickThrough: true });

    // A click-through window looks exactly like one that is not, and its clicks
    // land behind it — the ring is the only signal the user gets.
    await waitFor(() => {
      expect(rings()).toBe(1);
    });

    fireEvent.click(screen.getByRole('button', { name: 'Stop clicks passing through' }));

    await waitFor(() => {
      expect(io.calls.ignoreCursor).toEqual([false]);
    });
    expect(rings()).toBe(0);
  });

  it('treats a pointer that moved as a drag, never as a click', async () => {
    const io = show();

    // Expanded, the widget shows its remaining readings; collapsed it shows
    // only the lead one. That is what the drag must not do.
    const collapsed = (): boolean => screen.queryByText('Temperature') === null;
    await waitFor(() => {
      expect(collapsed()).toBe(true);
    });

    const panel = document.querySelector('[data-tauri-drag-region]');
    expect(panel).not.toBeNull();
    const node = panel!;

    fireEvent.pointerDown(node, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.click(node, { clientX: 140, clientY: 90 });

    // A drag ends with a synthetic click, and toggling on it would collapse the
    // widget every time the user moved it.
    expect(io.calls.dragged).toBe(1);
    expect(collapsed()).toBe(true);

    fireEvent.pointerDown(node, { button: 0, clientX: 10, clientY: 10 });
    fireEvent.click(node, { clientX: 12, clientY: 11 });

    await waitFor(() => {
      expect(collapsed()).toBe(false);
    });
  });

  it('reads the machine from the broadcast it is given', async () => {
    const io = show();
    await waitFor(() => {
      expect(screen.getByRole('group', { name: 'CPU' })).toBeDefined();
    });

    io.frame(makeSystem({ cpu: { total: 50, temperature: 61 } }));

    await waitFor(() => {
      expect(screen.getAllByText('50%').length).toBeGreaterThan(0);
    });
  });

  it('runs no clock of its own, so a widget nobody touches costs nothing between frames', async () => {
    // The sampler is the only source of readings. A widget that also ran a
    // repeating timer would keep waking the GPU for a window the user is not
    // looking at, and would show a second, slightly different number for the
    // same machine than the rest of the app shows.
    //
    // The spy is read around the MOUNT and before any `waitFor`: Testing
    // Library's own waitFor polls on an interval, so a spy held open for the
    // whole test counts the harness as well as the product - which is exactly
    // how the first version of this test passed while a polling mutant was in
    // the tree. A zero-delay timeout settles the backend read instead.
    const intervals = vi.spyOn(globalThis, 'setInterval');
    const io = harness();
    io.setSpec(SPEC);
    render(
      <WidgetApp id="cpu-1" locale="en" bridge={io.bridge} window={io.window} source={io.source} />,
    );
    await act(async () => {
      await new Promise((resolve) => {
        setTimeout(resolve, 0);
      });
    });

    expect(intervals).not.toHaveBeenCalled();
    intervals.mockRestore();

    // And it asks once, then waits to be told: one read of the spec and one
    // subscription to the broadcast, rather than a fresh pair per tick.
    expect(io.calls.reads).toBe(1);
    expect(io.calls.subscriptions).toBe(1);
  });
});

describe('a widget whose spec arrives', () => {
  it('draws the widget the backend describes, in the style it was given', async () => {
    const io = show(null);
    await waitFor(() => {
      expect(screen.getByText('This widget is not on screen.')).toBeDefined();
    });

    io.configure({ ...SPEC, kind: 'memory', style: 'glass' });

    await waitFor(() => {
      expect(screen.getByRole('group', { name: 'Memory' })).toBeDefined();
    });
    // The glass style is a lighter fill; if it were ignored the two styles would
    // be indistinguishable and the setting would be decorative.
    expect(document.querySelector('[class*="bg-[rgb(20_20_24/0.84)]"]')).not.toBeNull();
  });
});
