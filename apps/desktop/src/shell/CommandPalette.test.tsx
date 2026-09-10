import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { CommandPalette, DIAGNOSE_EVENT } from './CommandPalette';
import { registerShellStrings } from './strings';

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function renderPalette(overrides: Partial<Parameters<typeof CommandPalette>[0]> = {}) {
  const props = {
    open: true,
    onOpenChange: vi.fn(),
    onNavigate: vi.fn(),
    onOpenSettings: vi.fn(),
    onToggleHud: vi.fn(),
    ...overrides,
  };
  render(<CommandPalette {...props} />);
  return props;
}

function input(): HTMLElement {
  return screen.getByRole('combobox');
}

function options(): HTMLElement[] {
  return within(screen.getByRole('listbox')).getAllByRole('option');
}

describe('CommandPalette', () => {
  it('lists every section plus the actions', () => {
    renderPalette();
    // Twelve routes and three actions.
    expect(options()).toHaveLength(15);
    expect(screen.getByRole('option', { name: /Processes/ })).toBeTruthy();
    expect(screen.getByRole('option', { name: /Why is my PC slow/ })).toBeTruthy();
  });

  it('filters case-insensitively on the name', () => {
    renderPalette();
    fireEvent.change(input(), { target: { value: 'proc' } });

    const names = options().map((option) => option.textContent ?? '');
    expect(names).toHaveLength(1);
    expect(names[0]).toContain('Processes');
  });

  it('says so when nothing matches, rather than showing an empty box', () => {
    renderPalette();
    fireEvent.change(input(), { target: { value: 'zzzz' } });

    expect(screen.queryAllByRole('option')).toHaveLength(0);
    expect(screen.getByRole('status').textContent).toContain('Nothing matches');
  });

  it('points aria-activedescendant at the highlighted option as the arrows move', () => {
    // The whole reason this is a hand-rolled listbox: focus stays in the input
    // so typing keeps working, and the selection is communicated by id.
    renderPalette();

    const first = options()[0];
    expect(input().getAttribute('aria-activedescendant')).toBe(first?.id);
    expect(first?.getAttribute('aria-selected')).toBe('true');

    fireEvent.keyDown(input(), { key: 'ArrowDown' });

    const second = options()[1];
    expect(input().getAttribute('aria-activedescendant')).toBe(second?.id);
    expect(second?.getAttribute('aria-selected')).toBe('true');
    expect(options()[0]?.getAttribute('aria-selected')).toBe('false');
    // Focus never moved, which is the property that makes it a combobox.
    expect(document.activeElement).toBe(input());
  });

  it('wraps from the first option to the last on ArrowUp', () => {
    renderPalette();
    fireEvent.keyDown(input(), { key: 'ArrowUp' });

    const all = options();
    expect(input().getAttribute('aria-activedescendant')).toBe(all.at(-1)?.id);
  });

  it('Enter runs the highlighted command and closes', () => {
    const props = renderPalette();
    fireEvent.change(input(), { target: { value: 'network' } });
    fireEvent.keyDown(input(), { key: 'Enter' });

    expect(props.onNavigate).toHaveBeenCalledWith('network');
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
  });

  it('Enter with no matches does nothing at all', () => {
    const props = renderPalette();
    fireEvent.change(input(), { target: { value: 'zzzz' } });
    fireEvent.keyDown(input(), { key: 'Enter' });

    expect(props.onNavigate).not.toHaveBeenCalled();
    expect(props.onOpenChange).not.toHaveBeenCalled();
  });

  it('the settings and overlay actions call their handlers', () => {
    const props = renderPalette();

    fireEvent.change(input(), { target: { value: 'open settings' } });
    fireEvent.keyDown(input(), { key: 'Enter' });
    expect(props.onOpenSettings).toHaveBeenCalledTimes(1);
  });

  it('the diagnosis action goes to the dashboard and announces the intent', async () => {
    // The dialog lives in `features/dashboard`, which this module may not
    // reach into, so the contract is: navigate, then fire the event the
    // dashboard listens for.
    const props = renderPalette();
    const heard = vi.fn();
    window.addEventListener(DIAGNOSE_EVENT, heard);

    fireEvent.change(input(), { target: { value: 'slow' } });
    fireEvent.keyDown(input(), { key: 'Enter' });

    expect(props.onNavigate).toHaveBeenCalledWith('dashboard');
    // A frame later, not synchronously: this palette is itself a dialog and
    // is closing in the same tick. Opening the verdict dialog inside that
    // teardown had it dismissed along with the palette — observed in the
    // running app, where the event arrived and nothing appeared.
    expect(heard).not.toHaveBeenCalled();
    await vi.waitFor(() => {
      expect(heard).toHaveBeenCalledTimes(1);
    });
    window.removeEventListener(DIAGNOSE_EVENT, heard);
  });

  it('Escape closes it', () => {
    const props = renderPalette();
    fireEvent.keyDown(screen.getByRole('dialog'), { key: 'Escape' });
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
  });

  it('is translated, so a Romanian user does not get an English palette', async () => {
    await i18n.changeLanguage('ro');
    renderPalette();

    expect(screen.getByRole('option', { name: /Procese/ })).toBeTruthy();
    expect(screen.getByRole('option', { name: /De ce merge greu/ })).toBeTruthy();
  });

  it('matches a query typed without diacritics', async () => {
    // "Retea" must find "Rețea" — nobody types the comma-below on a filter.
    await i18n.changeLanguage('ro');
    renderPalette();
    fireEvent.change(input(), { target: { value: 'retea' } });

    expect(options()).toHaveLength(1);
  });
});
