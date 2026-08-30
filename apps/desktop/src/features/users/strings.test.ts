import { describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { bundles, registerUsersStrings, USERS_NS } from './strings';

function keyPaths(value: unknown, prefix = ''): string[] {
  if (typeof value !== 'object' || value === null) return [prefix];

  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    keyPaths(child, prefix === '' ? key : `${prefix}.${key}`),
  );
}

describe('users translations', () => {
  it('defines the same keys in English and Romanian', () => {
    const strip = (paths: readonly string[]) =>
      [...new Set(paths.map((path) => path.replace(/_(one|few|other)$/, '')))].sort();

    expect(strip(keyPaths(bundles.ro))).toEqual(strip(keyPaths(bundles.en)));
  });

  it('names every session state the backend can emit', () => {
    // The table indexes `state.<key>` with whatever Rust sent. These keys are
    // produced by an explicit mapping in `users.rs` precisely so this list is
    // a fixed contract rather than Rust variant names — but the two still have
    // to agree, and this is where that is checked.
    const states = ['active', 'connected', 'disconnected', 'idle', 'listen', 'shadow', 'unknown'];
    const paths = new Set(keyPaths(bundles.en));

    for (const state of states) {
      expect(paths, `missing state.${state}`).toContain(`state.${state}`);
    }
  });

  it('registers the namespace without errors', async () => {
    await initI18n();
    registerUsersStrings();

    await i18n.changeLanguage('en');
    expect(i18n.t('title', { ns: USERS_NS })).toBe('Users');

    await i18n.changeLanguage('ro');
    expect(i18n.t('title', { ns: USERS_NS })).toBe('Utilizatori');
  });
});
