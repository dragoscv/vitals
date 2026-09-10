import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';
import type { Alert } from '@vitals/protocol';

import { createManualAlertSource } from '../alerts/useAlerts';
import { DashboardScreen } from './DashboardScreen';
import { HistoryCollector } from './history';
import { registerDashboardStrings } from './strings';
import { makeProcess, makeProcessMap, makeSystem } from './test-fixtures';
import type { LayoutBackend } from './useLayout';
import { createManualSystemSource, NO_SAMPLER } from './useSystemSnapshot';
import { defaultLayout, type DashboardLayout } from './widgets';

beforeAll(async () => {
  await initI18n();
  registerDashboardStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function memoryBackend(initial?: unknown): LayoutBackend & { saved: unknown[] } {
  const saved: unknown[] = [];
  return {
    saved,
    load: () => Promise.resolve(initial),
    save: (value) => {
      saved.push(value);
      return Promise.resolve();
    },
  };
}

/** A source already holding one frame, so the screen is past its skeletons. */
function liveSource(overrides: Parameters<typeof makeSystem>[0] = {}) {
  const source = createManualSystemSource();
  source.push({
    system: makeSystem(overrides),
    processes: makeProcessMap([
      makeProcess({ name: 'chrome.exe', cpu: 22, memoryPrivate: 900_000_000 }),
      makeProcess({ name: 'code.exe', cpu: 8, memoryPrivate: 400_000_000 }),
    ]),
  });
  return source;
}

async function renderDashboard(
  options: {
    layout?: DashboardLayout;
    source?: ReturnType<typeof liveSource>;
    alerts?: readonly Alert[];
    onNavigate?: (route: string) => void;
  } = {},
) {
  const backend = memoryBackend(options.layout);
  const collector = new HistoryCollector();
  const view = render(
    <DashboardScreen
      source={options.source ?? liveSource()}
      alertSource={createManualAlertSource(options.alerts ?? [])}
      layoutBackend={backend}
      historyCollector={collector}
      onNavigate={options.onNavigate as never}
    />,
  );
  // Layout hydration is a promise; without flushing it the screen is still
  // showing skeletons and every query below would fail for the wrong reason.
  await screen.findByRole('heading', { name: 'Dashboard' });
  return { ...view, backend, collector };
}

describe('DashboardScreen', () => {
  it('shows skeletons until the first frame arrives', () => {
    // Rendering zeroes before the sampler has reported would state that the
    // machine is idle, which is a measurement nobody took.
    render(
      <DashboardScreen
        source={createManualSystemSource()}
        layoutBackend={memoryBackend()}
        historyCollector={new HistoryCollector()}
      />,
    );

    expect(document.querySelector('[aria-busy="true"]')).toBeTruthy();
  });

  it('explains itself instead of showing skeletons forever', async () => {
    // Found by opening the running dev server: with no Tauri host no frame
    // will EVER arrive, and the grid sat on its skeletons indefinitely. That
    // is the splash-screen failure again in a different place — an unbounded
    // loading state makes a broken app look merely busy, so nobody reports it
    // and there is nothing on screen to diagnose from.
    const source = createManualSystemSource();
    source.push({ pending: false, system: null, error: NO_SAMPLER });

    render(
      <DashboardScreen
        source={source}
        layoutBackend={memoryBackend()}
        historyCollector={new HistoryCollector()}
      />,
    );

    expect(await screen.findByText('No readings are arriving')).toBeTruthy();
    expect(document.querySelector('[aria-busy="true"]')).toBeNull();
  });

  it('renders the default widgets once data lands', async () => {
    await renderDashboard();

    for (const name of ['CPU', 'Memory', 'Disk', 'Network']) {
      expect(screen.getByRole('region', { name })).toBeTruthy();
    }
  });

  it('names each widget region so they are distinguishable', async () => {
    // A dashboard is a wall of similar panels; without names a screen reader
    // user must read the contents of each to tell them apart.
    await renderDashboard();
    const regions = screen.getAllByRole('region');
    const names = regions.map((region) => region.getAttribute('aria-label'));

    expect(new Set(names).size).toBe(names.length);
  });

  it('hides hardware widgets the machine cannot support', async () => {
    // A stored layout containing a GPU widget, restored on a machine with none.
    await renderDashboard({
      layout: [
        { id: 'cpu', size: 'half' },
        { id: 'gpu', size: 'half' },
      ],
      source: liveSource({ gpus: [] }),
    });

    expect(screen.getByRole('region', { name: 'CPU' })).toBeTruthy();
    expect(screen.queryByRole('region', { name: 'GPU' })).toBeNull();
  });

  it('shows the GPU widget when an adapter is present', async () => {
    await renderDashboard({
      layout: [{ id: 'gpu', size: 'half' }],
      source: liveSource({ gpus: [{ id: 0, name: 'RTX 3060 Ti' }] }),
    });

    expect(screen.getByRole('region', { name: 'GPU' })).toBeTruthy();
  });

  describe('edit mode', () => {
    it('keeps layout controls out of the tab order until editing', async () => {
      // Mounted or not, never hidden with CSS: twelve widgets would otherwise
      // put thirty-six invisible buttons between the user and the content.
      await renderDashboard();
      expect(screen.queryByRole('button', { name: 'Move up' })).toBeNull();

      fireEvent.click(screen.getByRole('button', { name: 'Edit layout' }));
      expect(screen.getAllByRole('button', { name: 'Move up' }).length).toBeGreaterThan(0);
    });

    it('disables removal of an essential widget', async () => {
      await renderDashboard({ layout: [{ id: 'cpu', size: 'half' }] });
      fireEvent.click(screen.getByRole('button', { name: 'Edit layout' }));

      const cpu = screen.getByRole('region', { name: 'CPU' });
      const button = within(cpu).getByRole('button', {
        name: 'This widget cannot be removed.',
      });
      expect(button.hasAttribute('disabled')).toBe(true);
    });

    it('removes a non-essential widget and persists the change', async () => {
      const { backend } = await renderDashboard({
        layout: [
          { id: 'cpu', size: 'half' },
          { id: 'disk', size: 'half' },
        ],
      });

      fireEvent.click(screen.getByRole('button', { name: 'Edit layout' }));
      const disk = screen.getByRole('region', { name: 'Disk' });
      fireEvent.click(within(disk).getByRole('button', { name: 'Remove' }));

      expect(screen.queryByRole('region', { name: 'Disk' })).toBeNull();
      await vi.waitFor(() => {
        expect(backend.saved.at(-1)).toEqual([{ id: 'cpu', size: 'half' }]);
      });
    });

    it('disables the move arrows at the ends of the list', async () => {
      await renderDashboard({
        layout: [
          { id: 'cpu', size: 'half' },
          { id: 'memory', size: 'half' },
        ],
      });
      fireEvent.click(screen.getByRole('button', { name: 'Edit layout' }));

      const cpu = screen.getByRole('region', { name: 'CPU' });
      const memory = screen.getByRole('region', { name: 'Memory' });

      expect(within(cpu).getByRole('button', { name: 'Move up' }).hasAttribute('disabled')).toBe(
        true,
      );
      expect(
        within(memory).getByRole('button', { name: 'Move down' }).hasAttribute('disabled'),
      ).toBe(true);
    });

    it('restores the defaults', async () => {
      const { backend } = await renderDashboard({ layout: [{ id: 'cpu', size: 'half' }] });

      fireEvent.click(screen.getByRole('button', { name: 'Edit layout' }));
      fireEvent.click(screen.getByRole('button', { name: 'Reset to defaults' }));

      await vi.waitFor(() => {
        expect(backend.saved.at(-1)).toEqual(defaultLayout);
      });
    });
  });

  describe('alerts', () => {
    /** As the Rust engine emits it: keys, not prose, plus rounded values. */
    const overheating: Alert = {
      kind: 'thermalCpu',
      severity: 'critical',
      subject: '',
      title: 'alert.thermalCpu.title',
      cause: 'alert.thermalCpu.cause',
      values: { celsius: 99 },
      route: 'performance',
      sinceSample: 1,
    };

    it('says explicitly that nothing is wrong', async () => {
      // The empty state is the widget's most common and most valuable output:
      // it redirects the user from guessing at hardware to a specific app.
      await renderDashboard({ layout: [{ id: 'alerts', size: 'full' }] });
      expect(screen.getByText('Nothing needs your attention.')).toBeTruthy();
    });

    it('explains the cause rather than restating the number', async () => {
      await renderDashboard({
        layout: [{ id: 'alerts', size: 'full' }],
        alerts: [overheating],
      });

      expect(screen.getByText('The processor is overheating')).toBeTruthy();
      expect(screen.getByText(/protects itself by slowing down/)).toBeTruthy();
    });

    it('links an alert to the section that explains it', async () => {
      const onNavigate = vi.fn();
      await renderDashboard({
        layout: [{ id: 'alerts', size: 'full' }],
        alerts: [overheating],
        onNavigate,
      });

      fireEvent.click(screen.getByRole('button', { name: 'Investigate' }));
      expect(onNavigate).toHaveBeenCalledWith('performance');
    });
  });

  it('groups processes by name in the top lists', async () => {
    // A top five that is five rows of chrome.exe answers nothing.
    const source = createManualSystemSource();
    source.push({
      system: makeSystem(),
      processes: makeProcessMap([
        makeProcess({ name: 'chrome.exe', cpu: 10 }),
        makeProcess({ name: 'chrome.exe', cpu: 15 }),
      ]),
    });

    await renderDashboard({ layout: [{ id: 'topCpu', size: 'half' }], source });

    const widget = screen.getByRole('region', { name: 'Top by CPU' });
    expect(within(widget).getByText('chrome.exe')).toBeTruthy();
    expect(within(widget).getByText('2 processes')).toBeTruthy();
  });

  it('sends the user to Processes from a top list', async () => {
    const onNavigate = vi.fn();
    await renderDashboard({ layout: [{ id: 'topCpu', size: 'half' }], onNavigate });

    fireEvent.click(screen.getByRole('button', { name: 'View all processes' }));
    expect(onNavigate).toHaveBeenCalledWith('processes');
  });

  describe('unmeasurable readings', () => {
    it('omits a whole widget the machine has no sensor for', async () => {
      // Most desktops report no temperature at all without a ring-0 driver.
      // A permanently blank Thermals card would be worse than its absence, so
      // the capability check removes it before it can render.
      await renderDashboard({
        layout: [{ id: 'thermals', size: 'half' }],
        source: liveSource({ cpu: { temperature: null }, gpus: [{ id: 0, temperature: null }] }),
      });

      expect(screen.queryByRole('region', { name: 'Thermals' })).toBeNull();
    });

    it('says a missing sub-reading is unavailable rather than showing zero', async () => {
      // The widget IS supported here — the adapter exists and reports
      // utilisation — but it cannot report video memory. Zero bytes and "we
      // cannot read it" are different facts, and conflating them is worse
      // than omitting the figure, because the user believes the number.
      await renderDashboard({
        layout: [{ id: 'gpu', size: 'half' }],
        source: liveSource({
          gpus: [{ id: 0, name: 'Intel UHD Graphics', memoryUsed: null, memoryTotal: null }],
        }),
      });

      const widget = screen.getByRole('region', { name: 'GPU' });
      expect(within(widget).getByText('Not reported by your hardware')).toBeTruthy();
    });
  });

  it('renders in Romanian without falling back to key paths', async () => {
    await i18n.changeLanguage('ro');
    await renderDashboardRomanian();

    expect(screen.getByRole('heading', { name: 'Panou' })).toBeTruthy();
    expect(screen.getByRole('region', { name: 'Procesor' })).toBeTruthy();
  });
});

async function renderDashboardRomanian() {
  render(
    <DashboardScreen
      source={liveSource()}
      layoutBackend={memoryBackend([{ id: 'cpu', size: 'half' }])}
      historyCollector={new HistoryCollector()}
    />,
  );
  await screen.findByRole('heading', { name: 'Panou' });
}
