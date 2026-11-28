import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import type { KeyedBackend } from '@/settings/persistence';

import type { WidgetWindowSpec } from './model';
import { MAX_WIDGETS } from './model';
import { useWidgets } from './store';
import type { WidgetBridge, WidgetEdit, WidgetMove } from './windows';

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

function mount(options: { backend: KeyedBackend; bridge: WidgetBridge }) {
  return renderHook(() => useWidgets(options));
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
