import { act } from '@testing-library/react';

/**
 * Makes the four box metrics read a fixture's `data-*` attributes.
 *
 * happy-dom ships no layout engine: every box reports zero, so anything that
 * decides what to draw from a measurement correctly draws nothing. Stubbing the
 * metrics from attributes lets a test state the geometry it is reasoning about —
 * the same trade the Dialog test makes when it asserts the classes that produce
 * a layout guarantee rather than the pixels.
 *
 * An element without the attributes still measures zero, so a test can also
 * check the "nothing measurable" path by leaving them off.
 */
export type BoxName = 'offsetLeft' | 'offsetTop' | 'offsetWidth' | 'offsetHeight';

const BOXES: readonly (readonly [BoxName, string])[] = [
  ['offsetLeft', 'left'],
  ['offsetTop', 'top'],
  ['offsetWidth', 'w'],
  ['offsetHeight', 'h'],
];

/** Returns the function that puts the real metrics back. */
export function stubBoxes(): () => void {
  const saved = new Map<BoxName, PropertyDescriptor | undefined>();
  for (const [name, key] of BOXES) {
    saved.set(name, Object.getOwnPropertyDescriptor(HTMLElement.prototype, name));
    Object.defineProperty(HTMLElement.prototype, name, {
      configurable: true,
      get(this: HTMLElement): number {
        return Number(this.dataset[key] ?? 0);
      },
    });
  }
  return () => {
    for (const [name, descriptor] of saved) {
      if (descriptor === undefined)
        delete (HTMLElement.prototype as Record<BoxName, unknown>)[name];
      else Object.defineProperty(HTMLElement.prototype, name, descriptor);
    }
  };
}

/** Flushes a React commit and the microtask that follows an observer. */
export async function settle(): Promise<void> {
  await act(async () => {});
  await act(async () => {});
}
