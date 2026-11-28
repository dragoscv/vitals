import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { KeyedBackend } from '@/settings/persistence';

import type { WidgetWindowSpec } from './model';
import { defaultPosition, MAX_WIDGETS } from './model';
import { useWidgets, type WidgetStoreOptions } from './store';
import {
  createScreenSource,
  type ScreenRect,
  type WidgetBridge,
  type WidgetEdit,
  type WidgetMove,
} from './windows';

// The warnings this feature reports are part of its contract — a widget that
// could not be brought back, or a screen list that could not be learned, has to
// say so, because silence there is indistinguishable from "nothing to do".
const { warned } = vi.hoisted(() => ({ warned: [] as unknown[][] }));

vi.mock('@/lib/logToFile', () => ({
  reportToLog: (...args: unknown[]) => {
    warned.push(args);
  },
}));

/**
 * The registry is the only thing standing between a user's widget layout and
 * losing it, and every one of its writers is asynchronous. These tests drive
 * the real hook against injected I/O so the guarantees below are the ones the
 * app actually has, not the ones the code appears to have.
 */

/** One persisted widget, so a test can start from a placed layout. */
function placed(id: string): Record<string, unknown> {
  return {
    id,
    kind: 'cpu',
    size: 'medium',
    style: 'plain',
    layout: 'stack',
    opacity: 1,
    alwaysOnTop: true,
    remoteConnectionId: null,
    clickThrough: false,
    x: 40,
    y: 40,
  };
}

interface Harness {
  readonly backend: KeyedBackend;
  readonly bridge: WidgetBridge;
  readonly saved: unknown[];
  readonly synced: (readonly WidgetWindowSpec[])[];
  /** Push a move or a close as if it came from a widget window. */
  emitMoved(move: WidgetMove): void;
  emitClosed(edit: WidgetEdit): void;
  emitChanged(spec: WidgetWindowSpec): void;
}

function harness(stored: unknown = [], live?: (specs: readonly WidgetWindowSpec[]) => string[]) {
  const saved: unknown[] = [];
  const synced: (readonly WidgetWindowSpec[])[] = [];
  const moved = new Set<(move: WidgetMove) => void>();
  const closed = new Set<(edit: WidgetEdit) => void>();
  const changed = new Set<(spec: WidgetWindowSpec) => void>();

  const backend: KeyedBackend = {
    load: () => Promise.resolve(stored),
    save: (value) => {
      saved.push(value);
      return Promise.resolve();
    },
  };

  const bridge: WidgetBridge = {
    sync: (specs) => {
      synced.push(specs);
      return Promise.resolve(live === undefined ? specs.map((spec) => spec.id) : live(specs));
    },
    get: () => Promise.resolve(null),
    close: () => Promise.resolve(),
    setFlags: () => Promise.resolve(),
    openSettings: () => Promise.resolve(),
    onMoved: (handler) => {
      moved.add(handler);
      return () => moved.delete(handler);
    },
    onClosed: (handler) => {
      closed.add(handler);
      return () => closed.delete(handler);
    },
    onChanged: (handler) => {
      changed.add(handler);
      return () => changed.delete(handler);
    },
    onEdit: () => () => undefined,
    onConfig: () => () => undefined,
  };

  return {
    backend,
    bridge,
    saved,
    synced,
    emitMoved: (move) => {
      for (const handler of moved) handler(move);
    },
    emitClosed: (edit) => {
      for (const handler of closed) handler(edit);
    },
    emitChanged: (spec) => {
      for (const handler of changed) handler(spec);
    },
  } satisfies Harness;
}

function mount(options: WidgetStoreOptions) {
  return renderHook(() => useWidgets(options));
}

/** A screen source a test controls, so no host and no browser are needed. */
function screenSource(...screens: readonly ScreenRect[]) {
  return createScreenSource(() => Promise.resolve(screens));
}

beforeEach(() => {
  vi.useRealTimers();
});

it('restores the stored layout so a restart does not lose the widgets', async () => {
  const io = harness([
    {
      id: 'cpu-1',
      kind: 'cpu',
      size: 'large',
      style: 'glass',
      layout: 'ring',
      opacity: 0.5,
      alwaysOnTop: false,
      remoteConnectionId: null,
      clickThrough: true,
      x: 400,
      y: 120,
    },
  ]);

  const { result } = mount(io);
  await waitFor(() => {
    expect(result.current.hydrated).toBe(true);
  });

  expect(result.current.widgets).toHaveLength(1);
  expect(result.current.widgets[0]).toMatchObject({
    id: 'cpu-1',
    size: 'large',
    style: 'glass',
    opacity: 0.5,
    clickThrough: true,
  });
});

it('falls back to an empty layout instead of throwing on an unreadable file', async () => {
  // A corrupt settings file is a cosmetic problem. Refusing to open the app
  // because of one is not an acceptable recovery.
  const io = harness({ not: 'an array' });

  const { result } = mount(io);
  await waitFor(() => {
    expect(result.current.hydrated).toBe(true);
  });

  expect(result.current.widgets).toEqual([]);
});

it('collapses a burst of edits into one push, so a drag is not a round trip per frame', async () => {
  const io = harness([placed('cpu-1')]);
  const { result } = mount(io);
  await waitFor(() => {
    expect(result.current.hydrated).toBe(true);
  });
  await waitFor(() => {
    expect(io.synced.length).toBeGreaterThan(0);
  });
  const pushes = io.synced.length;

  // A drag reports a position per frame. Each edit is flushed as its own render
  // (so React's batching is not what does the work here) and all three land well
  // inside one debounce window: the host must be told once, not three times.
  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

  for (const opacity of [0.5, 0.6, 0.7]) {
    act(() => {
      result.current.update('cpu-1', { opacity });
    });
    await sleep(10);
  }
  await sleep(30);

  expect(io.synced).toHaveLength(pushes);

  await waitFor(() => {
    expect(io.synced).toHaveLength(pushes + 1);
  });

  // Collapsed, not lossy: the host is given the newest state, not the first one.
  expect(io.synced[pushes]?.[0]?.opacity).toBe(0.7);
});

it('keeps the configuration when the host builds fewer windows than asked for', async () => {
  // An unbuildable spec — a window-manager refusal, a display that went away —
  // is indistinguishable here from a widget the user closed, and deleting the
  // user's configuration on that ambiguity is the one unrecoverable mistake.
  const io = harness([], () => []);
  const { result } = mount(io);
  await waitFor(() => {
    expect(result.current.hydrated).toBe(true);
  });

  act(() => {
    result.current.add('cpu', 'cpu-1');
  });
  await waitFor(() => {
    expect(io.synced.length).toBeGreaterThan(0);
  });

  expect(result.current.widgets.map((widget) => widget.id)).toEqual(['cpu-1']);
  expect(result.current.open).toEqual([]);
});

it('removes a widget that reports itself closed', async () => {
  const io = harness();
  const { result } = mount(io);
  await waitFor(() => {
    expect(result.current.hydrated).toBe(true);
  });
  act(() => {
    result.current.add('cpu', 'cpu-1');
  });

  act(() => {
    io.emitClosed({ id: 'cpu-1' });
  });

  expect(result.current.widgets).toEqual([]);
});

it('stores the position the user dragged a widget to', async () => {
  const io = harness();
  const { result } = mount(io);
  await waitFor(() => {
    expect(result.current.hydrated).toBe(true);
  });
  act(() => {
    result.current.add('cpu', 'cpu-1');
  });

  act(() => {
    io.emitMoved({ id: 'cpu-1', x: 640, y: 300 });
  });

  expect(result.current.widgets[0]).toMatchObject({ x: 640, y: 300 });
});

it('mirrors a flag flipped inside the widget so the settings screen agrees', async () => {
  const io = harness();
  const { result } = mount(io);
  await waitFor(() => {
    expect(result.current.hydrated).toBe(true);
  });
  act(() => {
    result.current.add('cpu', 'cpu-1');
  });

  act(() => {
    io.emitChanged({
      id: 'cpu-1',
      kind: 'cpu',
      size: 'small',
      style: 'plain',
      layout: 'stack',
      opacity: 1,
      alwaysOnTop: true,
      remoteConnectionId: null,
      clickThrough: true,
      x: 10,
      y: 20,
      width: 190,
      height: 120,
    });
  });

  expect(result.current.widgets[0]).toMatchObject({ alwaysOnTop: true, clickThrough: true });
});

it('refuses to place more widgets than the host will build', async () => {
  const io = harness();
  const { result } = mount(io);
  await waitFor(() => {
    expect(result.current.hydrated).toBe(true);
  });

  act(() => {
    for (let index = 0; index < MAX_WIDGETS + 3; index += 1) {
      result.current.add('cpu', `cpu-${String(index)}`);
    }
  });

  expect(result.current.widgets).toHaveLength(MAX_WIDGETS);
});

it('writes the layout to disk after a change', async () => {
  const io = harness();
  const { result } = mount(io);
  await waitFor(() => {
    expect(result.current.hydrated).toBe(true);
  });

  act(() => {
    result.current.add('memory', 'mem-1');
  });

  await waitFor(() => {
    expect(io.saved.length).toBeGreaterThan(0);
  });
  const last = io.saved.at(-1) as readonly { id: string }[];
  expect(last.map((widget) => widget.id)).toEqual(['mem-1']);
});

describe('the empty layout', () => {
  it('writes nothing to disk before it has read what was stored', async () => {
    // The load is asynchronous, so a save fired before it lands would overwrite
    // the stored layout with an empty list — the loss this guard exists for.
    const io = harness([{ id: 'cpu-1', kind: 'cpu' }]);

    mount(io);
    await waitFor(() => {
      expect(io.saved.length).toBeGreaterThan(0);
    });

    const first = io.saved[0] as readonly { id: string }[];
    expect(first.map((widget) => widget.id)).toEqual(['cpu-1']);
  });
});

describe('the grid a dragged widget lands on', () => {
  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

  it('stores the snapped position, not the raw pointer coordinate', async () => {
    const io = harness();
    const { result } = mount(io);
    await waitFor(() => {
      expect(result.current.hydrated).toBe(true);
    });
    act(() => {
      result.current.add('cpu', 'cpu-1');
    });

    act(() => {
      io.emitMoved({ id: 'cpu-1', x: 103, y: 207 });
    });

    expect(result.current.widgets[0]).toMatchObject({ x: 104, y: 208 });

    // And the snapped value is what is persisted, so a restart agrees with the
    // screen rather than re-deriving a different position from the raw one.
    await waitFor(() => {
      const last = io.saved.at(-1) as readonly { x: number; y: number }[];
      expect(last[0]).toMatchObject({ x: 104, y: 208 });
    });
  });

  it('schedules no write when a later move lands on the same grid cell', async () => {
    // The point of snapping, beyond two widgets lining up: every tick of a drag
    // after the first rounds to the cell the widget is already in, so a slow
    // drag does not turn into one disk write per frame.
    const io = harness();
    const { result } = mount(io);
    await waitFor(() => {
      expect(result.current.hydrated).toBe(true);
    });
    act(() => {
      result.current.add('cpu', 'cpu-1');
    });

    act(() => {
      io.emitMoved({ id: 'cpu-1', x: 103, y: 207 });
    });
    await waitFor(() => {
      expect(io.saved.length).toBeGreaterThan(0);
    });
    await sleep(300);
    const writes = io.saved.length;

    // 105 and 209 round to the same 104/208, so nothing changed.
    act(() => {
      io.emitMoved({ id: 'cpu-1', x: 105, y: 209 });
    });
    await sleep(400);

    expect(io.saved).toHaveLength(writes);
  });
});

describe('the cascade for a new widget', () => {
  it('does not drop a new widget on one that is already placed', async () => {
    // The two widgets sit on the first two cascade slots, then the first is
    // removed: the list length is one, so the index-based default would land the
    // newcomer on slot one — which the survivor already occupies.
    const first = defaultPosition(0);
    const second = defaultPosition(1);
    const io = harness([
      { ...placed('cpu-1'), x: first.x, y: first.y },
      { ...placed('mem-1'), x: second.x, y: second.y },
    ]);
    const { result } = mount(io);
    await waitFor(() => {
      expect(result.current.hydrated).toBe(true);
    });

    act(() => {
      result.current.remove('cpu-1');
    });
    act(() => {
      result.current.add('cpu', 'cpu-2');
    });

    const added = result.current.widgets.find((widget) => widget.id === 'cpu-2');
    expect(added).toBeDefined();
    expect({ x: added?.x, y: added?.y }).not.toEqual({ x: second.x, y: second.y });
    // And it went to the first slot, so the cascade still fills in order.
    expect({ x: added?.x, y: added?.y }).toEqual({ x: first.x, y: first.y });
  });
});

describe('a widget whose screen is gone', () => {
  const LAPTOP: ScreenRect = { x: 0, y: 0, width: 1920, height: 1080 };

  it('comes back onto a screen on its own, without anyone asking', async () => {
    const io = harness([{ ...placed('ghost-1'), x: 9000, y: 9000 }]);
    const { result } = mount({ ...io, screens: screenSource(LAPTOP) });

    // 1920 - 280 = 1640; 1080 - 170 = 910 for a medium widget.
    await waitFor(() => {
      expect(result.current.widgets[0]).toMatchObject({ x: 1640, y: 910 });
    });

    // Persisted, so the recovery survives a restart rather than being applied
    // again on every open.
    await waitFor(() => {
      const last = io.saved.at(-1) as readonly { id: string; x: number }[];
      expect(last[0]).toMatchObject({ id: 'ghost-1', x: 1640 });
    });
  });

  it('can also be asked for, and asking twice changes nothing the second time', async () => {
    const io = harness([{ ...placed('ghost-1'), x: 9000, y: 9000 }]);
    const { result } = mount({ ...io, screens: screenSource(LAPTOP) });
    await waitFor(() => {
      expect(result.current.widgets[0]).toMatchObject({ x: 1640, y: 910 });
    });
    await waitFor(() => {
      expect(io.saved.length).toBeGreaterThan(0);
    });
    const writes = io.saved.length;

    act(() => {
      result.current.bringAllOnScreen();
    });
    const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
    await sleep(400);

    expect(result.current.widgets[0]).toMatchObject({ x: 1640, y: 910 });
    expect(io.saved).toHaveLength(writes);
  });

  it('leaves a widget that is already on a screen exactly where it is', async () => {
    const io = harness([{ ...placed('cpu-1'), x: 40, y: 40 }]);
    const { result } = mount({ ...io, screens: screenSource(LAPTOP) });
    await waitFor(() => {
      expect(result.current.hydrated).toBe(true);
    });
    await waitFor(() => {
      expect(io.saved.length).toBeGreaterThan(0);
    });
    const writes = io.saved.length;

    act(() => {
      result.current.bringAllOnScreen();
    });
    await waitFor(() => {
      expect(result.current.widgets[0]).toMatchObject({ x: 40, y: 40 });
    });

    const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));
    await sleep(400);

    // Nothing moved, so nothing was written: the action must not rewrite the
    // file just because it was called.
    expect(io.saved).toHaveLength(writes);
  });

  it('moves nothing when no screen could be learned', async () => {
    const io = harness([{ ...placed('ghost-1'), x: 9000, y: 9000 }]);
    const { result } = mount({ ...io, screens: screenSource() });
    await waitFor(() => {
      expect(result.current.hydrated).toBe(true);
    });

    warned.length = 0;
    act(() => {
      result.current.bringAllOnScreen();
    });

    // An empty answer is not a licence to guess: a widget merely sitting on
    // another monitor must not be flung somewhere else on a guess.
    await waitFor(() => {
      expect(warned).toHaveLength(1);
    });
    expect(result.current.widgets[0]).toMatchObject({ x: 9000, y: 9000 });

    // And it says why. Without this assertion the guard is invisible: with it
    // removed the outcome is the same, so nothing else could tell a reader that
    // the branch still runs.
    expect(warned[0]?.[0]).toBe('warn');
    expect(warned[0]?.[2]).toBe('widgets');
    expect(String(warned[0]?.[1])).toContain('no screens known');
  });
});
