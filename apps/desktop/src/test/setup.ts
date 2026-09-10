/**
 * Test environment setup.
 *
 * happy-dom does not implement everything the app touches. Rather than
 * guarding each call site with a runtime check that only exists for tests,
 * the gaps are filled here.
 */

import { beforeEach } from 'vitest';

// Screens mirror their search and sort into `location.hash` (lib/useUrlState)
// and read it back on mount. happy-dom keeps one window per test FILE, so
// without this a query typed in one test is silently restored by the next
// test's mount — and a table that should show every row shows none.
beforeEach(() => {
  if (typeof history !== 'undefined' && location.hash !== '') {
    history.replaceState(null, '', location.pathname + location.search);
  }
});

// ResizeObserver is used by every chart. Absent in happy-dom.
if (!('ResizeObserver' in globalThis)) {
  globalThis.ResizeObserver = class {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  };
}

// matchMedia backs theme resolution and prefers-reduced-motion.
if (!globalThis.matchMedia) {
  globalThis.matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  });
}

// happy-dom's Web Animations shim rejects `Animation.finished` when the
// animation is cancelled, and Motion cancels one every time a route changes
// mid-fade or a dialog unmounts. A browser marks that promise as "handled"
// unless someone awaits it; happy-dom does not, so it surfaces as an
// unhandled rejection with no test at fault. Attach a no-op handler so the
// suite is judged on its assertions, not on the shim.
//
// Verified directly: `body.animate(...).cancel()` in bare happy-dom logs
// `UNHANDLED AbortError` on the process.
if (typeof Element !== 'undefined' && typeof Element.prototype.animate === 'function') {
  const animate = Element.prototype.animate;
  Element.prototype.animate = function patchedAnimate(...args) {
    const animation = animate.apply(this, args);
    animation.finished.catch(() => {});
    return animation;
  };
}
