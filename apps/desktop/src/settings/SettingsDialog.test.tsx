import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { useState } from 'react';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { initI18n, i18n } from '@vitals/i18n';

import { registerShellStrings } from '../shell/strings';
import { ThemeProvider } from '../theme/ThemeProvider';
import type { SettingsBackend } from './persistence';
import { SettingsDialog } from './SettingsDialog';
import { flushSettings, resetSettingsForTests, setSettingsBackend, useSettings } from './store';

function memoryBackend() {
  let stored: unknown;
  const backend: SettingsBackend = {
    load: () => Promise.resolve(stored),
    save: (value) => {
      stored = value;
      return Promise.resolve();
    },
  };
  return { backend, read: () => stored as Record<string, unknown> | undefined };
}

/** Mirrors how the shell mounts it: theme changes must reach the provider. */
function Harness() {
  const settings = useSettings((state) => state.settings);
  const setTheme = useSettings((state) => state.setTheme);
  const [open, setOpen] = useState(true);

  return (
    <ThemeProvider initial={settings.theme} onChange={setTheme}>
      <SettingsDialog open={open} onOpenChange={setOpen} version="1.2.3" />
    </ThemeProvider>
  );
}

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
  resetSettingsForTests();
});

/**
 * Activates a tab.
 *
 * Radix commits the selection on `mousedown`, not `click` — it deliberately
 * matches the platform behaviour of a real tab strip. A synthetic `click`
 * alone leaves the panel unchanged, which reads as a broken component rather
 * than an incomplete event sequence.
 */
function selectTab(scope: HTMLElement, name: string): void {
  const tab = within(scope).getByRole('tab', { name });
  fireEvent.mouseDown(tab);
  fireEvent.click(tab);
}

describe('SettingsDialog', () => {
  it('has an accessible name and every tab', async () => {
    setSettingsBackend(memoryBackend().backend);
    render(<Harness />);

    const dialog = await screen.findByRole('dialog', { name: 'Settings' });
    for (const name of [
      'General',
      'Appearance',
      'Sampling',
      'Notifications',
      'Privacy',
      'Advanced',
      'About',
    ]) {
      expect(within(dialog).getByRole('tab', { name }), `${name} tab missing`).toBeTruthy();
    }
  });

  it('shows exactly one panel at a time', async () => {
    setSettingsBackend(memoryBackend().backend);
    render(<Harness />);
    const dialog = await screen.findByRole('dialog');

    selectTab(dialog, 'About');

    await waitFor(() => {
      expect(within(dialog).getByText('Version 1.2.3')).toBeTruthy();
    });
    expect(within(dialog).getAllByRole('tabpanel')).toHaveLength(1);
  });

  it('shows no machine facts, and no spinner, when there is no host to ask', async () => {
    // Tests run without a Tauri host, so this is the path a browser preview
    // takes. The important part is that it *settles*: an About panel stuck on
    // a skeleton forever would look like a hang rather than an absence.
    setSettingsBackend(memoryBackend().backend);
    render(<Harness />);
    const dialog = await screen.findByRole('dialog');

    selectTab(dialog, 'About');

    await waitFor(() => {
      expect(within(dialog).getByText('Version 1.2.3')).toBeTruthy();
    });

    // The version and the issue links still render; only the facts block is
    // absent, and nothing is left loading.
    await waitFor(() => {
      expect(within(dialog).queryByText('Copy system details')).toBeNull();
      expect(within(dialog).queryByRole('status')).toBeNull();
    });
    expect(within(dialog).getByRole('button', { name: 'Open an issue' })).toBeTruthy();
  });

  it('applies an appearance change to the document immediately', async () => {
    setSettingsBackend(memoryBackend().backend);
    render(<Harness />);
    const dialog = await screen.findByRole('dialog');

    selectTab(dialog, 'Appearance');
    const dark = await within(dialog).findByRole('radio', { name: 'Dark' });
    fireEvent.click(dark);

    // Settings in this app have no Save button, so "changed" and "applied"
    // must be the same moment or the switch is lying.
    await waitFor(() => {
      expect(document.documentElement.classList.contains('dark')).toBe(true);
    });
  });

  it('persists an appearance change', async () => {
    const { backend, read } = memoryBackend();
    setSettingsBackend(backend);
    render(<Harness />);
    const dialog = await screen.findByRole('dialog');

    selectTab(dialog, 'Appearance');
    const swatches = within(dialog).getAllByRole('radio', { name: /accent/i });
    const target = swatches.find((node) => node.getAttribute('data-accent') === 'teal');
    fireEvent.click(target as HTMLElement);

    await flushSettings();

    expect((read()?.['theme'] as { accent: string }).accent).toBe('teal');
  });

  it('gives the accent group one tab stop with arrow keys inside it', async () => {
    setSettingsBackend(memoryBackend().backend);
    render(<Harness />);
    const dialog = await screen.findByRole('dialog');
    selectTab(dialog, 'Appearance');

    // Scoped by name: the theme-mode segmented control is also a radiogroup.
    const group = await within(dialog).findByRole('radiogroup', { name: 'Accent colour' });
    const swatches = within(group).getAllByRole('radio');

    // Ten identically-shaped colour buttons in the tab order would sit between
    // the theme selector and every setting below it.
    expect(swatches.filter((node) => (node as HTMLElement).tabIndex === 0)).toHaveLength(1);

    const selected = swatches.find((node) => node.getAttribute('aria-checked') === 'true');
    fireEvent.keyDown(selected as HTMLElement, { key: 'ArrowRight' });

    await waitFor(() => {
      expect(useSettings.getState().settings.theme.accent).not.toBe('blue');
    });
  });

  it('changes the interface language from the appearance tab', async () => {
    setSettingsBackend(memoryBackend().backend);
    render(<Harness />);
    const dialog = await screen.findByRole('dialog');
    selectTab(dialog, 'Appearance');

    useSettings.getState().patch({ locale: 'ro' });

    await waitFor(() => {
      expect(i18n.language).toBe('ro');
    });
  });

  it('disables options whose parent switch is off', async () => {
    setSettingsBackend(memoryBackend().backend);
    render(<Harness />);
    const dialog = await screen.findByRole('dialog');

    selectTab(dialog, 'Notifications');

    // Present but inert rather than hidden: a setting that vanishes leaves the
    // user hunting for something they know exists.
    const alerts = await within(dialog).findAllByRole('switch');
    const children = alerts.slice(1);
    expect(children.length).toBeGreaterThan(0);
    for (const alert of children) {
      expect(alert.getAttribute('data-disabled')).not.toBeNull();
    }
  });

  it('warns before advanced features and ties the warning to the switch', async () => {
    setSettingsBackend(memoryBackend().backend);
    render(<Harness />);
    const dialog = await screen.findByRole('dialog');

    selectTab(dialog, 'Advanced');

    const toggle = await within(dialog).findByRole('switch');
    const describedBy = toggle.getAttribute('aria-describedby');
    expect(describedBy).toBeTruthy();
    // The warning must be read at the point of decision, not merely be nearby.
    expect(document.getElementById(describedBy as string)?.textContent).toMatch(/damage hardware/i);
  });
});
