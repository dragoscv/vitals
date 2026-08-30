import { afterEach, describe, expect, it, vi } from 'vitest';

import { hasTauriHost } from './host';
import { getWindowControls } from './windowControls';

afterEach(() => {
  vi.unstubAllGlobals();
});

/**
 * These guard a bug that every other gate missed.
 *
 * The first implementation tested `'__TAURI_INTERNALS__' in globalThis`, which
 * is false inside the shipped app. Typecheck, lint and the whole unit suite
 * passed — because outside Tauri the wrong answer is also the expected one —
 * while in the real binary the caption buttons, settings persistence and the
 * version readout were all silently inert. Only running the built app found
 * it, so the positive case is pinned here.
 */
describe('hasTauriHost', () => {
  it('is false with no host', () => {
    expect(hasTauriHost()).toBe(false);
  });

  it('is true when the host marks the global, as Tauri actually does', () => {
    vi.stubGlobal('isTauri', true);
    expect(hasTauriHost()).toBe(true);
  });

  it('is not fooled by the internals object alone', () => {
    // The previous, wrong predicate. Asserting it is insufficient stops the
    // same shortcut being reintroduced.
    vi.stubGlobal('__TAURI_INTERNALS__', {});
    expect(hasTauriHost()).toBe(false);
  });
});

describe('getWindowControls', () => {
  it('returns inert controls with no host rather than throwing', async () => {
    const controls = getWindowControls();
    await expect(controls.isMaximized()).resolves.toBe(false);
    await expect(controls.minimize()).resolves.toBeUndefined();

    const off = await controls.onResized(() => {});
    expect(() => off()).not.toThrow();
  });
});
