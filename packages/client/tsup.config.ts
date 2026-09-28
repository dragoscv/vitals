import { defineConfig } from 'tsup';

/**
 * The published build.
 *
 * JavaScript only. `@vitals/protocol` is imported for types alone, so the
 * bundle has no runtime dependency at all. Declarations are emitted by `tsc`
 * (tsconfig.build.json) and made self-contained by
 * scripts/inline-protocol.mjs: tsup's own declaration bundler needs the
 * TypeScript JS API, which the TypeScript 7 this repo type-checks with does
 * not have (ADR 0024).
 */
export default defineConfig({
  entry: ['src/index.ts'],
  format: ['esm'],
  target: 'es2022',
  dts: false,
  clean: true,
  sourcemap: true,
  treeshake: true,
});
