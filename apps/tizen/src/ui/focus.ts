/**
 * Wires the pure spatial rule (`lib/spatial.ts`) to the DOM and the remote.
 *
 * Every focusable thing in the app is a native `<button>` or `<input>`, so
 * Enter activates it without any extra code and a screen reader (Samsung's
 * Voice Guide) announces it; this module only decides where the arrows go.
 */

import { directionOf, pickNext, type Box } from '../lib/spatial';

export const KEY_ENTER = 13;
/** Tizen's Return key. Escape and Backspace stand in for it in a desktop browser. */
export const KEY_BACK = 10009;
const KEY_ESCAPE = 27;
const KEY_BACKSPACE = 8;

const FOCUSABLE = 'button:not([disabled]), input:not([disabled]), [data-focusable]';

function visible(el: HTMLElement): boolean {
  const r = el.getBoundingClientRect();
  return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== 'hidden';
}

export function focusables(root: ParentNode = document): HTMLElement[] {
  return [...root.querySelectorAll<HTMLElement>(FOCUSABLE)].filter(visible);
}

function boxOf(el: HTMLElement): Box {
  const r = el.getBoundingClientRect();
  return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
}

/** The region an element belongs to: its nearest `[data-zone]` ancestor. */
function zoneOf(el: HTMLElement): string {
  return el.closest<HTMLElement>('[data-zone]')?.dataset.zone ?? '';
}

export function focusElement(el: HTMLElement): void {
  el.focus({ preventScroll: true });
  // `nearest` keeps a row that is already on screen still; a TV list that
  // jumps on every press is impossible to follow from across the room.
  el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

/** Focuses the first `[data-autofocus]` in `root`, else its first focusable. */
export function focusFirst(root: ParentNode | null): boolean {
  if (root === null) return false;
  const preferred = root.querySelector<HTMLElement>('[data-autofocus]');
  const target = preferred !== null && visible(preferred) ? preferred : focusables(root)[0];
  if (target === undefined) return false;
  focusElement(target);
  return true;
}

export function isBackKey(event: KeyboardEvent): boolean {
  if (event.keyCode === KEY_BACK || event.keyCode === KEY_ESCAPE) return true;
  // Backspace means "back" only outside a text field, where it deletes.
  return event.keyCode === KEY_BACKSPACE && !(event.target instanceof HTMLInputElement);
}

/**
 * Moves focus for an arrow key. Returns `true` when the key was handled.
 *
 * Inside a text field, Left and Right move the caret as a person expects;
 * only Up and Down leave it.
 */
export function moveFocus(event: KeyboardEvent, root: ParentNode = document): boolean {
  const dir = directionOf(event.keyCode);
  if (dir === null) return false;
  const active = document.activeElement;
  if (active instanceof HTMLInputElement && (dir === 'left' || dir === 'right')) return false;
  const all = focusables(root);
  if (!(active instanceof HTMLElement) || !all.includes(active)) {
    // Focus was lost (the focused row's process exited): recover rather than
    // leave the remote doing nothing.
    const first = all[0];
    if (first !== undefined) focusElement(first);
    return true;
  }
  const boxes = all.map(boxOf);
  const from = boxes[all.indexOf(active)];
  if (from === undefined) return true;
  const index = pickNext(from, boxes, dir, all.map(zoneOf), zoneOf(active));
  const next = all[index];
  if (next !== undefined) focusElement(next);
  return true;
}

/**
 * Asks the TV to deliver the number keys to the app. Without this, Tizen
 * keeps them for channel entry and a pairing code cannot be typed on the
 * remote. Feature-detected so the app still runs in a desktop browser.
 */
export function registerRemoteKeys(): void {
  const input = typeof tizen !== 'undefined' ? tizen?.tvinputdevice : undefined;
  if (input === undefined) return;
  try {
    input.registerKeyBatch(['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'], undefined, (error) =>
      console.warn('could not register the number keys', error.message),
    );
  } catch (error) {
    console.warn('could not register the number keys', error);
  }
}

/** Leaves the app, as Back at the top level does in every TV app. */
export function exitApp(): void {
  try {
    if (typeof tizen !== 'undefined' && tizen?.application !== undefined) {
      tizen.application.getCurrentApplication().exit();
      return;
    }
  } catch (error) {
    console.warn('could not exit', error);
  }
}

/** The digit a key event carries, from the remote or a keyboard, or `null`. */
export function digitOf(event: KeyboardEvent): string | null {
  if (event.keyCode >= 48 && event.keyCode <= 57) return String(event.keyCode - 48);
  // Numeric keypad on a desktop keyboard.
  if (event.keyCode >= 96 && event.keyCode <= 105) return String(event.keyCode - 96);
  return null;
}
