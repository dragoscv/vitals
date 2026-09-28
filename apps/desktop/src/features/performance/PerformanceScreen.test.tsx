import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { resetSettingsForTests, useSettings } from '../../settings/store';
import { HistoryCollector } from '../dashboard/history';
import { makeSystem, type SystemOverrides } from '../dashboard/test-fixtures';
import { createManualSystemSource, NO_SAMPLER } from '../dashboard/useSystemSnapshot';
import { PerformanceScreen } from './PerformanceScreen';
import { registerPerformanceStrings } from './strings';

beforeAll(async () => {
  await initI18n();
  registerPerformanceStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
  resetSettingsForTests();
});

function mount(overrides: SystemOverrides = {}) {
  const source = createManualSystemSource();
  const collector = new HistoryCollector();
  const system = makeSystem(overrides);

  source.push({ system, processes: new Map() });
  // A few frames so the charts have something to draw and the panels are not
  // exercising only their empty paths.
  for (let seq = 1; seq <= 3; seq += 1) {
    collector.push({ system, processes: new Map(), seq, elapsedMs: 1000, timestampMs: seq * 1000 });
  }

  render(<PerformanceScreen source={source} historyCollector={collector} />);
  return { source, collector };
}

describe('PerformanceScreen', () => {
  it('shows skeletons before the first frame', () => {
    render(
      <PerformanceScreen
        source={createManualSystemSource()}
        historyCollector={new HistoryCollector()}
      />,
    );

    expect(document.querySelector('[aria-busy="true"]')).toBeTruthy();
  });

  it('explains a dead sampler instead of waiting forever', () => {
    // Same rule as everywhere else in this app: a loading state must resolve
    // either way, or a broken app looks merely busy.
    const source = createManualSystemSource();
    source.push({ pending: false, system: null, error: NO_SAMPLER });

    render(<PerformanceScreen source={source} historyCollector={new HistoryCollector()} />);

    expect(screen.getByText('No readings are arriving')).toBeTruthy();
    expect(document.querySelector('[aria-busy="true"]')).toBeNull();
  });

  it('lists every resource in the rail', () => {
    mount({ gpus: [{ id: 0, name: 'RTX 3060 Ti' }], disks: [{ id: 0, mount: 'C:' }] });

    const rail = screen.getByRole('navigation', { name: 'Resources' });
    expect(within(rail).getByRole('button', { name: /CPU/ })).toBeTruthy();
    expect(within(rail).getByRole('button', { name: /Memory/ })).toBeTruthy();
    expect(within(rail).getByRole('button', { name: /RTX 3060 Ti/ })).toBeTruthy();
    expect(within(rail).getByRole('button', { name: /C:/ })).toBeTruthy();
  });

  it('opens on CPU', () => {
    mount();
    expect(screen.getByText('Logical processors (4)')).toBeTruthy();
  });

  it('switches panels when a resource is chosen', () => {
    mount();

    const rail = screen.getByRole('navigation', { name: 'Resources' });
    fireEvent.click(within(rail).getByRole('button', { name: /Memory/ }));

    // A memory-only figure, to prove the panel actually changed.
    expect(screen.getByText('Composition')).toBeTruthy();
  });

  it('marks the selected resource for assistive technology', () => {
    mount();
    const rail = screen.getByRole('navigation', { name: 'Resources' });
    const cpu = within(rail).getByRole('button', { name: /CPU/ });

    expect(cpu.getAttribute('aria-current')).toBe('true');
  });

  it('hides virtual adapters until "show hidden" is ticked', () => {
    mount({
      networks: [
        { id: 0, name: 'Ethernet', kind: 'ethernet' },
        { id: 1, name: 'vEthernet (WSL)', kind: 'virtual' },
      ],
    });

    const rail = screen.getByRole('navigation', { name: 'Resources' });
    expect(within(rail).queryByRole('button', { name: /WSL/ })).toBeNull();

    fireEvent.click(screen.getByRole('checkbox', { name: /Show hidden devices \(1\)/ }));
    expect(within(rail).getByRole('button', { name: /WSL/ })).toBeTruthy();
  });

  it('hides a device from its context menu and remembers the choice', () => {
    mount({
      disks: [
        { id: 0, mount: 'C:' },
        { id: 7, mount: 'H:' },
      ],
    });

    const rail = screen.getByRole('navigation', { name: 'Resources' });
    fireEvent.contextMenu(within(rail).getByRole('button', { name: /H:/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Hide' }));

    expect(within(rail).queryByRole('button', { name: /H:/ })).toBeNull();
    expect(within(rail).getByRole('button', { name: /C:/ })).toBeTruthy();
    // Persisted by name, so it survives a restart and a renumbered id.
    expect(useSettings.getState().settings.resourceVisibility).toEqual({ 'disk:H:': 'hidden' });
  });

  it('shows a hidden device again from its context menu', () => {
    useSettings.getState().patch({
      resourceVisibility: { 'disk:H:': 'hidden' },
      showHiddenResources: true,
    });
    mount({ disks: [{ id: 7, mount: 'H:' }] });

    const rail = screen.getByRole('navigation', { name: 'Resources' });
    fireEvent.contextMenu(within(rail).getByRole('button', { name: /H:/ }));
    fireEvent.click(screen.getByRole('menuitem', { name: 'Show' }));

    expect(useSettings.getState().settings.resourceVisibility).toEqual({});
  });

  it('offers no menu on CPU, which cannot be hidden', () => {
    mount();

    const rail = screen.getByRole('navigation', { name: 'Resources' });
    fireEvent.contextMenu(within(rail).getByRole('button', { name: /CPU/ }));
    expect(screen.queryByRole('menuitem')).toBeNull();
  });

  it('charts the selected network adapter, not the machine total', () => {
    // Every adapter used to draw the same machine-wide line, so switching
    // between them looked like the page ignored the click.
    mount({
      networks: [
        { id: 16, name: 'Ethernet', kind: 'ethernet', rx: 5000 },
        { id: 14, name: 'Tailscale', kind: 'ethernet', rx: 30 },
      ],
    });

    const rail = screen.getByRole('navigation', { name: 'Resources' });
    fireEvent.click(within(rail).getByRole('button', { name: /Ethernet/ }));
    expect(screen.getByRole('img', { name: 'Ethernet' })).toBeTruthy();

    fireEvent.click(within(rail).getByRole('button', { name: /Tailscale/ }));
    expect(screen.getByRole('img', { name: 'Tailscale' })).toBeTruthy();
    expect(screen.queryByRole('img', { name: 'Ethernet' })).toBeNull();
  });

  it('offers no menu on thermals either', () => {
    mount({ cpu: { temperature: 55 } });

    const rail = screen.getByRole('navigation', { name: 'Resources' });
    fireEvent.contextMenu(within(rail).getByRole('button', { name: /Thermals/ }));
    expect(screen.queryByRole('menuitem')).toBeNull();
  });

  it('keeps rail buttons clear of the overlay scrollbar', () => {
    // The Radix bar is 10 px and overlays the viewport; the list must pad
    // that side by more, or the bar paints over the buttons' edge.
    mount();

    const rail = screen.getByRole('navigation', { name: 'Resources' });
    const list = within(rail).getAllByRole('listitem')[0]?.parentElement;
    expect(list?.className).toMatch(/\blg:pr-4\b/);
    expect(list?.className).toMatch(/\bpb-4\b/);
  });

  it('puts "show hidden" above the list, where a 43-entry rail cannot bury it', () => {
    mount();

    const rail = screen.getByRole('navigation', { name: 'Resources' });
    const checkbox = within(rail).getByRole('checkbox', { name: /Show hidden devices/ });
    const list = within(rail).getByRole('list');
    expect(checkbox.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it('explains hidden devices in a popover behind the (i), not inline', async () => {
    mount();
    const hint = /Right-click any device to hide or show it/;
    expect(screen.queryByText(hint)).toBeNull();

    fireEvent.click(screen.getByRole('button', { name: 'About hidden devices' }));
    expect(await screen.findByText(hint)).toBeTruthy();
  });

  describe('thermals', () => {
    it('is absent when the machine reports no temperature', () => {
      mount({
        cpu: { temperature: null },
        gpus: [{ id: 0, temperature: null, hotspotTemperature: null }],
        disks: [{ id: 0, temperature: null }],
      });

      const rail = screen.getByRole('navigation', { name: 'Resources' });
      expect(within(rail).queryByRole('button', { name: /Thermals/ })).toBeNull();
    });

    it('lists the readings and says what cannot be measured', () => {
      mount({ cpu: { temperature: 62 } });

      const rail = screen.getByRole('navigation', { name: 'Resources' });
      fireEvent.click(within(rail).getByRole('button', { name: /Thermals/ }));

      expect(screen.getByText('CPU package')).toBeTruthy();
      // The honesty section: the limitation is Vitals', not the machine's, and
      // saying so stops someone hunting for a hardware fault.
      expect(screen.getByText('What cannot be measured')).toBeTruthy();
      expect(screen.getByText('Per-core CPU temperature')).toBeTruthy();
    });

    it('separates GPU hotspot from core', () => {
      // Hotspot runs well above core and is what actually throttles, so
      // folding them into one reading would hide the number that matters.
      mount({ gpus: [{ id: 0, name: 'RTX', temperature: 60, hotspotTemperature: 88 }] });

      const rail = screen.getByRole('navigation', { name: 'Resources' });
      fireEvent.click(within(rail).getByRole('button', { name: /Thermals/ }));

      expect(screen.getByText('RTX core')).toBeTruthy();
      expect(screen.getByText('RTX hotspot')).toBeTruthy();
    });
  });

  describe('honesty about missing readings', () => {
    it('omits an optional row rather than rendering it blank', () => {
      // A machine reporting no SMART data would otherwise show a column of
      // empty rows that looks like the panel failed to load.
      mount({ disks: [{ id: 0, mount: 'C:', health: null, queueDepth: null }] });

      const rail = screen.getByRole('navigation', { name: 'Resources' });
      fireEvent.click(within(rail).getByRole('button', { name: /C:/ }));

      expect(screen.queryByText('Life remaining')).toBeNull();
      expect(screen.getByText('Active time')).toBeTruthy();
    });

    it('states unavailability for a headline figure', () => {
      // Dropping the row is right for optional detail; for a figure the panel
      // is *about*, silence would read as a rendering bug.
      mount({ memory: { swapUsed: null } });

      const rail = screen.getByRole('navigation', { name: 'Resources' });
      fireEvent.click(within(rail).getByRole('button', { name: /Memory/ }));

      expect(screen.getAllByText('Not reported by your hardware').length).toBeGreaterThan(0);
    });
  });

  it('renders in Romanian without falling back to key paths', async () => {
    await i18n.changeLanguage('ro');
    mount();

    expect(screen.getByRole('heading', { name: 'Performanță' })).toBeTruthy();
    const rail = screen.getByRole('navigation', { name: 'Resurse' });
    expect(within(rail).getByRole('button', { name: /Procesor/ })).toBeTruthy();
  });
});
