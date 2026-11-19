import { describe, expect, it, vi, type Mock } from 'vitest';

import { navItems } from './navigation';
import {
  displayKeys,
  isTypingTarget,
  runShortcut,
  shortcuts,
  type ShortcutActions,
} from './shortcuts';

type MockedActions = { readonly [K in keyof ShortcutActions]: Mock<ShortcutActions[K]> };

function actions(): MockedActions {
  return {
    openPalette: vi.fn<ShortcutActions['openPalette']>(),
    openHelp: vi.fn<ShortcutActions['openHelp']>(),
    openSettings: vi.fn<ShortcutActions['openSettings']>(),
    navigate: vi.fn<ShortcutActions['navigate']>(),
  };
}

const none = { ctrlKey: false, shiftKey: false, altKey: false, metaKey: false };

describe('runShortcut', () => {
  it('opens the palette on Ctrl+K and on Cmd+K', () => {
    const a = actions();
    expect(runShortcut({ ...none, key: 'k', ctrlKey: true }, a)).toBe(true);
    expect(runShortcut({ ...none, key: 'K', metaKey: true }, a)).toBe(true);
    expect(a.openPalette).toHaveBeenCalledTimes(2);
  });

  it('opens the help sheet on ? without a modifier, and not on Ctrl+?', () => {
    const a = actions();
    expect(runShortcut({ ...none, key: '?', shiftKey: true }, a)).toBe(true);
    expect(runShortcut({ ...none, key: '?', ctrlKey: true, shiftKey: true }, a)).toBe(false);
    expect(a.openHelp).toHaveBeenCalledTimes(1);
  });

  it('opens settings on Ctrl+,', () => {
    const a = actions();
    expect(runShortcut({ ...none, key: ',', ctrlKey: true }, a)).toBe(true);
    expect(a.openSettings).toHaveBeenCalledTimes(1);
  });

  it('Ctrl+1..9 navigate in sidebar order, and Ctrl+0 does nothing', () => {
    const a = actions();
    for (let digit = 1; digit <= 9; digit += 1) {
      expect(runShortcut({ ...none, key: String(digit), ctrlKey: true }, a)).toBe(true);
      expect(a.navigate).toHaveBeenLastCalledWith(navItems[digit - 1]?.id);
    }
    expect(runShortcut({ ...none, key: '0', ctrlKey: true }, a)).toBe(false);
    expect(a.navigate).toHaveBeenCalledTimes(9);
  });

  it('a plain digit or letter is not a shortcut', () => {
    const a = actions();
    expect(runShortcut({ ...none, key: '1' }, a)).toBe(false);
    expect(runShortcut({ ...none, key: 'k' }, a)).toBe(false);
    expect(a.navigate).not.toHaveBeenCalled();
    expect(a.openPalette).not.toHaveBeenCalled();
  });

  it('Ctrl+Shift+H is claimed but not acted on, because useHud owns the toggle', () => {
    // If this ever started calling an action, the overlay would flip twice per
    // press and appear to do nothing.
    const a = actions();
    expect(runShortcut({ ...none, key: 'h', ctrlKey: true, shiftKey: true }, a)).toBe(true);
    for (const fn of Object.values(a)) expect(fn).not.toHaveBeenCalled();
  });

  it('every registry entry has a label key and at least one key cap', () => {
    for (const shortcut of shortcuts) {
      expect(shortcut.labelKey).toMatch(/^shortcuts\./);
      expect(shortcut.keys.length).toBeGreaterThan(0);
    }
  });
});

describe('isTypingTarget', () => {
  it('is true for text inputs, textareas and contenteditable', () => {
    const input = document.createElement('input');
    const area = document.createElement('textarea');
    const editable = document.createElement('div');
    editable.contentEditable = 'true';
    document.body.append(input, area, editable);

    expect(isTypingTarget(input)).toBe(true);
    expect(isTypingTarget(area)).toBe(true);
    expect(isTypingTarget(editable)).toBe(true);
  });

  it('is false for buttons, checkboxes and the body', () => {
    const button = document.createElement('button');
    const check = document.createElement('input');
    check.type = 'checkbox';

    expect(isTypingTarget(button)).toBe(false);
    expect(isTypingTarget(check)).toBe(false);
    expect(isTypingTarget(document.body)).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});

describe('displayKeys', () => {
  it('draws the primary modifier as Cmd on macOS and Ctrl everywhere else', () => {
    const simple = ['Ctrl', 'K'];
    expect(displayKeys(simple, 'macos')).toEqual(['\u2318', 'K']);
    expect(displayKeys(simple, 'windows')).toEqual(['Ctrl', 'K']);
    // No report yet is the Windows spelling, matching the rest of the shell.
    expect(displayKeys(simple, null)).toEqual(['Ctrl', 'K']);
  });

  it('leaves every other cap alone, including the Cmd-already glyphs', () => {
    expect(displayKeys(['Ctrl', 'Shift', 'H'], 'macos')).toEqual(['\u2318', 'Shift', 'H']);
    expect(displayKeys(['?'], 'macos')).toEqual(['?']);
  });
});
