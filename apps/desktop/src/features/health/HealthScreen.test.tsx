import { render, screen, within } from '@testing-library/react';
import { beforeAll, describe, expect, it } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { HealthError } from './api';
import { SPARSE, fakeHealthApi } from './fixtures.test-utils';
import { HealthScreen } from './HealthScreen';
import { registerHealthStrings } from './strings';

beforeAll(async () => {
  await initI18n('en');
  registerHealthStrings();
});

/** The value cell next to a label, inside one card. */
function valueOf(card: string, label: string): string {
  const region = screen.getByRole('region', { name: card });
  const term = within(region).getByText(label, { selector: 'dt' });
  return term.nextElementSibling?.textContent ?? '';
}

describe('HealthScreen', () => {
  it('shows an em dash, never a zero, for every reading vmui did not supply', async () => {
    render(<HealthScreen api={fakeHealthApi({ fetch: () => Promise.resolve(SPARSE) })} />);
    await screen.findByRole('region', { name: 'Activity' });

    expect(valueOf('Activity', 'Steps today')).toBe('—');
    expect(valueOf('Heart', 'Heart rate')).toBe('—');
    expect(valueOf('Heart', 'Blood pressure')).toBe('—');
    expect(valueOf('Sleep', 'Sleep last night')).toBe('—');
    expect(valueOf('Body', 'Body mass index')).toBe('—');
    expect(valueOf('Food and water', 'Calories eaten')).toBe('—');
    expect(valueOf('Food and water', 'Water')).toBe('—');
    expect(document.body.textContent).not.toMatch(/\b0 (bpm|kcal|km)\b/);
    // No estimate label beside an absent estimate.
    expect(screen.queryByText('estimate')).toBeNull();
  });

  it('shows the readings of a full body, labels estimates, and says it is not medical advice', async () => {
    render(<HealthScreen api={fakeHealthApi()} />);
    await screen.findByRole('region', { name: 'Activity' });

    expect(valueOf('Activity', 'Steps today')).toBe('8,123');
    expect(valueOf('Activity', 'Distance')).toBe('6.1 km');
    expect(valueOf('Heart', 'Heart rate')).toBe('72 bpm');
    expect(valueOf('Heart', 'Blood pressure')).toBe('121/79 mmHg');
    expect(valueOf('Sleep', 'Sleep last night')).toBe('6 h 52 min');
    expect(valueOf('Body', 'Body fat')).toBe('18.2 %');
    expect(valueOf('Body', 'Body mass index')).toContain('27.2');
    expect(valueOf('Body', 'Body mass index')).toContain('estimate');
    expect(valueOf('Food and water', 'Protein')).toBe('98 / 160 g');
    expect(valueOf('Food and water', 'Water')).toBe('1,500 / 2,500 ml');
    expect(screen.getByRole('img', { name: /heart rate over the last 24 hours/i })).toBeTruthy();
    expect(screen.getByText(/not medical advice/i)).toBeTruthy();
    expect(screen.getByText(/last updated/i)).toBeTruthy();
  });

  it('offers to pair again when vmui no longer knows this device', async () => {
    render(
      <HealthScreen
        api={fakeHealthApi({ fetch: () => Promise.reject(new HealthError('signedOut')) })}
      />,
    );
    expect(await screen.findByText(/no longer paired/i)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Pair again' })).toBeTruthy();
  });

  it('says the account is not the owner, without offering a pairing that would not help', async () => {
    render(
      <HealthScreen
        api={fakeHealthApi({ fetch: () => Promise.reject(new HealthError('notOwner')) })}
      />,
    );
    expect(await screen.findByText('This vmui account is not the owner')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Pair again' })).toBeNull();
  });
});
