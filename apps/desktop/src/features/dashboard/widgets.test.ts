import { describe, expect, it } from 'vitest';

import {
  addWidget,
  availableToAdd,
  capabilitiesFrom,
  defaultLayout,
  isAvailable,
  moveWidget,
  parseLayout,
  removeWidget,
  resizeWidget,
  visibleLayout,
  widgetById,
  widgetCatalogue,
  type DashboardLayout,
  type WidgetCapabilities,
} from './widgets';
import { makeSystem } from './test-fixtures';

const ALL: WidgetCapabilities = { hasGpu: true, hasBattery: true, hasThermal: true };
const NONE: WidgetCapabilities = { hasGpu: false, hasBattery: false, hasThermal: false };

describe('parseLayout', () => {
  it('falls back to defaults for anything that is not an array', () => {
    // The layout file is plain JSON in a roaming profile and is trivially
    // hand-editable, so every one of these is a real thing that can arrive.
    for (const raw of [null, undefined, 42, 'cpu', {}, true]) {
      expect(parseLayout(raw)).toEqual(defaultLayout);
    }
  });

  it('drops ids this build no longer knows about', () => {
    // The upgrade case: a widget removed in a new version must not take the
    // dashboard down with it.
    const layout = parseLayout([
      { id: 'cpu', size: 'half' },
      { id: 'wormhole', size: 'full' },
    ]);
    expect(layout).toEqual([{ id: 'cpu', size: 'half' }]);
  });

  it('drops duplicates, keeping the first', () => {
    // A duplicate id would render two React children with the same key, which
    // corrupts reconciliation rather than merely looking odd.
    const layout = parseLayout([
      { id: 'cpu', size: 'half' },
      { id: 'cpu', size: 'full' },
    ]);
    expect(layout).toEqual([{ id: 'cpu', size: 'half' }]);
  });

  it('repairs an invalid size to the widget default', () => {
    const layout = parseLayout([{ id: 'alerts', size: 'enormous' }]);
    expect(layout).toEqual([{ id: 'alerts', size: widgetById.get('alerts')?.defaultSize }]);
  });

  it('returns defaults when nothing survived, rather than an empty dashboard', () => {
    // An empty grid reads as a broken app. Defaults read as a fresh one.
    expect(parseLayout([{ id: 'gone' }, 'junk', null])).toEqual(defaultLayout);
  });

  it('round-trips a layout it produced', () => {
    const layout = parseLayout(JSON.parse(JSON.stringify(defaultLayout)) as unknown);
    expect(layout).toEqual(defaultLayout);
  });
});

describe('capabilitiesFrom', () => {
  it('reports nothing when there is no frame yet', () => {
    expect(capabilitiesFrom(null)).toEqual(NONE);
  });

  it('treats a null temperature as no thermal support', () => {
    // This is the distinction that matters: the field always exists, and the
    // Rust side reports null for everything it cannot read without a ring-0
    // driver. Presence of the field is not presence of a sensor.
    const system = makeSystem({ cpu: { temperature: null }, gpus: [] });
    expect(capabilitiesFrom(system).hasThermal).toBe(false);
  });

  it('reports thermal support from a GPU alone', () => {
    const system = makeSystem({
      cpu: { temperature: null },
      gpus: [{ id: 0, temperature: 61 }],
    });
    expect(capabilitiesFrom(system).hasThermal).toBe(true);
  });

  it('reports a battery only when one is present', () => {
    expect(capabilitiesFrom(makeSystem({ battery: null })).hasBattery).toBe(false);
    expect(capabilitiesFrom(makeSystem({ battery: {} })).hasBattery).toBe(true);
  });
});

describe('isAvailable', () => {
  it('lets unconditional widgets through on any machine', () => {
    const cpu = widgetById.get('cpu');
    expect(cpu && isAvailable(cpu, NONE)).toBe(true);
  });

  it('gates hardware widgets on the matching capability', () => {
    const cases: readonly [string, keyof WidgetCapabilities][] = [
      ['gpu', 'hasGpu'],
      ['battery', 'hasBattery'],
      ['thermals', 'hasThermal'],
    ];

    for (const [id, capability] of cases) {
      const widget = widgetCatalogue.find((candidate) => candidate.id === id);
      expect(widget).toBeDefined();
      if (widget === undefined) continue;
      expect(isAvailable(widget, NONE)).toBe(false);
      expect(isAvailable(widget, { ...NONE, [capability]: true })).toBe(true);
    }
  });
});

describe('visibleLayout', () => {
  it('hides a widget the machine cannot support without deleting it', () => {
    // The undock case: pulling an eGPU must not permanently forget the widget,
    // because plugging it back in should restore the dashboard as it was.
    const stored: DashboardLayout = [
      { id: 'cpu', size: 'half' },
      { id: 'gpu', size: 'half' },
    ];

    expect(visibleLayout(stored, NONE)).toEqual([{ id: 'cpu', size: 'half' }]);
    expect(visibleLayout(stored, ALL)).toEqual(stored);
  });
});

describe('mutations', () => {
  const base: DashboardLayout = [
    { id: 'cpu', size: 'half' },
    { id: 'memory', size: 'half' },
    { id: 'disk', size: 'half' },
  ];

  it('refuses to remove an essential widget', () => {
    // CPU and memory are the reason the screen exists; a dashboard without
    // them is not a configuration anyone chose deliberately.
    expect(removeWidget(base, 'cpu')).toBe(base);
    expect(removeWidget(base, 'disk')).toEqual([
      { id: 'cpu', size: 'half' },
      { id: 'memory', size: 'half' },
    ]);
  });

  it('does not add a widget twice', () => {
    expect(addWidget(base, 'cpu')).toBe(base);
    expect(addWidget(base, 'network')).toHaveLength(4);
  });

  it('adds at the widget default size', () => {
    const layout = addWidget(base, 'alerts');
    expect(layout.at(-1)).toEqual({ id: 'alerts', size: 'full' });
  });

  it('resizes only the named widget', () => {
    const layout = resizeWidget(base, 'memory', 'full');
    expect(layout).toEqual([
      { id: 'cpu', size: 'half' },
      { id: 'memory', size: 'full' },
      { id: 'disk', size: 'half' },
    ]);
  });

  it('swaps with the neighbour rather than jumping', () => {
    // A "move up" that travels two places when the widget above is full-width
    // is indistinguishable from a bug.
    expect(moveWidget(base, 'disk', 'up')).toEqual([
      { id: 'cpu', size: 'half' },
      { id: 'disk', size: 'half' },
      { id: 'memory', size: 'half' },
    ]);
  });

  it('is a no-op at the ends', () => {
    expect(moveWidget(base, 'cpu', 'up')).toBe(base);
    expect(moveWidget(base, 'disk', 'down')).toBe(base);
  });

  it('ignores a widget that is not in the layout', () => {
    expect(moveWidget(base, 'benchmarks' as never, 'up')).toBe(base);
  });
});

describe('availableToAdd', () => {
  it('offers only what is supported and not already placed', () => {
    const offered = availableToAdd(defaultLayout, NONE).map((widget) => widget.id);

    expect(offered).not.toContain('cpu'); // already placed
    expect(offered).not.toContain('gpu'); // unsupported
    expect(offered).toContain('alerts');
  });

  it('offers nothing once everything supported is placed', () => {
    const full = widgetCatalogue.reduce<DashboardLayout>(
      (layout, widget) => addWidget(layout, widget.id),
      [],
    );
    expect(availableToAdd(full, ALL)).toHaveLength(0);
  });
});
