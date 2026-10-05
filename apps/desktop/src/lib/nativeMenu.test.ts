import { afterEach, describe, expect, it } from 'vitest';

import { suppressNativeContextMenu } from './nativeMenu';

function rightClick(target: Element): boolean {
  const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2 });
  target.dispatchEvent(event);
  return event.defaultPrevented;
}

let off: (() => void) | undefined;
afterEach(() => {
  off?.();
  document.body.replaceChildren();
});

describe('suppressNativeContextMenu', () => {
  it('stops the browser menu on ordinary surfaces', () => {
    off = suppressNativeContextMenu();
    const div = document.createElement('div');
    document.body.append(div);
    expect(rightClick(div)).toBe(true);
  });

  it('leaves the native Cut, Copy and Paste menu on text fields', () => {
    off = suppressNativeContextMenu();
    const input = document.createElement('input');
    document.body.append(input);
    expect(rightClick(input)).toBe(false);
  });

  it('stops nothing once removed', () => {
    suppressNativeContextMenu()();
    const div = document.createElement('div');
    document.body.append(div);
    expect(rightClick(div)).toBe(false);
  });
});
