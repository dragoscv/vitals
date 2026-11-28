import { describe, expect, it } from 'vitest';

import {
  addWidget,
  clampOpacity,
  defaultLayoutFor,
  defaultPosition,
  geometryFor,
  MAX_WIDGETS,
  MIN_OPACITY,
  newWidgetId,
  parseWidgets,
  removeWidget,
  sameRegistry,
  shouldAnimate,
  toWindowSpec,
  updateWidget,
  widgetIdFromLabel,
  widgetLabel,
  type WidgetInstance,
} from './model';

function aWidget(overrides: Partial<WidgetInstance> = {}): WidgetInstance {
  return {
    id: 'w1',
    kind: 'cpu',
    size: 'small',
    style: 'plain',
    layout: 'ring',
    opacity: 1,
    alwaysOnTop: true,
    clickThrough: false,
    x: 80,
    y: 80,
    ...overrides,
  };
}

describe('parsing a stored registry', () => {
  it('rejects a widget whose kind this build cannot render', () => {
    expect(parseWidgets([{ id: 'a', kind: 'quantum' }])).toEqual([]);
  });

  it('keeps the first of two widgets sharing an identifier', () => {
    const parsed = parseWidgets([
      { id: 'same', kind: 'cpu' },
      { id: 'same', kind: 'memory' },
    ]);

    expect(parsed).toHaveLength(1);
    expect(parsed[0]?.kind).toBe('cpu');
  });

  it('clamps an out-of-range opacity rather than dropping the widget', () => {
    const tooLoud = parseWidgets([{ id: 'a', kind: 'cpu', opacity: 5 }]);
    const tooFaint = parseWidgets([{ id: 'a', kind: 'cpu', opacity: 0.01 }]);

    expect(tooLoud[0]?.opacity).toBe(1);
    expect(tooFaint[0]?.opacity).toBe(MIN_OPACITY);
  });

  it('falls back to a layout the kind can actually draw', () => {
    // A ring gauge needs one 0–100 number; the disk widget has none.
    const parsed = parseWidgets([{ id: 'a', kind: 'disk', layout: 'ring' }]);

    expect(parsed[0]?.layout).toBe(defaultLayoutFor('disk'));
  });

  it('gives a widget with no stored position the first slot in the cascade', () => {
    const parsed = parseWidgets([{ id: 'a', kind: 'cpu' }]);

    expect(parsed[0]).toMatchObject(defaultPosition(0));
  });

  it('refuses a registry that is not a list at all', () => {
    expect(parseWidgets({ id: 'a' })).toEqual([]);
    expect(parseWidgets(null)).toEqual([]);
  });

  it('caps the list at the documented maximum, so a hand-edited file cannot exceed it', () => {
    const many = Array.from({ length: MAX_WIDGETS + 5 }, (_, index) => ({
      id: `w${index}`,
      kind: 'cpu',
    }));

    expect(parseWidgets(many)).toHaveLength(MAX_WIDGETS);
  });
});

describe('the stored shape round-trips', () => {
  it('survives being written as JSON and read back unchanged', () => {
    const placed = addWidget(addWidget([], 'cpu', 'a'), 'memory', 'b');
    const configured = updateWidget(placed, 'a', {
      size: 'large',
      style: 'glass',
      layout: 'split',
      opacity: 0.6,
      clickThrough: true,
      x: 400,
      y: 250,
    });

    const written = JSON.parse(JSON.stringify(configured)) as unknown;
    const read = parseWidgets(written);

    expect(sameRegistry(configured, read)).toBe(true);
  });
});

describe('editing the registry', () => {
  it('returns the same list when an update changes nothing', () => {
    const placed = [aWidget()];

    expect(updateWidget(placed, 'w1', {})).toBe(placed);
  });

  it('returns the same list when the identifier matches no widget', () => {
    const placed = [aWidget()];

    expect(updateWidget(placed, 'missing', { size: 'large' })).toBe(placed);
  });

  it('returns the same list when a removal matches nothing', () => {
    const placed = [aWidget()];

    expect(removeWidget(placed, 'missing')).toBe(placed);
  });

  it('returns the same list when the maximum is already reached', () => {
    let placed = addWidget([], 'cpu', 'a');
    while (placed.length < MAX_WIDGETS) placed = addWidget(placed, 'cpu', newWidgetId());

    expect(addWidget(placed, 'cpu')).toBe(placed);
  });

  it('keeps the fields a patch does not mention', () => {
    const placed = [aWidget({ style: 'glass', opacity: 0.5 })];

    const updated = updateWidget(placed, 'w1', { size: 'large' });

    expect(updated[0]).toMatchObject({ style: 'glass', opacity: 0.5, size: 'large' });
  });
});

describe('identifiers', () => {
  it('gives two widgets created in the same millisecond different identifiers', () => {
    expect(newWidgetId(1000)).not.toBe(newWidgetId(1000));
  });

  it('reads the widget identifier back out of its window label', () => {
    const label = widgetLabel('abc123');

    expect(label).toBe('widget-abc123');
    expect(widgetIdFromLabel(label)).toBe('abc123');
    expect(widgetIdFromLabel('main')).toBeNull();
  });
});

describe('window geometry and motion', () => {
  it('derives the window size from the size step rather than from stored pixels', () => {
    const small = toWindowSpec(aWidget({ size: 'small' }));
    const large = toWindowSpec(aWidget({ size: 'large' }));

    expect(small.width).toBe(geometryFor('small').width);
    expect(large.width).toBe(geometryFor('large').width);
    expect(large.width).toBeGreaterThan(small.width);
  });

  it('reports reduced motion so callers skip the animation entirely', () => {
    expect(shouldAnimate(true)).toBe(false);
    expect(shouldAnimate(false)).toBe(true);
  });

  it('leaves a non-finite opacity at fully opaque', () => {
    expect(clampOpacity(Number.NaN)).toBe(1);
  });
});
