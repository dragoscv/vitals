import { describe, expect, it } from 'vitest';

import { pickNext, type Box } from './spatial';

function box(left: number, top: number, width: number, height: number): Box {
  return { left, top, right: left + width, bottom: top + height };
}

describe('pickNext', () => {
  it('moves down from a right-hand button to the next row even when a full-width panel below is in the beam', () => {
    // Settings on the Odyssey: Remove sits at the right of the pairing row,
    // the language choices are left-aligned below it, and the About panel
    // spans the whole width further down. Down must reach the choices.
    const remove = box(1500, 200, 160, 60);
    const english = box(300, 400, 200, 60);
    const about = box(100, 600, 1600, 300);
    expect(pickNext(remove, [remove, english, about], 'down')).toBe(1);
  });

  it('moves right along a row rather than to a nearer card diagonally below', () => {
    const current = box(0, 0, 100, 100);
    // The diagonal card is closer by straight-line distance than the aligned one.
    const diagonal = box(110, 105, 100, 100);
    const aligned = box(150, 0, 100, 100);
    expect(pickNext(current, [current, diagonal, aligned], 'right')).toBe(2);
  });

  it('moves down to a row that overlaps, however much wider it is', () => {
    const tab = box(300, 0, 80, 40);
    const wide = box(0, 60, 1600, 400);
    const narrowOff = box(0, 50, 80, 40);
    expect(pickNext(tab, [tab, narrowOff, wide], 'down')).toBe(2);
  });

  it('finds nothing when no element lies in the pressed direction, so focus stays put', () => {
    const current = box(0, 0, 100, 100);
    const others = [box(200, 0, 100, 100), box(0, 200, 100, 100)];
    expect(pickNext(current, others, 'left')).toBe(-1);
    expect(pickNext(current, others, 'up')).toBe(-1);
  });

  it('reaches the navigation drawer on the left from the leftmost content, at the item beside it', () => {
    const drawer = [box(0, 100, 200, 60), box(0, 180, 200, 60), box(0, 260, 200, 60)];
    const content = box(260, 190, 400, 300);
    expect(pickNext(content, drawer, 'left')).toBe(1);
  });

  it('never picks an element behind the current one, even when it is the closest', () => {
    const current = box(100, 100, 100, 100);
    const behind = box(0, 100, 90, 100);
    const ahead = box(900, 100, 100, 100);
    expect(pickNext(current, [behind, ahead], 'right')).toBe(1);
  });

  it('keeps Up and Down inside their region, so Up from the tabs never lands in the rail', () => {
    const railTop = box(0, 50, 200, 60);
    const tab = box(300, 200, 120, 50);
    expect(pickNext(tab, [railTop, tab], 'up', ['rail', 'content'], 'content')).toBe(-1);
    // Left still crosses into the rail.
    expect(pickNext(tab, [railTop, tab], 'left', ['rail', 'content'], 'content')).toBe(0);
  });

  it('still moves Down to a left-aligned row from a button at the far right', () => {
    const high = box(800, 100, 120, 40);
    const turnOn = box(300, 200, 160, 40);
    expect(pickNext(high, [high, turnOn], 'down', ['content', 'content'], 'content')).toBe(1);
  });
});
