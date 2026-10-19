import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest';

import type { Alert } from '@vitals/protocol';
import { i18n, initI18n } from '@vitals/i18n';

import { takeDriveCleanup } from '../../../shell/navigation';
import { registerDashboardStrings } from '../strings';
import { AlertsWidget } from './AlertsWidget';

beforeAll(async () => {
  await initI18n();
  registerDashboardStrings();
});

afterEach(async () => {
  cleanup();
  takeDriveCleanup();
  await i18n.changeLanguage('en');
});

function alert(overrides: Partial<Alert> = {}): Alert {
  return {
    kind: 'diskSpace',
    severity: 'critical',
    subject: 'C:',
    title: 'alert.diskSpace.title',
    cause: 'alert.diskSpace.cause',
    values: { disk: 'C:', percent: 4 },
    route: 'storage',
    sinceSample: 0,
    ...overrides,
  };
}

describe('alerts widget', () => {
  it('a low-space alert opens the clean-up wizard for its drive on Storage', () => {
    const onNavigate = vi.fn();
    render(<AlertsWidget alerts={[alert()]} onNavigate={onNavigate} />);

    fireEvent.click(screen.getByRole('button', { name: 'Clean up this drive' }));

    expect(onNavigate).toHaveBeenCalledWith('storage');
    expect(takeDriveCleanup()).toBe('C:');
    expect(screen.queryByRole('button', { name: 'Investigate' })).toBeNull();
  });

  it('other alerts keep their Investigate link and request no clean-up', () => {
    const onNavigate = vi.fn();
    render(
      <AlertsWidget
        alerts={[
          alert({ kind: 'diskLatency', title: 'alert.diskLatency.title', route: 'processes' }),
        ]}
        onNavigate={onNavigate}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: 'Investigate' }));
    expect(onNavigate).toHaveBeenCalledWith('processes');
    expect(takeDriveCleanup()).toBeNull();
  });

  it('names the action in Romanian', async () => {
    await i18n.changeLanguage('ro');
    render(<AlertsWidget alerts={[alert()]} onNavigate={vi.fn()} />);
    expect(screen.getByRole('button', { name: 'Curăță această unitate' })).toBeTruthy();
  });
});
