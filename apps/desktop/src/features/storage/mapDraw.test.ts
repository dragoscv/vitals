import { describe, expect, it } from 'vitest';

import { FULL_VIEW, branchOf, hitTest, lerpView, openTarget, toPixels } from './mapDraw';
import type { MapCell } from './model';

function cell(overrides: Partial<MapCell> = {}): MapCell {
  return {
    kind: 'directory',
    node: 1,
    depth: 1,
    x0: 0,
    y0: 0,
    x1: 1,
    y1: 1,
    allocated: 100,
    count: 1,
    openable: true,
    name: 'a',
    incomplete: null,
    ...overrides,
  };
}

describe('storage map geometry', () => {
  it('maps a cell to pixels through the viewport', () => {
    const c = cell({ x0: 0.5, x1: 0.75, y0: 0.25, y1: 0.5 });
    expect(toPixels(c, FULL_VIEW, 400, 200)).toEqual({ x: 200, y: 50, w: 100, h: 50 });
    // Zoomed onto the right half: the same cell doubles in width.
    const zoomed = toPixels(c, { x0: 0.5, y0: 0, x1: 1, y1: 1 }, 400, 200);
    expect(zoomed.x).toBe(0);
    expect(zoomed.w).toBe(200);
  });

  it('hits the deepest cell under the pointer, not its parent', () => {
    // A parent's rectangle contains its child's in both layouts; the click
    // means the most specific one.
    const parent = cell({ node: 1, depth: 1 });
    const child = cell({ node: 2, depth: 2, x0: 0.2, x1: 0.4, y0: 0.2, y1: 0.4 });
    const cells = [parent, child];
    expect(hitTest(cells, FULL_VIEW, 100, 100, 30, 30)).toBe(child);
    expect(hitTest(cells, FULL_VIEW, 100, 100, 80, 80)).toBe(parent);
    expect(hitTest(cells, FULL_VIEW, 100, 100, 150, 30)).toBeNull();
  });

  it('a click on a leaf in an icicle opens the folder in the row above', () => {
    // The parent is NOT under the pointer in an icicle; it shares the column.
    const parent = cell({ node: 1, depth: 1, x0: 0, x1: 0.5, y0: 0.14, y1: 0.28 });
    const other = cell({ node: 9, depth: 1, x0: 0.5, x1: 1, y0: 0.14, y1: 0.28 });
    const leaf = cell({ node: 2, depth: 2, openable: false, x0: 0.1, x1: 0.2, y0: 0.28, y1: 0.42 });
    const cells = [parent, other, leaf];
    const hit = hitTest(cells, FULL_VIEW, 100, 100, 15, 35);
    expect(hit).toBe(leaf);
    expect(openTarget(cells, 'icicle', leaf)).toBe(parent);
  });

  it('a click on a leaf in a treemap opens the rectangle around it', () => {
    const parent = cell({ node: 1, depth: 1, x0: 0, x1: 0.5, y0: 0, y1: 1 });
    const beside = cell({ node: 9, depth: 1, x0: 0.5, x1: 1, y0: 0, y1: 1 });
    const leaf = cell({ node: 2, depth: 2, openable: false, x0: 0.1, x1: 0.2, y0: 0.1, y1: 0.2 });
    expect(openTarget([parent, beside, leaf], 'treemap', leaf)).toBe(parent);
    // The focus itself (depth 0) is never a target: it is already open.
    const focus = cell({ node: 0, depth: 0 });
    expect(openTarget([focus, leaf], 'treemap', leaf)).toBeNull();
  });

  it('interpolates the zoom and clamps past either end', () => {
    const to = { x0: 0.5, y0: 0.5, x1: 1, y1: 1 };
    expect(lerpView(FULL_VIEW, to, 0.5)).toEqual({ x0: 0.25, y0: 0.25, x1: 1, y1: 1 });
    expect(lerpView(FULL_VIEW, to, 2)).toEqual(to);
    expect(lerpView(FULL_VIEW, to, -1)).toEqual(FULL_VIEW);
  });

  it('colours an icicle by column and a treemap by containment', () => {
    const left = cell({ node: 1, depth: 1, x0: 0, x1: 0.5, y0: 0.2, y1: 0.4 });
    const right = cell({ node: 2, depth: 1, x0: 0.5, x1: 1, y0: 0.2, y1: 0.4 });
    // Below `right` in the next row: same branch in an icicle.
    const below = cell({ node: 3, depth: 2, x0: 0.6, x1: 0.7, y0: 0.4, y1: 0.6 });
    const icicle = branchOf([left, right, below], 'icicle');
    expect(icicle.get(below)).toBe(icicle.get(right));

    const inside = cell({ node: 4, depth: 2, x0: 0.1, x1: 0.2, y0: 0.25, y1: 0.35 });
    const treemap = branchOf([left, right, inside], 'treemap');
    expect(treemap.get(inside)).toBe(treemap.get(left));
  });
});
