// @ts-check
import js from '@eslint/js';
import { defineConfig } from 'eslint/config';
import tseslint from 'typescript-eslint';
import reactHooks from 'eslint-plugin-react-hooks';
import prettier from 'eslint-config-prettier';
import globals from 'globals';

export default defineConfig(
  {
    ignores: [
      '**/dist/**',
      '**/target/**',
      '**/node_modules/**',
      '**/.turbo/**',
      '**/.vitest/**',
      // Generated from Rust — lint failures here belong to the generator.
      'packages/protocol/src/generated/**',
    ],
  },

  // Type-aware linting for every TypeScript source. `projectService` uses each
  // package's own tsconfig.json, so vite/vitest configs must be in a package's
  // `include` (they are). The rules this unlocks — no-floating-promises,
  // no-misused-promises, await-thenable — are the ones that actually catch bugs
  // in a codebase built on `invoke()`.
  {
    files: ['**/*.{ts,tsx,mts,cts}'],
    extends: [js.configs.recommended, tseslint.configs.recommendedTypeChecked],
    languageOptions: {
      globals: { ...globals.browser, ...globals.es2023 },
      parserOptions: {
        projectService: {
          // Config files that sit outside every tsconfig `include`.
          allowDefaultProject: ['packages/*/vitest.config.ts'],
          defaultProject: 'apps/desktop/tsconfig.json',
        },
        tsconfigRootDir: import.meta.dirname,
        // typescript-eslint requires the TS 6 API (aliased as `typescript`).
        // Fail loudly if the alias ever resolves to something unsupported
        // rather than silently degrading to untyped linting.
        onUnsupportedTypeScriptVersion: 'error',
        ecmaFeatures: { jsx: true },
      },
    },
    rules: {
      // Unused args are legitimate when satisfying a callback signature;
      // requiring an underscore prefix documents the intent.
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' },
      ],
      // `any` defeats the whole point of generating types from Rust.
      '@typescript-eslint/no-explicit-any': 'error',
      '@typescript-eslint/consistent-type-imports': [
        'error',
        { prefer: 'type-imports', fixStyle: 'inline-type-imports' },
      ],
      // Every `invoke()` returns a promise; dropping one hides a failure.
      '@typescript-eslint/no-floating-promises': ['error', { ignoreVoid: true }],
      '@typescript-eslint/no-misused-promises': [
        'error',
        { checksVoidReturn: { attributes: false } },
      ],
      // Template literals with numbers are the norm in a metrics UI.
      '@typescript-eslint/restrict-template-expressions': [
        'error',
        { allowNumber: true, allowBoolean: true },
      ],
      // A dense realtime UI must not ship stray console noise.
      'no-console': ['warn', { allow: ['warn', 'error'] }],
      eqeqeq: ['error', 'smart'],
      'prefer-const': 'error',
      'no-var': 'error',
    },
  },

  {
    files: ['**/*.tsx'],
    plugins: { 'react-hooks': reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
    },
  },

  {
    files: ['**/*.{test,spec}.{ts,tsx}', '**/test/**'],
    rules: {
      'no-console': 'off',
      // Async stand-ins for IPC must match the real signature, which is
      // `Promise<T>`; a mock with nothing to await is the normal case.
      '@typescript-eslint/require-await': 'off',
      // Tests deliberately throw non-Errors to prove the boundary copes.
      '@typescript-eslint/only-throw-error': 'off',
      // Test bodies mock freely; the typed-unsafe family is noise there.
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-argument': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-return': 'off',
      '@typescript-eslint/unbound-method': 'off',
    },
  },

  // Plain JS config files get no type information.
  {
    files: ['**/*.{js,mjs,cjs}'],
    extends: [tseslint.configs.disableTypeChecked],
  },

  prettier,
);
