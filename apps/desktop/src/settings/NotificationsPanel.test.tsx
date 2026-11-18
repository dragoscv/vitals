/**
 * The alert-rule editor's round trip.
 *
 * The row's done-means asks for a round trip through Settings > Alerts. This
 * is that, through the real panel and the real store: a rule is added, its
 * fields are edited, and it is removed. It lives in the frontend suite rather
 * than the browser suite, so it runs on every commit instead of only in the
 * end-to-end job — the browser job has no Tauri host and the settings store
 * persists through the Tauri store plugin, so a rule written there would not
 * survive a reload to prove anything anyway.
 *
 * The rule's key guarantee is asserted here too: the settings type *is* the
 * wire type (`Rule` comes from the generated protocol), so what the row
 * stores is byte-for-byte what the Rust engine receives.
 */

import { fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { registerShellStrings } from '../shell/strings';
import { NotificationsPanel } from './panels';
import { resetSettingsForTests, useSettings } from './store';

// The channel section asks the host for each channel's last result. No host
// here, which is a state the panel must work in anyway.
vi.mock('../shell/host', () => ({ hasTauriHost: () => false }));

beforeAll(async () => {
  await initI18n();
  registerShellStrings();
});

beforeEach(() => {
  resetSettingsForTests();
});

function rules() {
  return useSettings.getState().settings.alertRules;
}

describe('the alert-rule editor', () => {
  it('starts with no rules, so a monitor invents no conditions of its own', () => {
    render(<NotificationsPanel />);
    expect(rules().length).toBe(0);
    expect(screen.queryByLabelText('Rule name')).toBeNull();
  });

  it('adds a rule the engine can use and shows its fields', () => {
    render(<NotificationsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Add a rule' }));

    expect(rules().length).toBe(1);
    const rule = rules()[0];
    expect(rule?.name).toBe('New rule');
    expect(rule?.metric).toBe('cpuPercent');
    expect(rule?.comparator).toBe('above');
    expect(rule?.threshold).toBe(90);
    expect(rule?.sustainSecs).toBe(30);
    expect(rule?.severity).toBe('warning');

    // The fields the user sees carry those values, so the row is not merely
    // in the store behind an empty form.
    const name = screen.getByLabelText('Rule name');
    expect(name).toHaveProperty('value', 'New rule');
    expect(screen.getByLabelText('Threshold')).toHaveProperty('value', '90');
    expect(screen.getByLabelText('Seconds it must hold')).toHaveProperty('value', '30');
  });

  it('writes every edit back to the store the engine is fed from', () => {
    render(<NotificationsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Add a rule' }));

    fireEvent.change(screen.getByLabelText('Rule name'), {
      target: { value: 'Fans are noisy' },
    });
    expect(rules()[0]?.name).toBe('Fans are noisy');

    fireEvent.change(screen.getByLabelText('Threshold'), { target: { value: '95' } });
    expect(rules()[0]?.threshold).toBe(95);

    fireEvent.change(screen.getByLabelText('Seconds it must hold'), {
      target: { value: '120' },
    });
    expect(rules()[0]?.sustainSecs).toBe(120);
  });

  it('ignores a half-typed number instead of storing something the engine would drop', () => {
    render(<NotificationsPanel />);
    fireEvent.click(screen.getByRole('button', { name: 'Add a rule' }));

    // An empty field is what a user sees while retyping. Storing NaN here
    // would be a rule that Rust silently discards, with nothing on screen to
    // say why it stopped firing.
    fireEvent.change(screen.getByLabelText('Threshold'), { target: { value: '' } });
    expect(rules()[0]?.threshold).toBe(90);
    fireEvent.change(screen.getByLabelText('Threshold'), { target: { value: 'abc' } });
    expect(rules()[0]?.threshold).toBe(90);
  });

  it('removes only the rule that was removed', () => {
    render(<NotificationsPanel />);
    const add = screen.getByRole('button', { name: 'Add a rule' });
    fireEvent.click(add);
    fireEvent.click(add);
    expect(rules().length).toBe(2);

    // The first row's button, so a bug that always removes the last element
    // is caught rather than hidden.
    fireEvent.click(screen.getAllByRole('button', { name: 'Remove this rule' })[0] as HTMLElement);
    expect(rules().length).toBe(1);
    expect(screen.getAllByLabelText('Rule name').length).toBe(1);
  });
});
