/**
 * The strip repeats the alert engine's verdict and never makes up its own:
 * "all clear" only when the engine found nothing, the most severe alert by
 * name otherwise, and no verdict at all before the first measurement.
 */

import { fireEvent, render, screen } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { Alert } from '@vitals/protocol';

import { HealthStrip, topAlert } from './HealthStrip';
import { registerDashboardStrings } from './strings';

const overheating: Alert = {
  kind: 'thermalCpu',
  severity: 'critical',
  subject: '',
  title: 'alert.thermalCpu.title',
  cause: 'alert.thermalCpu.cause',
  values: { celsius: 99 },
  route: 'performance',
  sinceSample: 9,
};

const minor: Alert = { ...overheating, kind: 'cpuSustained', severity: 'warning', sinceSample: 1 };

beforeAll(async () => {
  await initI18n();
  registerDashboardStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

describe('HealthStrip', () => {
  it('says all clear when the alert engine found nothing', () => {
    render(<HealthStrip alerts={[]} measured onDiagnose={vi.fn()} />);
    expect(screen.getByRole('status').textContent).toBe(
      'All clear: nothing needs your attention right now.',
    );
  });

  it('names the alert, by the engine’s own title, when something is wrong', () => {
    render(<HealthStrip alerts={[overheating]} measured onDiagnose={vi.fn()} />);
    expect(screen.getByRole('status').textContent).toBe(
      'Needs attention: The processor is overheating',
    );
  });

  it('gives no verdict about a machine it has not measured yet', () => {
    render(<HealthStrip alerts={[]} measured={false} onDiagnose={vi.fn()} />);
    expect(screen.getByRole('status').textContent).toBe('Waiting for the first readings…');
    expect(
      screen.getByRole('button', { name: 'Why is my PC slow?' }).hasAttribute('disabled'),
    ).toBe(true);
  });

  it('opens the diagnosis from its primary button', () => {
    const onDiagnose = vi.fn();
    render(<HealthStrip alerts={[]} measured onDiagnose={onDiagnose} />);
    fireEvent.click(screen.getByRole('button', { name: 'Why is my PC slow?' }));
    expect(onDiagnose).toHaveBeenCalledTimes(1);
  });

  it('is a named region so a screen reader can find it', () => {
    render(<HealthStrip alerts={[]} measured onDiagnose={vi.fn()} />);
    expect(screen.getByRole('region', { name: 'How your computer is doing' })).toBeTruthy();
  });
});

describe('topAlert', () => {
  it('prefers the most severe alert even when a milder one is older', () => {
    expect(topAlert([minor, overheating])).toBe(overheating);
  });

  it('breaks a tie in severity by age, so the named alert does not flicker', () => {
    const newer = { ...minor, subject: 'b', sinceSample: 5 };
    expect(topAlert([newer, minor])).toBe(minor);
  });

  it('has nothing to name when there are no alerts', () => {
    expect(topAlert([])).toBeNull();
  });
});
