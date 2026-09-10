/**
 * Test environment setup.
 *
 * happy-dom does not implement everything the app touches. Rather than
 * guarding each call site with a runtime check that only exists for tests,
 * the gaps are filled here.
 */

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
