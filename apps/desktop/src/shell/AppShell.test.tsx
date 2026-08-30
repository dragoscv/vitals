import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n, i18n } from '@vitals/i18n';

import type { SettingsBackend } from '../settings/persistence';
import { registerDashboardStrings } from '../features/dashboard';
import { resetSettingsForTests, setSettingsBackend, useSettings } from '../settings/store';
import { ThemeProvider } from '../theme/ThemeProvider';
import { AppShell } from './AppShell';
import { registerShellStrings } from './strings';

/** Lets a test drive the forced-collapse breakpoint. */
function stubViewport(width: number): void {
  vi.stubGlobal('matchMedia', (query: string) => {
    const max = /max-width:\s*(\d+)px/.exec(query);
    return {
      matches: max ? width <= Number(max[1]) : false,
      media: query,
      onchange: null,
      addEventListener: () => {},
      removeEventListener: () => {},
      addListener: () => {},
      removeListener: () => {},
      dispatchEvent: () => false,
    };
  });
}

const nullBackend: SettingsBackend = {
  load: () => Promise.resolve(undefined),
  save: () => Promise.resolve(),
};

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
  // The shell renders whichever screen the route names, and the dashboard is
  // the default route — so its namespace is part of this component's
  // environment, exactly as it is in `main.tsx`.
  registerDashboardStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
  resetSettingsForTests();
  setSettingsBackend(nullBackend);
  stubViewport(1400);
});

function renderShell() {
  return render(
    <ThemeProvider>
      <AppShell version="1.2.3" />
    </ThemeProvider>,
  );
}

describe('AppShell', () => {
  it('renders the title bar, navigation and a main landmark', () => {
    renderShell();

    expect(screen.getByRole('navigation', { name: 'Main navigation' })).toBeTruthy();
    expect(screen.getByRole('main')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Close' })).toBeTruthy();
  });

  it('swaps the content when a destination is chosen', () => {
    renderShell();
    const main = screen.getByRole('main');

    // This assertion used to point at whichever route was still unbuilt, and
    // it had to be re-pointed every time one of them shipped — Storage, then
    // Benchmarks, then Users. Every route renders a real screen now, so there
    // is no placeholder left to aim at and the test asserts the thing it was
    // always actually about: choosing a destination replaces what `main`
    // shows, and the store agrees with the screen.
    expect(within(main).getByRole('heading', { name: 'Dashboard' })).toBeTruthy();

    fireEvent.click(screen.getByRole('button', { name: 'Users' }));

    expect(within(main).queryByRole('heading', { name: 'Dashboard' })).toBeNull();
    expect(useSettings.getState().route).toBe('users');
  });

  it('names the window after the section so the taskbar says where you are', () => {
    renderShell();
    fireEvent.click(screen.getByRole('button', { name: 'Benchmarks' }));
    expect(document.title).toContain('Benchmarks');
  });

  it('opens settings and returns focus to the trigger on close', async () => {
    renderShell();
    const nav = screen.getByRole('navigation', { name: 'Main navigation' });
    const trigger = within(nav).getByRole('button', { name: 'Settings' });

    trigger.focus();
    fireEvent.click(trigger);

    const dialog = await screen.findByRole('dialog');
    expect(within(dialog).getByRole('tab', { name: 'Appearance' })).toBeTruthy();

    fireEvent.keyDown(dialog, { key: 'Escape' });

    // Without focus return, dismissing the dialog drops a keyboard user at the
    // top of the document, and they must tab all the way back.
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
      expect(document.activeElement).toBe(trigger);
    });
  });

  it('forces the sidebar to icons only below the narrow breakpoint', () => {
    stubViewport(760);
    renderShell();

    // An expanded 224px sidebar at 720px leaves less than one card of content.
    expect(screen.getByRole('button', { name: 'Expand sidebar' })).toBeTruthy();
    // The stored preference is untouched, so widening restores what the user
    // actually chose rather than what the viewport imposed.
    expect(useSettings.getState().settings.sidebarCollapsed).toBe(false);
  });

  it('does not write a collapse preference the user cannot see', () => {
    stubViewport(760);
    renderShell();

    fireEvent.click(screen.getByRole('button', { name: 'Expand sidebar' }));

    expect(useSettings.getState().settings.sidebarCollapsed).toBe(false);
  });

  it('constrains content width so an ultrawide does not stretch it', () => {
    renderShell();
    const column = screen.getByRole('main').firstElementChild as HTMLElement;
    expect(column.className).toContain('max-w-[var(--content-max)]');
    expect(column.className).toContain('mx-auto');
  });

  it('translates the whole shell when the locale changes', async () => {
    renderShell();
    await i18n.changeLanguage('ro');

    await waitFor(() => {
      expect(screen.getByRole('navigation', { name: /navigare|Main navigation/i })).toBeTruthy();
      expect(screen.getByRole('button', { name: 'Procese' })).toBeTruthy();
    });
  });
});
