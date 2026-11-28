/**
 * The widget window's own palette.
 *
 * Pinned dark on purpose, and the accent is the one thing borrowed from the
 * user's choice in the main window — so the ring and the sparklines match the
 * charts they are next to. Everything else has to survive being drawn on top
 * of a photograph, so it does not follow the desktop theme.
 */

import { describe, expect, it } from 'vitest';

import { accents } from '../../theme/types';

import { applyWidgetTheme } from './theme';

describe('the widget window theme', () => {
  it('is dark whatever the desktop is set to', () => {
    const root = document.createElement('div');

    applyWidgetTheme(root);

    expect(root.classList.contains('dark')).toBe(true);
    expect(root.dataset['accent']).toBe('green');
  });

  it('takes the accent the main window reported', () => {
    const root = document.createElement('div');
    const other = accents.find((accent) => accent !== 'green') ?? 'green';

    applyWidgetTheme(root, other);

    expect(root.dataset['accent']).toBe(other);
  });
});
