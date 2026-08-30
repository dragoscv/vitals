/**
 * Proves the shell survives a screen that throws.
 *
 * In its own file because it mocks `../routes` at module level, which would
 * otherwise apply to every test in `AppShell.test.tsx` and make the ordinary
 * navigation assertions meaningless.
 *
 * The behaviour under test: there were no error boundaries anywhere in this
 * app. One throw in one widget unmounted the whole React tree and left a white
 * window — no navigation, no Settings, no text explaining anything. For a tool
 * people open *because* their computer is already misbehaving, that reads as
 * "Vitals broke my machine".
 */

import { render, screen } from '@testing-library/react';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import type { SettingsBackend } from '../settings/persistence';
import { resetSettingsForTests, setSettingsBackend } from '../settings/store';
import { ThemeProvider } from '../theme/ThemeProvider';
import { AppShell } from './AppShell';
import { registerShellStrings } from './strings';

vi.mock('../routes', () => ({
  RouteView: () => {
    throw new Error('sampler returned an impossible frame');
  },
  RouteSkeleton: () => null,
}));

const nullBackend: SettingsBackend = {
  load: () => Promise.resolve(undefined),
  save: () => Promise.resolve(),
};

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
  resetSettingsForTests();
  setSettingsBackend(nullBackend);

  vi.stubGlobal('matchMedia', (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  }));

  // React logs the caught error, and the boundary logs it again deliberately.
  vi.spyOn(console, 'error').mockImplementation(() => undefined);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('AppShell with a crashing screen', () => {
  it('explains the failure instead of blanking the window', () => {
    render(
      <ThemeProvider>
        <AppShell version="1.2.3" />
      </ThemeProvider>,
    );

    expect(screen.getByText('This section stopped working')).toBeTruthy();
    // The message is shown, not buried behind "something went wrong": this is
    // open source, shipped to people already debugging, whose only support
    // channel is a GitHub issue they need something to paste into.
    expect(screen.getByText('sampler returned an impossible frame')).toBeTruthy();
  });

  it('leaves the navigation and settings reachable', () => {
    render(
      <ThemeProvider>
        <AppShell version="1.2.3" />
      </ThemeProvider>,
    );

    expect(screen.getByRole('navigation', { name: 'Main navigation' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Settings' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Processes' })).toBeTruthy();
  });

  it('offers a retry rather than looping on its own', () => {
    // An automatic retry would be a hot loop pinning a core, inside the
    // application whose entire purpose is showing you what is pinning a core.
    render(
      <ThemeProvider>
        <AppShell version="1.2.3" />
      </ThemeProvider>,
    );

    expect(screen.getByRole('button', { name: 'Try again' })).toBeTruthy();
  });
});
