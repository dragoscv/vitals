/**
 * The settings screen's guarantees.
 *
 * The screen is the only place a widget can be configured, and the only way out
 * of a click-through window, so the properties asserted here are the ones that
 * make it usable rather than the ones that make it look right: the controls
 * wait for the stored layout before they can be pressed, a kind is never
 * offered a layout that renders nothing, every action is a real control a
 * keyboard can reach, and asking to edit a widget from the widget itself lands
 * on that widget's row.
 *
 * The backend is injected because the read has to be held open: without that
 * every render here is already hydrated and "the controls wait for the stored
 * layout" is untestable. The machine list is injected the same way, because a
 * widget's source is the one setting that decides whether the card talks to a
 * network at all.
 */

import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { act } from 'react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { KeyedBackend } from '@/settings/persistence';

import { makeSystem } from '../dashboard/test-fixtures';
import { INITIAL_SYSTEM_SNAPSHOT, createManualSystemSource } from '../dashboard/useSystemSnapshot';
import type { MachineEntry } from './MachinesPanel';
import {
  MAX_WIDGETS,
  toWindowSpec,
  widgetKinds,
  type WidgetInstance,
  type WidgetKind,
  type WidgetWindowSpec,
} from './model';
import { setRemoteConnections } from './remote';
import { makeSummary, scriptedFetch } from './remote.fixtures';
import {
  WIDGET_EDIT_EVENT,
  createScreenSource,
  type ScreenRect,
  type WidgetBridge,
} from './windows';
import { WidgetsScreen } from './WidgetsScreen';

/**
 * A liveness bound for the waits that depend on a debounce, not a speed
 * assertion.
 *
 * The screen hands its windows to the host through a 120 ms debounce, so an
 * assertion about what the host received is waiting on a timer rather than on a
 * render. On the loaded release Verify runner (its Android lane builds beside
 * this one) the default 1 s wait expired before that timer fired on the commit
 * of 2026-11-28 — a statement about the machine, not about the code, and the
 * same bound `DashboardScreen.test.tsx` needed on 2026-11-27. If the push never
 * happens the test still fails, loud, just later.
 */
const LIVENESS_TIMEOUT_MS = 10_000;

beforeAll(async () => {
  await initI18n('en');
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

afterEach(() => {
  // The remote registry is module state, so a machine registered by one test
  // would otherwise be polled by the next one's widget.
  setRemoteConnections([]);
  vi.unstubAllGlobals();
});

/** One stored widget, complete enough to survive `parseWidgets`. */
function stored(
  kind: WidgetInstance['kind'],
  id: string,
  overrides: Partial<WidgetInstance> = {},
): WidgetInstance {
  return {
    id,
    kind,
    size: 'small',
    style: 'plain',
    layout: kind === 'cpu' ? 'ring' : 'stack',
    opacity: 1,
    alwaysOnTop: true,
    remoteConnectionId: null,
    clickThrough: false,
    x: 40,
    y: 40,
    ...overrides,
  };
}

/** A machine a widget can be pointed at. */
const RIG: MachineEntry = {
  id: 'rig',
  label: 'Rig',
  baseUrl: 'http://rig:7331',
  token: 'shared-secret',
};

/** One monitor, big enough to hold a widget at 40, 40. */
const LAPTOP: ScreenRect = { x: 0, y: 0, width: 1920, height: 1080 };

/** A backend holding `value`, optionally holding the answer back. */
function memoryBackend(value: unknown, gate?: Promise<void>): KeyedBackend {
  return {
    load: async () => {
      if (gate !== undefined) await gate;
      return value;
    },
    save: async () => {},
  };
}

/** A promise the test releases, for holding the stored layout back. */
function deferred(): { promise: Promise<void>; release: () => void } {
  let release = (): void => {};
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  return { promise, release };
}

interface Harness {
  readonly bridge: WidgetBridge;
  readonly synced: WidgetWindowSpec[][];
  /** Emits what a widget's own settings button sends. */
  readonly edited: (id: string) => void;
}

function harness(): Harness {
  const synced: WidgetWindowSpec[][] = [];
  const edits = new Set<(payload: { id: string }) => void>();

  const bridge: WidgetBridge = {
    sync: async (specs) => {
      synced.push([...specs]);
      return specs.map((spec) => spec.id);
    },
    get: async () => null,
    close: async () => {},
    setFlags: async () => {},
    openSettings: async () => {},
    onMoved: () => () => {},
    onClosed: () => () => {},
    onChanged: () => () => {},
    onEdit: (listener) => {
      edits.add(listener);
      return () => {
        edits.delete(listener);
      };
    },
    onConfig: () => () => {},
  };

  return {
    bridge,
    synced,
    edited: (id) => {
      act(() => {
        for (const listener of edits) listener({ id });
      });
    },
  };
}

interface ShowOptions {
  readonly registry: unknown;
  /** Holds the stored widget layout back, so the pre-hydration state is observable. */
  readonly gate?: Promise<void>;
  /** The machines the widget sources may point at. Absent means none configured. */
  readonly machines?: readonly MachineEntry[];
  /** Where the monitors are, for the bring-back action. Absent uses the real one. */
  readonly screens?: ReturnType<typeof createScreenSource>;
}

/** The screen wired to a machine reporting one CPU at half load. */
function showWith(options: ShowOptions): Harness {
  const io = harness();
  const source = createManualSystemSource({
    ...INITIAL_SYSTEM_SNAPSHOT,
    system: makeSystem({ cpu: { total: 50, temperature: 61 } }),
    seq: 1,
    timestampMs: 1_760_000_000_000,
    pending: false,
  });
  render(
    <WidgetsScreen
      source={source}
      bridge={io.bridge}
      backend={memoryBackend(options.registry, options.gate)}
      {...(options.machines !== undefined && {
        machinesBackend: memoryBackend(options.machines),
      })}
      {...(options.screens !== undefined && { screens: options.screens })}
    />,
  );
  return io;
}

/** The screen wired to a machine reporting one CPU at half load, with no machines. */
function show(registry: unknown, gate?: Promise<void>): Harness {
  return showWith({ registry, ...(gate !== undefined && { gate }) });
}

/**
 * The gallery's place buttons.
 *
 * Named rather than found by container: the machines panel below the list has
 * its own submit button, so "every button on the screen" would make a test
 * about placing a widget pass or fail on an unrelated form.
 */
function placeTiles(): HTMLButtonElement[] {
  return screen.getAllByRole('button', { name: /Add a widget/u });
}

/**
 * The tile for one kind.
 *
 * Its accessible name is the tile's own heading followed by the button's own
 * text, because the button is labelled by both — so "CPU Add a widget" rather
 * than a name invented for the test.
 */
function placeTile(kind: WidgetKind): HTMLButtonElement {
  return screen.getByRole('button', {
    name: `${i18n.t(`widgets.kind.${kind}`)} ${i18n.t('widgets.add')}`,
  });
}

/**
 * The list row holding one widget, so an assertion cannot match a neighbour.
 *
 * Awaited, because the list does not exist until the stored layout has been
 * read: looking for a card immediately after rendering would be asking about a
 * screen that has not opened yet.
 */
async function cardFor(label: string): Promise<HTMLElement> {
  const row = (await screen.findByRole('button', { name: `Remove the ${label} widget` })).closest(
    'li',
  );
  if (row === null) throw new Error(`the ${label} card is not in a list row`);
  return row;
}

/** The segment labels one control offers, in order. */
function segments(control: HTMLElement): (string | undefined)[] {
  return [...control.querySelectorAll('[role="radio"]')].map(
    (item) => item.textContent?.trim() ?? undefined,
  );
}

/** Every element the screen has marked as the row a widget asked for. */
function marked(): Element[] {
  return [...document.querySelectorAll('[class*="border-[var(--color-accent)]"]')];
}

describe('the widget settings screen', () => {
  it('refuses to add a widget until it has read the stored layout', async () => {
    const hold = deferred();
    show([], hold.promise);

    // The read is held open: adding now would write over a layout that has not
    // arrived yet, and the user would lose a widget they already placed.
    expect(placeTiles().every((button) => button.disabled)).toBe(true);

    await act(async () => {
      hold.release();
    });

    await waitFor(() => {
      expect(placeTiles().every((button) => !button.disabled)).toBe(true);
    });
  });

  it('stops offering a kind once the registry is full and names the ceiling', async () => {
    show(Array.from({ length: MAX_WIDGETS }, (_, index) => stored('cpu', `cpu-${index + 1}`)));

    await waitFor(() => {
      expect(placeTiles().every((button) => button.disabled)).toBe(true);
    });
    expect(screen.getByText(`Up to ${MAX_WIDGETS} at once.`)).toBeDefined();
  });

  it('offers a ring gauge only to a kind whose number reads as a percentage', async () => {
    show([stored('cpu', 'cpu-1'), stored('network', 'network-1')]);

    await waitFor(() => {
      expect(screen.getAllByRole('radiogroup', { name: 'Layout' }).length).toBe(2);
    });

    const [cpuCard, networkCard] = screen.getAllByRole('radiogroup', { name: 'Layout' });
    expect(segments(cpuCard!)).toEqual(['Ring', 'Stack', 'Split']);
    // The byte counters have nothing to put in a 0–100 dial, so the ring is not
    // offered rather than offered and rendering an empty gauge.
    expect(segments(networkCard!)).toEqual(['Stack', 'Split']);
  });

  it('names each card after the kind of reading it shows', async () => {
    show([stored('thermal', 'thermal-1')]);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Thermals' })).toBeDefined();
    });
  });

  it('gives every action on a card a control a keyboard can reach', async () => {
    const io = show([stored('memory', 'memory-1')]);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Memory' })).toBeDefined();
    });

    // A node with no role and no tabindex is invisible to a keyboard even when
    // it carries a click handler, so each affordance has to exist as a real
    // control: the buttons cover collapse and remove, the slider covers
    // opacity, the two switches cover the window flags.
    const controls: HTMLElement[] = [
      ...screen.getAllByRole('button'),
      ...screen.getAllByRole('switch'),
      screen.getByRole('slider', { name: /Opacity/u }),
    ];

    expect(screen.getAllByRole('switch').length).toBe(2);
    // The element type goes in the generic rather than in an assertion: the
    // lint rule treats the two as equivalent and prefers the one that cannot
    // disagree with itself.
    expect(screen.getByRole<HTMLInputElement>('slider', { name: /Opacity/u }).type).toBe('range');
    for (const control of controls) {
      expect(control.tabIndex).toBeGreaterThanOrEqual(0);
    }

    // And the list really did reach the host, so those controls act on real
    // windows rather than on a screen nothing is listening to.
    await waitFor(
      () => {
        expect(io.synced.length).toBeGreaterThan(0);
      },
      { timeout: LIVENESS_TIMEOUT_MS },
    );
  });

  it('marks the row a widget asked for when its own settings button is pressed', async () => {
    const io = show([stored('cpu', 'cpu-1'), stored('disk', 'disk-1')]);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'Disk' })).toBeDefined();
    });

    expect(marked().length).toBe(0);
    io.edited('disk-1');

    // Scrolling alone leaves the user guessing which of two similar rows moved,
    // so the row is marked as well.
    await waitFor(() => {
      expect(marked().length).toBe(1);
    });
    const row = marked()[0]!.closest('li');
    expect(row?.textContent).toContain('Disk');
    expect(row?.textContent).not.toContain('CPU');
  });

  it('still names a widget the host refused to put on screen', async () => {
    const io = show([stored('cpu', 'cpu-1')]);
    // A window manager saying no is a live possibility, so the host is asked to
    // build nothing.
    io.bridge.sync = async () => [];

    await waitFor(() => {
      expect(screen.getByText('Not on screen')).toBeDefined();
    });

    // The row survives: a refusal is reported, not used as a licence to delete
    // the user's configuration, and it still says which widget it is. Scoped to
    // the card, because the gallery above carries the same kind name as a tile.
    const card = await cardFor('CPU');
    expect(within(card).getByRole('heading', { name: 'CPU' })).toBeDefined();
  });

  it('describes a widget through the same reading list the window shows', async () => {
    show([stored('cpu', 'cpu-1')]);

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: 'CPU' })).toBeDefined();
    });

    // The preview is drawn from `describe`, so a reading the card shows is a
    // reading the widget has. `getAllByText` because an animated value renders
    // twice: once visibly and once for a screen reader.
    expect(screen.getAllByText('50%').length).toBeGreaterThan(0);
    expect(screen.getByText('at 40, 40')).toBeDefined();
  });

  it('hands the host exactly the windows the model builds from the stored list', async () => {
    const io = show([stored('cpu', 'cpu-1')]);

    await waitFor(
      () => {
        expect(io.synced.length).toBeGreaterThan(0);
      },
      { timeout: LIVENESS_TIMEOUT_MS },
    );

    expect(io.synced.at(-1)).toEqual([toWindowSpec(stored('cpu', 'cpu-1'))]);
  });

  it('offers every kind in the gallery and places the one whose tile was pressed', async () => {
    const io = show([]);

    await waitFor(() => {
      expect(placeTiles().every((button) => !button.disabled)).toBe(true);
    });

    // One tile per kind the model knows. A gallery offering fewer would cap the
    // feature at whatever the screen happened to remember.
    expect(placeTiles()).toHaveLength(widgetKinds.length);

    fireEvent.click(placeTile('thermal'));

    await waitFor(() => {
      expect(screen.getByRole('button', { name: 'Remove the Thermals widget' })).toBeDefined();
    });
    // And it went through the controller rather than beside it: the host was
    // handed the record the model builds, so id minting and the write happen
    // once, in one place. The write is debounced, so this waits for the host
    // rather than for the click.
    await waitFor(
      () => {
        expect(io.synced.at(-1)?.map((spec) => spec.kind)).toEqual(['thermal']);
      },
      { timeout: LIVENESS_TIMEOUT_MS },
    );
  });

  it('points a widget at the machine the user picked, and back at this computer', async () => {
    const io = showWith({ registry: [stored('cpu', 'cpu-1')], machines: [RIG] });

    const picker = await waitFor(() =>
      screen.getByRole('radiogroup', {
        name: i18n.t('widgets.sourceFor', { kind: i18n.t('widgets.kind.cpu') }),
      }),
    );

    fireEvent.click(within(picker).getByRole('radio', { name: RIG.label }));

    await waitFor(
      () => {
        expect(io.synced.at(-1)?.[0]?.remoteConnectionId).toBe(RIG.id);
      },
      { timeout: LIVENESS_TIMEOUT_MS },
    );

    // Back to this computer must CLEAR the id. Storing the sentinel, or leaving
    // the old id behind, would sign the card "This machine" while it was still
    // polling a LAN address.
    fireEvent.click(
      within(picker).getByRole('radio', { name: i18n.t('widgets.source.thisMachine') }),
    );

    await waitFor(
      () => {
        expect(io.synced.at(-1)?.[0]?.remoteConnectionId).toBeNull();
      },
      { timeout: LIVENESS_TIMEOUT_MS },
    );
  });

  it('draws the reading from the machine a widget watches, not this computer', async () => {
    // Two machines reporting different numbers: 12 % on the far one, 50 % here.
    // The assertion is not that a percentage appears but WHICH percentage does.
    const far = makeSystem({ cpu: { total: 12, temperature: 40 } });
    const net = scriptedFetch(
      ...Array.from({ length: 6 }, () => ({
        status: 200,
        body: JSON.stringify(makeSummary(far)),
      })),
    );
    vi.stubGlobal('fetch', net.impl);

    showWith({
      registry: [stored('cpu', 'cpu-1', { remoteConnectionId: RIG.id })],
      machines: [RIG],
    });

    const card = await cardFor('CPU');
    await waitFor(() => {
      expect(within(card).getAllByText('12%').length).toBeGreaterThan(0);
    });

    expect(within(card).queryByText('50%')).toBeNull();
    expect(net.calls[0]).toBe(`${RIG.baseUrl}/api/v1/summary`);
  });

  it('keeps showing this computer while no machine has been chosen', async () => {
    const net = scriptedFetch({ status: 200, body: '{}' });
    vi.stubGlobal('fetch', net.impl);

    showWith({ registry: [stored('cpu', 'cpu-1')], machines: [RIG] });

    const card = await cardFor('CPU');
    await waitFor(() => {
      expect(within(card).getAllByText('50%').length).toBeGreaterThan(0);
    });

    expect(within(card).queryByText('12%')).toBeNull();
    // A configured machine is not a polled machine: a widget with no source
    // asks nobody for anything, which is what keeps the default offline.
    expect(net.calls).toEqual([]);
  });

  it('offers to bring the widgets back on screen, dead until the layout has arrived', async () => {
    const hold = deferred();
    let asked = 0;
    const screens = createScreenSource(() => {
      asked += 1;
      return Promise.resolve([LAPTOP]);
    });
    showWith({ registry: [stored('cpu', 'cpu-1')], gate: hold.promise, screens });

    const button = screen.getByRole<HTMLButtonElement>('button', {
      name: i18n.t('widgets.bringBack'),
    });
    // Asking before the stored list has been read would move widgets that have
    // not been loaded yet, so the action waits for the same hydration as the
    // rest of the screen.
    expect(button.disabled).toBe(true);

    await act(async () => {
      hold.release();
    });

    await waitFor(() => {
      expect(button.disabled).toBe(false);
    });
    // The store runs one pass of its own the moment it hydrates; the button has
    // to ask a second time rather than lean on that pass having happened.
    await waitFor(() => {
      expect(asked).toBe(1);
    });

    fireEvent.click(button);

    await waitFor(() => {
      expect(asked).toBe(2);
    });
  });
});

describe('the edit request', () => {
  it('travels on the event name the widget window publishes', () => {
    // The two sides build separately and nothing else checks that the name the
    // window emits is the name this screen subscribes to.
    expect(WIDGET_EDIT_EVENT).toBe('vitals://widget-edit');
  });
});
