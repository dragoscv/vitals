import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n, i18n } from '@vitals/i18n';

import type { SettingsBackend } from '../settings/persistence';
import { registerDashboardStrings } from '../features/dashboard';
import { resetSettingsForTests, setSettingsBackend, useSettings } from '../settings/store';
import { MotionProvider } from '../theme/MotionProvider';
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
  // Warm the lazy settings chunk. `React.lazy` then resolves from the module
  // cache, so "opens settings" measures focus handling rather than how long
  // vite takes to transform the dialog and its panels cold. That transform
  // is what crossed 8 s and then 12 s in full runs at 87-91 % CPU
  // (2026-09-27) while the file alone passed in under a second.
  await import('../settings/SettingsDialog');
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
  resetSettingsForTests();
  setSettingsBackend(nullBackend);
  stubViewport(1400);
});

async function renderShell() {
  const result = render(
    <ThemeProvider>
      <MotionProvider>
        <AppShell version="1.2.3" />
      </MotionProvider>
    </ThemeProvider>,
  );
  // The Motion runtime is a lazy chunk; the shell paints immediately from the
  // Suspense fallback and is swapped for the real tree when it lands. State
  // set before that swap is lost with the fallback, so every interaction in
  // these tests must come after it — exactly as it does in the real app,
  // where the swap completes before the window is revealed.
  await act(async () => {
    await Promise.resolve();
  });
  return result;
}

describe('AppShell', () => {
  it('renders the title bar, navigation and a main landmark', async () => {
    await renderShell();

    expect(screen.getByRole('navigation', { name: 'Main navigation' })).toBeTruthy();
    expect(screen.getByRole('main')).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Close' })).toBeTruthy();
  });

  it('swaps the content when a destination is chosen', async () => {
    await renderShell();
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

  it('names the window after the section so the taskbar says where you are', async () => {
    await renderShell();
    fireEvent.click(screen.getByRole('button', { name: 'Benchmarks' }));
    expect(document.title).toContain('Benchmarks');
  });

  it('opens settings and returns focus to the trigger on close', async () => {
    await renderShell();
    const nav = screen.getByRole('navigation', { name: 'Main navigation' });
    const trigger = within(nav).getByRole('button', { name: 'Settings' });

    trigger.focus();
    fireEvent.click(trigger);

    // The dialog is a lazy chunk: this waits for a real module import, not a
    // state change, and on a loaded machine that takes longer than the 1 s
    // default. Fails on unmodified HEAD under concurrent builds (2026-09-27).
    // The chunk is pre-loaded in `beforeAll`, so this waits on a render, not
    // an import; the margin covers a loaded machine, below testTimeout.
    const dialog = await screen.findByRole('dialog', {}, { timeout: 8000 });
    expect(within(dialog).getByRole('tab', { name: 'Appearance' })).toBeTruthy();

    fireEvent.keyDown(dialog, { key: 'Escape' });

    // Without focus return, dismissing the dialog drops a keyboard user at the
    // top of the document, and they must tab all the way back.
    await waitFor(() => {
      expect(screen.queryByRole('dialog')).toBeNull();
      expect(document.activeElement).toBe(trigger);
    });
    // The 8 s wait above only works because the suite's testTimeout (15 s,
    // vite.config.ts) exceeds it; vitest's 5 s default fired first.
  });

  it('forces the sidebar to icons only below the narrow breakpoint', async () => {
    stubViewport(760);
    await renderShell();

    // An expanded 224px sidebar at 720px leaves less than one card of content.
    expect(screen.getByRole('button', { name: 'Expand sidebar' })).toBeTruthy();
    // The stored preference is untouched, so widening restores what the user
    // actually chose rather than what the viewport imposed.
    expect(useSettings.getState().settings.sidebarCollapsed).toBe(false);
  });

  it('does not write a collapse preference the user cannot see', async () => {
    stubViewport(760);
    await renderShell();

    fireEvent.click(screen.getByRole('button', { name: 'Expand sidebar' }));

    expect(useSettings.getState().settings.sidebarCollapsed).toBe(false);
  });

  it('constrains content width so an ultrawide does not stretch it', async () => {
    await renderShell();
    const column = screen.getByRole('main').firstElementChild as HTMLElement;
    expect(column.className).toContain('max-w-[var(--content-max)]');
    expect(column.className).toContain('mx-auto');
  });

  it('passes a definite height from main down to the screen so only regions inside it scroll', async () => {
    // happy-dom does no layout, so this checks the chain rather than pixels
    // (the pixels were measured live over CDP). Drop `h-full` or `flex-1`
    // from any link and the screen's `flex-1` table resolves to auto height,
    // the page grows to its content and `<main>` scrolls the toolbar away.
    await renderShell();
    const column = screen.getByRole('main').firstElementChild as HTMLElement;
    expect(column.className).toMatch(/\bflex\b.*\bh-full\b|\bh-full\b.*\bflex\b/);
    expect(column.className).toContain('flex-col');

    const route = document.querySelector('[data-route-visible]');
    let link = route as HTMLElement | null;
    while (link !== null && link !== column) {
      expect(link.className, `wrapper <${link.tagName}> breaks the chain`).toContain('flex-1');
      expect(link.className).toContain('min-h-0');
      link = link.parentElement;
    }
    expect(link).toBe(column);
  });

  it('translates the whole shell when the locale changes', async () => {
    await renderShell();
    await i18n.changeLanguage('ro');

    await waitFor(() => {
      expect(screen.getByRole('navigation', { name: /navigare|Main navigation/i })).toBeTruthy();
      expect(screen.getByRole('button', { name: 'Procese' })).toBeTruthy();
    });
  });

  describe('keyboard shortcuts', () => {
    it('Ctrl+K opens the command palette', async () => {
      await renderShell();
      fireEvent.keyDown(window, { key: 'k', ctrlKey: true });

      const combobox = await screen.findByRole('combobox');
      expect(combobox).toBeTruthy();
      expect(screen.getByRole('listbox')).toBeTruthy();
    });

    it('the palette navigates on Enter and closes', async () => {
      await renderShell();
      fireEvent.keyDown(window, { key: 'k', ctrlKey: true });
      const combobox = await screen.findByRole('combobox');

      fireEvent.change(combobox, { target: { value: 'users' } });
      fireEvent.keyDown(combobox, { key: 'Enter' });

      expect(useSettings.getState().route).toBe('users');
      await waitFor(() => {
        expect(screen.queryByRole('combobox')).toBeNull();
      });
    });

    it('? opens the help sheet listing the shortcuts', async () => {
      await renderShell();
      fireEvent.keyDown(window, { key: '?', shiftKey: true });

      const dialog = await screen.findByRole('dialog', { name: 'Keyboard shortcuts' });
      expect(within(dialog).getByText('Search commands')).toBeTruthy();
    });

    it('Ctrl+, opens settings', async () => {
      await renderShell();
      fireEvent.keyDown(window, { key: ',', ctrlKey: true });

      const dialog = await screen.findByRole('dialog');
      expect(within(dialog).getByRole('tab', { name: 'Appearance' })).toBeTruthy();
    });

    it('Ctrl+3 jumps to the third section', async () => {
      await renderShell();
      fireEvent.keyDown(window, { key: '3', ctrlKey: true });
      expect(useSettings.getState().route).toBe('processes');
    });

    it('does nothing while the user is typing in a text box', async () => {
      await renderShell();
      const box = document.createElement('input');
      document.body.append(box);
      box.focus();

      fireEvent.keyDown(box, { key: '?', shiftKey: true });
      fireEvent.keyDown(box, { key: '3', ctrlKey: true });

      expect(screen.queryByRole('dialog')).toBeNull();
      expect(useSettings.getState().route).toBe('dashboard');
      box.remove();
    });
  });
});
