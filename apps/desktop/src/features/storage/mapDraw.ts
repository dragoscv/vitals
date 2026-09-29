/**
 * Draws a storage map onto a 2D canvas.
 *
 * Canvas, not SVG or DOM: a map is up to 5,000 rectangles (the backend's
 * cap), redrawn on every hover and every frame of a zoom. As elements that
 * is 5,000 nodes of layout and style recalculation per frame; as canvas it is
 * one fill per cell, well under a millisecond.
 *
 * Pure functions over the cell list so the geometry and the hit test can be
 * tested without a canvas at all.
 */

import type { MapCell, MapShape } from './model';

/** A cell's rectangle in CSS pixels. */
export interface PixelRect {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

/** The part of the unit square currently shown, for the zoom animation. */
export interface Viewport {
  readonly x0: number;
  readonly y0: number;
  readonly x1: number;
  readonly y1: number;
}

export const FULL_VIEW: Viewport = { x0: 0, y0: 0, x1: 1, y1: 1 };

export function toPixels(cell: MapCell, view: Viewport, width: number, height: number): PixelRect {
  const sx = width / (view.x1 - view.x0);
  const sy = height / (view.y1 - view.y0);
  return {
    x: (cell.x0 - view.x0) * sx,
    y: (cell.y0 - view.y0) * sy,
    w: (cell.x1 - cell.x0) * sx,
    h: (cell.y1 - cell.y0) * sy,
  };
}

/**
 * The deepest cell under a point, or `null`.
 *
 * Deepest, because in both layouts a parent's rectangle contains its
 * children's (the treemap by nesting, the icicle by column): the one the
 * pointer means is the most specific.
 */
export function hitTest(
  cells: readonly MapCell[],
  view: Viewport,
  width: number,
  height: number,
  px: number,
  py: number,
): MapCell | null {
  let best: MapCell | null = null;
  for (const cell of cells) {
    const r = toPixels(cell, view, width, height);
    if (px >= r.x && px < r.x + r.w && py >= r.y && py < r.y + r.h) {
      if (best === null || cell.depth >= best.depth) best = cell;
    }
  }
  return best;
}

/**
 * The folder a click on `hit` should open: `hit` itself when it can be
 * opened, otherwise the deepest openable folder that spans it.
 *
 * "Spans" differs by shape. In a treemap a parent's rectangle contains its
 * child's; in an icicle the parent sits in the row above, so only the column
 * (x range) is shared. Using pointer containment for both made a click on a
 * leaf in an icicle do nothing (found live: every movie in `D:\Movies` is a
 * folder of files, so the second row was all leaves).
 */
export function openTarget(
  cells: readonly MapCell[],
  shape: MapShape,
  hit: MapCell,
): MapCell | null {
  const openable = (c: MapCell) => c.kind === 'directory' && c.openable && c.depth > 0;
  if (openable(hit)) return hit;
  const eps = 1e-6;
  let best: MapCell | null = null;
  for (const cell of cells) {
    if (!openable(cell) || cell.depth >= hit.depth) continue;
    const spansX = cell.x0 <= hit.x0 + eps && cell.x1 >= hit.x1 - eps;
    const spansY = shape === 'icicle' || (cell.y0 <= hit.y0 + eps && cell.y1 >= hit.y1 - eps);
    if (spansX && spansY && (best === null || cell.depth > best.depth)) best = cell;
  }
  return best;
}

/** Linear interpolation between two viewports, `t` in 0..1. */
export function lerpView(from: Viewport, to: Viewport, t: number): Viewport {
  const k = Math.min(1, Math.max(0, t));
  return {
    x0: from.x0 + (to.x0 - from.x0) * k,
    y0: from.y0 + (to.y0 - from.y0) * k,
    x1: from.x1 + (to.x1 - from.x1) * k,
    y1: from.y1 + (to.y1 - from.y1) * k,
  };
}

/** Colours read from CSS variables once per draw, so themes just work. */
export interface MapPalette {
  /** One hue per top-level branch, cycled. */
  readonly branches: readonly string[];
  readonly files: string;
  readonly smaller: string;
  readonly warn: string;
  readonly edge: string;
  readonly text: string;
  readonly hover: string;
}

/**
 * Which top-level branch each cell belongs to, so a folder and everything
 * under it share a hue and the eye can follow a branch down the layers.
 */
export function branchOf(cells: readonly MapCell[], shape: MapShape): Map<MapCell, number> {
  const out = new Map<MapCell, number>();
  // A depth-1 cell starts a branch; a deeper cell belongs to the depth-1 cell
  // containing its centre. In an icicle a branch is a column (only x
  // matters, rows are depths); in a treemap it is a nested rectangle.
  const tops = cells.filter((c) => c.depth === 1);
  tops.forEach((top, i) => out.set(top, i));
  for (const cell of cells) {
    if (cell.depth <= 1) continue;
    const cx = (cell.x0 + cell.x1) / 2;
    const cy = (cell.y0 + cell.y1) / 2;
    const index = tops.findIndex(
      (top) =>
        cx >= top.x0 && cx <= top.x1 && (shape === 'icicle' || (cy >= top.y0 && cy <= top.y1)),
    );
    out.set(cell, index < 0 ? 0 : index);
  }
  return out;
}

export interface DrawOptions {
  readonly cells: readonly MapCell[];
  readonly shape: MapShape;
  readonly view: Viewport;
  readonly width: number;
  readonly height: number;
  readonly palette: MapPalette;
  readonly hovered: MapCell | null;
  readonly label: (cell: MapCell) => string | null;
}

/** Draws every cell; returns how many were visible, for the tests. */
export function drawMap(ctx: CanvasRenderingContext2D, options: DrawOptions): number {
  const { cells, shape, view, width, height, palette, hovered, label } = options;
  ctx.clearRect(0, 0, width, height);
  const branches = branchOf(cells, shape);
  let drawn = 0;
  ctx.font = '11px system-ui, sans-serif';
  ctx.textBaseline = 'middle';

  for (const cell of cells) {
    if (cell.depth === 0) continue;
    const r = toPixels(cell, view, width, height);
    if (r.x + r.w < 0 || r.y + r.h < 0 || r.x > width || r.y > height) continue;
    if (r.w < 0.5 || r.h < 0.5) continue;
    drawn++;

    const hue =
      cell.kind === 'files'
        ? palette.files
        : cell.kind === 'smaller'
          ? palette.smaller
          : (palette.branches[(branches.get(cell) ?? 0) % palette.branches.length] ??
            palette.files);
    ctx.globalAlpha = Math.max(0.35, 1 - (cell.depth - 1) * 0.12);
    ctx.fillStyle = cell.incomplete === null ? hue : palette.warn;
    ctx.fillRect(r.x, r.y, r.w, r.h);
    ctx.globalAlpha = 1;

    if (r.w > 2 && r.h > 2) {
      ctx.strokeStyle = palette.edge;
      ctx.lineWidth = 1;
      ctx.strokeRect(r.x + 0.5, r.y + 0.5, r.w - 1, r.h - 1);
    }
    if (cell === hovered) {
      ctx.strokeStyle = palette.hover;
      ctx.lineWidth = 2;
      ctx.strokeRect(r.x + 1, r.y + 1, r.w - 2, r.h - 2);
    }

    const text = r.w > 48 && r.h > 14 ? label(cell) : null;
    if (text !== null) {
      ctx.save();
      ctx.beginPath();
      ctx.rect(r.x + 4, r.y, r.w - 8, r.h);
      ctx.clip();
      ctx.fillStyle = palette.text;
      ctx.fillText(text, r.x + 5, r.y + Math.min(r.h / 2, 10));
      ctx.restore();
    }
  }
  return drawn;
}
