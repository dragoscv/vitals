import { beforeEach, describe, expect, it } from 'vitest';

import { applyTheme, resolveMode, rowHeight } from './apply';
import { defaultTheme } from './types';

describe('resolveMode', () => {
  it('follows the OS when set to system', () => {
    expect(resolveMode('system', true)).toBe('dark');
    expect(resolveMode('system', false)).toBe('light');
  });

  it('ignores the OS when explicitly set', () => {
    expect(resolveMode('light', true)).toBe('light');
    expect(resolveMode('dark', false)).toBe('dark');
  });
});

describe('rowHeight', () => {
  it('scales with density', () => {
    expect(rowHeight('compact')).toBeLessThan(rowHeight('default'));
    expect(rowHeight('default')).toBeLessThan(rowHeight('comfortable'));
  });
});

describe('applyTheme', () => {
  let root: HTMLElement;

  beforeEach(() => {
    root = document.createElement('div');
  });

  it('writes every axis independently', () => {
    applyTheme({ ...defaultTheme, mode: 'dark', accent: 'green', surface: 'acrylic' }, root, false);

    expect(root.classList.contains('dark')).toBe(true);
    expect(root.dataset['accent']).toBe('green');
    expect(root.dataset['surface']).toBe('acrylic');
  });

  it('removes the dark class when switching back to light', () => {
    applyTheme({ ...defaultTheme, mode: 'dark' }, root, false);
    applyTheme({ ...defaultTheme, mode: 'light' }, root, true);

    expect(root.classList.contains('dark')).toBe(false);
  });

  it('exposes row height as a custom property for CSS to consume', () => {
    applyTheme({ ...defaultTheme, density: 'compact' }, root, false);
    expect(root.style.getPropertyValue('--row-height')).toBe('24px');
  });

  it('leaves reduced-motion unset when following the system', () => {
    // Absent means the prefers-reduced-motion media query stays in control.
    // Writing "false" here would override a user's OS-level accessibility
    // setting, which would be a genuine accessibility regression.
    applyTheme({ ...defaultTheme, reduceMotion: null }, root, false);
    expect(root.dataset['reduceMotion']).toBeUndefined();
  });

  it('records an explicit reduced-motion override', () => {
    applyTheme({ ...defaultTheme, reduceMotion: true }, root, false);
    expect(root.dataset['reduceMotion']).toBe('true');

    applyTheme({ ...defaultTheme, reduceMotion: false }, root, false);
    expect(root.dataset['reduceMotion']).toBe('false');
  });

  it('clears an override when reverting to system', () => {
    applyTheme({ ...defaultTheme, reduceMotion: true }, root, false);
    applyTheme({ ...defaultTheme, reduceMotion: null }, root, false);
    expect(root.dataset['reduceMotion']).toBeUndefined();
  });
});
