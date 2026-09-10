import { describe, expect, it } from 'vitest';

import { isHudShortcut } from './useHud';

const none = { altKey: false, metaKey: false };

describe('isHudShortcut', () => {
  it('matches Ctrl+Shift+H whatever the letter case', () => {
    expect(isHudShortcut({ key: 'H', ctrlKey: true, shiftKey: true, ...none })).toBe(true);
    expect(isHudShortcut({ key: 'h', ctrlKey: true, shiftKey: true, ...none })).toBe(true);
  });

  it('refuses the chord with a modifier missing or added', () => {
    // Ctrl+H alone is a browser history chord and Shift+H is typing; either
    // one flipping the overlay would be a surprise the user cannot undo by
    // pressing the key again without knowing what happened.
    expect(isHudShortcut({ key: 'h', ctrlKey: true, shiftKey: false, ...none })).toBe(false);
    expect(isHudShortcut({ key: 'H', ctrlKey: false, shiftKey: true, ...none })).toBe(false);
    expect(
      isHudShortcut({ key: 'H', ctrlKey: true, shiftKey: true, altKey: true, metaKey: false }),
    ).toBe(false);
    expect(isHudShortcut({ key: 'G', ctrlKey: true, shiftKey: true, ...none })).toBe(false);
  });
});
