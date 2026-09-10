import { render, screen, within } from '@testing-library/react';
import { beforeAll, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { shortcuts } from './shortcuts';
import { ShortcutsHelp } from './ShortcutsHelp';
import { registerShellStrings, SHELL_NS } from './strings';

beforeAll(async () => {
  await initI18n('en');
  registerShellStrings();
});

describe('ShortcutsHelp', () => {
  it('lists every shortcut in the registry, so the sheet cannot drift from the bindings', () => {
    render(<ShortcutsHelp open onOpenChange={vi.fn()} />);
    const dialog = screen.getByRole('dialog');

    for (const shortcut of shortcuts) {
      const label = i18n.t(shortcut.labelKey, { ns: SHELL_NS });
      // A missing translation comes back as the key, which would still
      // "appear" — so assert the label resolved to prose first.
      expect(label, `${shortcut.id} has no label`).not.toBe(shortcut.labelKey);
      expect(within(dialog).getByText(label)).toBeTruthy();
      for (const key of shortcut.keys) {
        expect(within(dialog).getAllByText(key).length).toBeGreaterThan(0);
      }
    }
  });

  it('has a Romanian label for every shortcut too', () => {
    for (const shortcut of shortcuts) {
      const label = i18n.t(shortcut.labelKey, { ns: SHELL_NS, lng: 'ro' });
      expect(label, `${shortcut.id} has no Romanian label`).not.toBe(shortcut.labelKey);
    }
  });
});
