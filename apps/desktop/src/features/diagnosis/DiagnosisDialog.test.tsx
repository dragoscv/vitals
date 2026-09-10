import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { Alert, Diagnosis } from '@vitals/protocol';

import { HistoryCollector } from '../dashboard/history';
import { registerDashboardStrings } from '../dashboard/strings';
import { makeProcess, makeProcessMap, makeSystem } from '../dashboard/test-fixtures';
import { DiagnosisDialog } from './DiagnosisDialog';
import { formatReport, seriesFor } from './report';
import type { DiagnosisSource } from './source';

beforeAll(async () => {
  await initI18n();
  registerDashboardStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

const cpuAlert: Alert = {
  kind: 'cpuSustained',
  severity: 'warning',
  subject: '',
  title: 'alert.cpuSustained.title',
  cause: 'alert.cpuSustained.cause',
  values: { percent: 91, seconds: 15 },
  route: 'processes',
  sinceSample: 1,
};

const diskAlert: Alert = {
  kind: 'diskSpace',
  severity: 'info',
  subject: 'C:',
  title: 'alert.diskSpace.title',
  cause: 'alert.diskSpace.cause',
  values: { disk: 'C:', percent: 4 },
  route: 'storage',
  sinceSample: 1,
};

const busy: Diagnosis = {
  verdict: {
    alert: cpuAlert,
    subsystem: 'cpu',
    contributors: [
      { name: 'chrome.exe', pid: 4242, share: 61, value: 55.6 },
      { name: 'code.exe', pid: 77, share: 22, value: 20.1 },
    ],
    diffuse: false,
  },
  also: [diskAlert],
};

const healthy: Diagnosis = { verdict: null, also: [] };

function sourceReturning(diagnosis: Diagnosis | Error): DiagnosisSource & { calls: number } {
  const source = {
    calls: 0,
    diagnose: () => {
      source.calls += 1;
      return diagnosis instanceof Error ? Promise.reject(diagnosis) : Promise.resolve(diagnosis);
    },
  };
  return source;
}

function renderDialog(source: DiagnosisSource, options: { clipboard?: string[] } = {}) {
  const clipboard = options.clipboard ?? [];
  const history = new HistoryCollector();
  const system = makeSystem();
  history.push({ system, processes: new Map(), seq: 1, elapsedMs: 1000, timestampMs: 1000 });
  const onOpenChange = vi.fn();
  render(
    <DiagnosisDialog
      open
      onOpenChange={onOpenChange}
      source={source}
      system={system}
      processes={makeProcessMap([makeProcess({ name: 'chrome.exe', cpu: 55 })])}
      history={history.current}
      locale="en"
      writeClipboard={(text) => {
        clipboard.push(text);
        return Promise.resolve();
      }}
    />,
  );
  return { clipboard, onOpenChange };
}

describe('DiagnosisDialog', () => {
  it('names the processes responsible, most responsible first, with their share', async () => {
    renderDialog(sourceReturning(busy));

    await screen.findByText('The processor has been busy for a while');
    const region = screen.getByRole('region', { name: 'Responsible' });
    const items = within(region).getAllByRole('listitem');
    expect(items[0]?.textContent).toContain('chrome.exe');
    expect(items[0]?.textContent).toContain('61% of the load');
    expect(items[1]?.textContent).toContain('code.exe');
    // The secondary problem is shown but not promoted to the headline.
    expect(screen.getByText('Also noticed')).toBeDefined();
    expect(screen.getByText('C: is nearly full')).toBeDefined();
  });

  it('says nothing is wrong as a real answer, not an empty list', async () => {
    renderDialog(sourceReturning(healthy));
    await screen.findByText('Nothing is holding your computer back.');
    expect(screen.queryByText('Responsible')).toBeNull();
  });

  it('copies a plain-text report that a person can paste into a ticket', async () => {
    const { clipboard } = renderDialog(sourceReturning(busy));
    await screen.findByText('The processor has been busy for a while');

    fireEvent.click(screen.getByRole('button', { name: 'Copy report' }));
    await screen.findByRole('button', { name: 'Copied' });

    expect(clipboard).toHaveLength(1);
    const text = clipboard[0] ?? '';
    expect(text).toContain('Vitals — why is my PC slow?');
    expect(text).toContain('chrome.exe (PID 4242) — 61%');
    expect(text).toContain('Also noticed:');
    expect(text).toContain('C: is nearly full');
    // No JSON, no i18n keys leaking through.
    expect(text).not.toMatch(/alert\.[a-zA-Z]+\.(title|cause)/);
    expect(text).not.toContain('{');
  });

  it('reports a failure to reach the engine rather than pretending the machine is healthy', async () => {
    renderDialog(sourceReturning(new Error('ipc down')));
    await screen.findByRole('alert');
    expect(screen.queryByText('Nothing is holding your computer back.')).toBeNull();
    expect(screen.getByRole<HTMLButtonElement>('button', { name: 'Copy report' }).disabled).toBe(
      true,
    );
  });

  it('asks the engine once per opening, not once per frame', async () => {
    const source = sourceReturning(busy);
    renderDialog(source);
    await screen.findByText('The processor has been busy for a while');
    expect(source.calls).toBe(1);
  });
});

describe('report', () => {
  it('renders in Romanian when that is the language', async () => {
    await i18n.changeLanguage('ro');
    const text = formatReport(busy, {
      t: i18n.getFixedT('ro', 'dashboard'),
      locale: 'ro',
      system: makeSystem(),
      generatedAt: new Date(2026, 0, 1, 12, 0),
    });
    expect(text).toContain('Procese responsabile:');
    expect(text).toContain('Procesorul');
    expect(text).not.toMatch(/alert\./);
  });

  it('has no time series for a state that is not a load, so no misleading chart is drawn', () => {
    const history = new HistoryCollector().current;
    expect(seriesFor('storage', history)).toBeNull();
    expect(seriesFor('battery', history)).toBeNull();
    expect(seriesFor('cpu', history)).toBe(history.core.cpu);
    expect(seriesFor('thermal', history)).toBe(history.core.cpu);
  });
});
