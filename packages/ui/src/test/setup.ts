/**
 * Test environment shims.
 *
 * happy-dom implements no layout engine, so the browser APIs that Radix's
 * floating and scrolling primitives call during mount simply do not exist.
 * Without these the components throw before a single assertion runs, and the
 * failure reads as "Tooltip is broken" rather than "the DOM stub is
 * incomplete" — so stub them here, once, instead of per-test.
 */

if (!('ResizeObserver' in globalThis)) {
  globalThis.ResizeObserver = class {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  };
}

if (!('DOMRect' in globalThis)) {
  globalThis.DOMRect = class {
    readonly bottom = 0;
    readonly right = 0;
    constructor(
      readonly x = 0,
      readonly y = 0,
      readonly width = 0,
      readonly height = 0,
    ) {}
    get top(): number {
      return this.y;
    }
    get left(): number {
      return this.x;
    }
    toJSON(): unknown {
      return this;
    }
    static fromRect(): DOMRect {
      return new globalThis.DOMRect();
    }
  };
}

// Radix menus capture the pointer to keep press-drag-release working. happy-dom
// ships neither method, and their absence aborts the very first pointerdown.
const proto = globalThis.Element?.prototype as
  | (Element & {
      hasPointerCapture?: unknown;
      setPointerCapture?: unknown;
      releasePointerCapture?: unknown;
      scrollIntoView?: unknown;
    })
  | undefined;

if (proto) {
  proto.hasPointerCapture ??= () => false;
  proto.setPointerCapture ??= () => {};
  proto.releasePointerCapture ??= () => {};
  proto.scrollIntoView ??= () => {};
}

if (!('matchMedia' in globalThis)) {
  Object.defineProperty(globalThis, 'matchMedia', {
    writable: true,
    value: (query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }),
  });
}
