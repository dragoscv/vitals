/**
 * D-pad spatial navigation: given where focus is and where every focusable
 * element sits, which one does an arrow key move to?
 *
 * Written here rather than taken from a library because the rule is short and
 * the libraries that exist either want every element registered by hand
 * (Norigin) or patch the DOM globally (the WICG polyfill), and both carry far
 * more code than this. Pure over rectangles, so it is tested without a DOM.
 *
 * The rule, the one Android's focus search uses: only elements that lie
 * beyond the current one in the pressed direction are candidates. Those "in
 * the beam" — overlapping it on the other axis — win over those that are
 * not; among equals the nearest wins, with sideways distance weighted more
 * heavily than forward distance. A plain nearest-wins rule
 * sends Right from a card to one diagonally below it that happens to be a
 * few pixels closer, which is the single most disorienting thing a TV UI
 * can do (the first version of this file did exactly that in its own test).
 *
 * Up and Down soften the beam as Android's `beamBeats` does: an element in
 * the beam wins only if it starts before an off-beam one ends. Otherwise Down
 * from Settings' right-hand Remove button skipped the whole language row
 * (left-aligned, so off the beam) for the full-width About panel below it,
 * and Română could not be reached (found on the Odyssey, 2026-09-30).
 *
 * Up and Down also stay inside the region they start in (the navigation
 * rail or the content). Up from a PC's tab row, with nothing above it in the
 * content, otherwise jumped diagonally into the rail (found on the TV,
 * 2026-09-30). The rail is reached with Left, as in every TV app. Only the
 * region is fenced, not the beam: a strict beam would strand focus on a
 * right-hand button whose next row starts at the left.
 */

export type Direction = 'up' | 'down' | 'left' | 'right';

export interface Box {
  readonly left: number;
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
}

/** Sideways misalignment counts this many times more than distance ahead. */
const SIDEWAYS_WEIGHT = 3;
/** Sub-pixel layout and borders make touching boxes overlap by a hair. */
const EPSILON = 1;

function gapAndOffset(from: Box, to: Box, dir: Direction): { gap: number; offset: number } | null {
  let gap: number;
  let fromStart: number;
  let fromEnd: number;
  let toStart: number;
  let toEnd: number;
  switch (dir) {
    case 'right':
      gap = to.left - from.right;
      [fromStart, fromEnd, toStart, toEnd] = [from.top, from.bottom, to.top, to.bottom];
      break;
    case 'left':
      gap = from.left - to.right;
      [fromStart, fromEnd, toStart, toEnd] = [from.top, from.bottom, to.top, to.bottom];
      break;
    case 'down':
      gap = to.top - from.bottom;
      [fromStart, fromEnd, toStart, toEnd] = [from.left, from.right, to.left, to.right];
      break;
    case 'up':
      gap = from.top - to.bottom;
      [fromStart, fromEnd, toStart, toEnd] = [from.left, from.right, to.left, to.right];
      break;
  }
  if (gap < -EPSILON) return null;
  // Zero when the two overlap on the cross axis: a row directly below is
  // perfectly aligned however much wider or narrower it is.
  const offset = Math.max(0, toStart - fromEnd, fromStart - toEnd);
  return { gap: Math.max(0, gap), offset };
}

/**
 * Index into `candidates` of the element to focus, or -1 when nothing lies
 * that way. `zones[i]` names the region of `candidates[i]`, and `fromZone`
 * the current one; omit both when there is one region.
 */
export function pickNext(
  from: Box,
  candidates: readonly Box[],
  dir: Direction,
  zones?: readonly string[],
  fromZone?: string,
): number {
  const vertical = dir === 'up' || dir === 'down';
  const scored: { index: number; gap: number; far: number; score: number; inBeam: boolean }[] = [];
  candidates.forEach((box, index) => {
    if (box === from) return;
    if (vertical && zones !== undefined && zones[index] !== fromZone) return;
    const measured = gapAndOffset(from, box, dir);
    if (measured === null) return;
    scored.push({
      index,
      gap: measured.gap,
      far: farEdge(from, box, dir),
      score: measured.gap + SIDEWAYS_WEIGHT * measured.offset,
      inBeam: measured.offset === 0,
    });
  });
  let best: (typeof scored)[number] | undefined;
  for (const c of scored) {
    if (best === undefined || beats(c, best, vertical)) best = c;
  }
  return best?.index ?? -1;
}

/** Distance from `from` to the far side of `to`, along the direction of travel. */
function farEdge(from: Box, to: Box, dir: Direction): number {
  switch (dir) {
    case 'right':
      return to.right - from.right;
    case 'left':
      return from.left - to.left;
    case 'down':
      return to.bottom - from.bottom;
    case 'up':
      return from.top - to.top;
  }
}

interface Scored {
  readonly gap: number;
  readonly far: number;
  readonly score: number;
  readonly inBeam: boolean;
}

function beats(a: Scored, b: Scored, vertical: boolean): boolean {
  if (a.inBeam !== b.inBeam) {
    const [inside, outside] = a.inBeam ? [a, b] : [b, a];
    // Sideways the beam always wins; vertically only when it is not wholly further away.
    const beamWins = !vertical || inside.gap < outside.far;
    return a.inBeam ? beamWins : !beamWins;
  }
  return a.score < b.score;
}

export function directionOf(keyCode: number): Direction | null {
  switch (keyCode) {
    case 37:
      return 'left';
    case 38:
      return 'up';
    case 39:
      return 'right';
    case 40:
      return 'down';
    default:
      return null;
  }
}
