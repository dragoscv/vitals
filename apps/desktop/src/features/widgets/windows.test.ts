import { describe, expect, it } from 'vitest';

import { defaultPosition, MAX_WIDGETS } from './model';
import {
  bringOnScreen,
  cascadeSlots,
  createScreenSource,
  GRID_STEP,
  nextCascadePosition,
  readScreens,
  snapToGrid,
  type ScreenRect,
  type WidgetRect,
} from './windows';

const LAPTOP: ScreenRect = { x: 0, y: 0, width: 1920, height: 1080 };
const RIGHT: ScreenRect = { x: 1920, y: 0, width: 1280, height: 1024 };

function aWidget(overrides: Partial<WidgetRect> = {}): WidgetRect {
  return { x: 0, y: 0, width: 280, height: 170, ...overrides };
}

describe('snapping a dragged widget to the grid', () => {
  it('rounds each coordinate to the nearest step', () => {
    expect(snapToGrid({ x: 103, y: 207 })).toEqual({ x: 104, y: 208 });
  });

  it('is idempotent, and answers with the same object the second time', () => {
    const once = snapToGrid({ x: 103, y: 207 });
    const twice = snapToGrid(once);

    expect(twice).toBe(once);
  });

  it('returns a point already on the grid unchanged, so a drag on the grid writes nothing', () => {
    const onGrid = { x: 104, y: 208 };

    expect(snapToGrid(onGrid)).toBe(onGrid);
  });

  it('leaves an unmeasurable coordinate alone rather than inventing the origin', () => {
    const point = { x: Number.NaN, y: 40 };

    expect(snapToGrid(point)).toBe(point);
  });

  it('ignores a step that cannot divide, so a bad caller cannot move anything', () => {
    const point = { x: 103, y: 207 };

    expect(snapToGrid(point, 0)).toBe(point);
    expect(snapToGrid(point, Number.NaN)).toBe(point);
  });

  it('every cascade slot is already on the grid, so a fresh widget never moves on first snap', () => {
    const slots = cascadeSlots();

    expect(slots.length).toBeGreaterThan(0);
    for (const slot of slots) {
      expect(snapToGrid(slot)).toBe(slot);
    }
  });

  it('the grid divides the cascade origin and step, which is why a fresh widget does not move', () => {
    // The claim the constant rests on: 80 = origin, 28 = step, 4 = gcd.
    expect(defaultPosition(0).x % GRID_STEP).toBe(0);
    expect((defaultPosition(1).x - defaultPosition(0).x) % GRID_STEP).toBe(0);
  });
});

describe('choosing the cascade slot for a new widget', () => {
  it('takes the first slot when nothing is placed', () => {
    expect(nextCascadePosition([])).toEqual({ x: 80, y: 80 });
  });

  it('skips a slot that is already occupied', () => {
    const occupied = [defaultPosition(0), defaultPosition(1)];

    expect(nextCascadePosition(occupied)).toEqual(defaultPosition(2));
  });

  it('survives a removal, which the index-based default does not', () => {
    // Three widgets, the middle one removed, so the list length is two and the
    // index-based default would land the newcomer under the third.
    const occupied = [defaultPosition(0), defaultPosition(2)];
    const position = nextCascadePosition(occupied);

    expect(position).toEqual(defaultPosition(1));
    expect(position).not.toEqual(defaultPosition(occupied.length));
  });

  it('falls back to the index-based default once every slot is taken', () => {
    const occupied = cascadeSlots();
    const position = nextCascadePosition(occupied);

    expect(position).toEqual(snapToGrid(defaultPosition(occupied.length)));
  });

  it('does not run past the wrap, so the slot list is bounded by the cap', () => {
    expect(cascadeSlots().length).toBeLessThanOrEqual(MAX_WIDGETS);
  });
});

describe('bringing a widget back onto a screen', () => {
  it('returns the same object when the widget is already on a screen', () => {
    const widget = aWidget({ x: 40, y: 40 });

    expect(bringOnScreen(widget, [LAPTOP])).toBe(widget);
  });

  it('clamps a widget stored far off every screen back inside one', () => {
    const widget = aWidget({ x: 9000, y: 9000 });

    // 1920 - 280 = 1640; 1080 - 170 = 910.
    expect(bringOnScreen(widget, [LAPTOP])).toEqual({ x: 1640, y: 910 });
  });

  it('clamps a widget hanging off the right edge so its whole rectangle fits', () => {
    const widget = aWidget({ x: 1900, y: 1000 });

    expect(bringOnScreen(widget, [LAPTOP])).toEqual({ x: 1640, y: 910 });
  });

  it('chooses the nearest screen when the corner is off all of them', () => {
    // x = 3300 is past the right screen (ends at 3200); the nearest corner is
    // its right edge, so the widget is clamped to 1920 + 1280 - 280 = 2920.
    const widget = aWidget({ x: 3300, y: 200 });

    expect(bringOnScreen(widget, [LAPTOP, RIGHT])).toEqual({ x: 2920, y: 200 });
  });

  it('pins a widget bigger than the screen to the work area rather than off the edge', () => {
    const screen: ScreenRect = { x: 0, y: 0, width: 100, height: 100 };
    const widget = aWidget({ x: 500, y: 500, width: 240, height: 160 });

    expect(bringOnScreen(widget, [screen])).toEqual({ x: 0, y: 0 });
  });

  it('leaves the widget untouched when no screen could be learned', () => {
    const widget = aWidget({ x: 9000, y: 9000 });

    expect(bringOnScreen(widget, [])).toBe(widget);
  });

  it('moves a widget back onto a screen whose origin is not zero', () => {
    const screen: ScreenRect = { x: -1920, y: 0, width: 1920, height: 1080 };
    const widget = aWidget({ x: -9000, y: -9000 });

    expect(bringOnScreen(widget, [screen])).toEqual({ x: -1920, y: 0 });
  });
});

describe('learning the screens this machine has', () => {
  it('resolves to a list and never rejects, host or no host', async () => {
    const screens = await readScreens();

    expect(Array.isArray(screens)).toBe(true);
    for (const screen of screens) {
      expect(screen.width).toBeGreaterThan(0);
      expect(screen.height).toBeGreaterThan(0);
    }
  });

  it('a screen source answers with what it was given, so a test needs no host', async () => {
    const source = createScreenSource(() => Promise.resolve([LAPTOP]));

    await expect(source.screens()).resolves.toEqual([LAPTOP]);
  });
});
