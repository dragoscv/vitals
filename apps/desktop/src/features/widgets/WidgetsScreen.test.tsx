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
 * layout" is untestable.
 */

import { render, screen, waitFor } from '@testing-library/react';
import { act } from 'react';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { KeyedBackend } from '@/settings/persistence';

import { makeSystem } from '../dashboard/test-fixtures';
import { INITIAL_SYSTEM_SNAPSHOT, createManualSystemSource } from '../dashboard/useSystemSnapshot';
import { MAX_WIDGETS, toWindowSpec, type WidgetInstance, type WidgetWindowSpec } from './model';
import { WIDGET_EDIT_EVENT, type WidgetBridge } from './windows';
import { WidgetsScreen } from './WidgetsScreen';

beforeAll(async () => {
  await initI18n('en');
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
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
    clickThrough: false,
    x: 40,
    y: 40,
    ...overrides,
  };
}

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

/** The screen wired to a machine reporting one CPU at half load. */
function show(registry: unknown, gate?: Promise<void>): Harness {
  const io = harness();
  const source = createManualSystemSource({
    ...INITIAL_SYSTEM_SNAPSHOT,
    system: makeSystem({ cpu: { total: 50, temperature: 61 } }),
    seq: 1,
    timestampMs: 1_760_000_000_000,
    pending: false,
  });
  render(
    <WidgetsScreen source={source} bridge={io.bridge} backend={memoryBackend(registry, gate)} />,
  );
  return io;
}

/** The add buttons, in the order the content kinds are declared. */
function addButtons(): HTMLButtonElement[] {
  return [...screen.getByRole('group', { name: 'Add a widget' }).querySelectorAll('button')];
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
    expect(addButtons().every((button) => button.disabled)).toBe(true);

    await act(async () => {
      hold.release();
    });

    await waitFor(() => {
      expect(addButtons().every((button) => !button.disabled)).toBe(true);
    });
  });

  it('stops offering a kind once the registry is full and names the ceiling', async () => {
    show(Array.from({ length: MAX_WIDGETS }, (_, index) => stored('cpu', `cpu-${index + 1}`)));

    await waitFor(() => {
      expect(addButtons().every((button) => button.disabled)).toBe(true);
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
    await waitFor(() => {
      expect(io.synced.length).toBeGreaterThan(0);
    });
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
    // the user's configuration.
    expect(screen.getByRole('heading', { name: 'CPU' })).toBeDefined();
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

    await waitFor(() => {
      expect(io.synced.length).toBeGreaterThan(0);
    });

    expect(io.synced.at(-1)).toEqual([toWindowSpec(stored('cpu', 'cpu-1'))]);
  });
});

describe('the edit request', () => {
  it('travels on the event name the widget window publishes', () => {
    // The two sides build separately and nothing else checks that the name the
    // window emits is the name this screen subscribes to.
    expect(WIDGET_EDIT_EVENT).toBe('vitals://widget-edit');
  });
});
