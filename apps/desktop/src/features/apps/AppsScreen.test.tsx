import { fireEvent, render, screen, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { AppsScreen } from './AppsScreen';
import type { AppsSnapshot, InstalledApp } from './model';
import { registerAppsStrings } from './strings';
import type { Uninstaller } from './useApps';

beforeAll(async () => {
  await initI18n();
  registerAppsStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

function app(overrides: Partial<InstalledApp> = {}): InstalledApp {
  return {
    keyName: `key-${overrides.name ?? 'Thing'}`,
    name: 'Thing',
    publisher: 'Vendor',
    version: '1.0.0',
    installDate: '2024-03-07',
    installLocation: null,
    estimatedSize: 1024 * 1024,
    uninstallString: '"C:\\Thing\\unins.exe"',
    quietUninstallString: null,
    isMsi: false,
    perUser: false,
    source: 'machineNative',
    ...overrides,
  };
}

function snapshot(overrides: Partial<AppsSnapshot> = {}): AppsSnapshot {
  return {
    apps: [app()],
    examined: 100,
    rejected: 0,
    rejectedByReason: [],
    duplicatesCollapsed: 0,
    ...overrides,
  };
}

async function mount(data = snapshot(), uninstall = vi.fn<Uninstaller>()) {
  uninstall.mockResolvedValue(undefined);
  const reader = vi.fn<() => Promise<AppsSnapshot>>().mockResolvedValue(data);
  render(<AppsScreen reader={reader} uninstall={uninstall} />);
  await screen.findByRole('heading', { name: 'Installed apps' });
  return { reader, uninstall };
}

describe('AppsScreen', () => {
  it('shows skeletons until the read settles', () => {
    render(<AppsScreen reader={() => new Promise(() => undefined)} />);
    expect(document.querySelector('[aria-busy="true"]')).toBeTruthy();
  });

  it('reaches a terminal state when the read fails', async () => {
    const reader = vi
      .fn<() => Promise<AppsSnapshot>>()
      .mockRejectedValue(new Error('registry read failed'));

    render(<AppsScreen reader={reader} />);

    expect(await screen.findByRole('alert')).toBeTruthy();
    expect(document.querySelector('[aria-busy="true"]')).toBeNull();
  });

  it('lists applications', async () => {
    await mount(snapshot({ apps: [app({ name: 'Steam' }), app({ name: 'Blender' })] }));

    expect(screen.getByText('Steam')).toBeTruthy();
    expect(screen.getByText('Blender')).toBeTruthy();
  });

  describe('honesty about sizes', () => {
    it('says a size was not reported rather than showing zero', async () => {
      await mount(snapshot({ apps: [app({ estimatedSize: null })] }));
      expect(screen.getByText('Not reported')).toBeTruthy();
    });

    it('explains that sizes come from the installer, not from disk', async () => {
      // The figure is frequently wrong: Windows never recomputes it, so a
      // program that has since downloaded 40 GB still reports its original.
      await mount(snapshot({ apps: [app(), app({ name: 'Silent', estimatedSize: null })] }));

      expect(screen.getByText(/did not report a size/)).toBeTruthy();
      expect(screen.getByText(/Windows never updates them/)).toBeTruthy();
    });

    it('stays quiet when every app declared a size', async () => {
      await mount();
      expect(screen.queryByText(/did not report a size/)).toBeNull();
    });
  });

  describe('the filtered-out count', () => {
    it('breaks the rejections down by reason', async () => {
      // Without the breakdown a well-filtered scan and a broken one look
      // identical — both produce a short list. "900 Windows components" is the
      // filter working; "900 with no name" would mean the scan failed.
      await mount(
        snapshot({
          examined: 1000,
          rejected: 900,
          rejectedByReason: [
            { reason: 'systemComponent', count: 800 },
            { reason: 'updateOrHotfix', count: 100 },
          ],
        }),
      );

      expect(screen.getByText(/900 of 1000 registry entries were not applications/)).toBeTruthy();
      expect(screen.getByText(/800 Windows components/)).toBeTruthy();
      expect(screen.getByText(/100 updates and hotfixes/)).toBeTruthy();
    });

    it('says nothing when nothing was filtered', async () => {
      await mount();
      expect(screen.queryByText(/were not applications/)).toBeNull();
    });
  });

  describe('sorting', () => {
    it('sorts by size, largest first, with unknowns last', async () => {
      await mount(
        snapshot({
          apps: [
            app({ name: 'Unknown', estimatedSize: null }),
            app({ name: 'Tiny', estimatedSize: 10 }),
            app({ name: 'Huge', estimatedSize: 10_000_000_000 }),
          ],
        }),
      );

      fireEvent.click(screen.getByRole('radio', { name: 'Size' }));

      const rows = screen.getAllByRole('row').slice(1);
      const names = rows.map((row) => within(row).getAllByRole('cell')[0]?.textContent ?? '');
      expect(names[0]).toContain('Huge');
      expect(names[2]).toContain('Unknown');
    });
  });

  it('searches by publisher', async () => {
    await mount(
      snapshot({
        apps: [
          app({ name: 'Steam', publisher: 'Valve' }),
          app({ name: 'Blender', publisher: 'BF' }),
        ],
      }),
    );

    fireEvent.change(screen.getByRole('searchbox'), { target: { value: 'valve' } });

    expect(screen.getByText('Steam')).toBeTruthy();
    expect(screen.queryByText('Blender')).toBeNull();
  });

  describe('uninstall', () => {
    it('disables the button when no uninstaller was published', async () => {
      await mount(snapshot({ apps: [app({ uninstallString: null })] }));

      const button = screen.getByRole('button', { name: 'Uninstall' });
      expect(button.hasAttribute('disabled')).toBe(true);
    });

    it('asks before doing anything', async () => {
      // Irreversible, and the user may have misclicked a row.
      const { uninstall } = await mount();

      fireEvent.click(screen.getByRole('button', { name: 'Uninstall' }));

      expect(screen.getByText(/Uninstall Thing\?/)).toBeTruthy();
      expect(uninstall).not.toHaveBeenCalled();
    });

    it('states that Vitals itself deletes nothing', async () => {
      // "Uninstall" in a third-party tool reasonably makes people wonder what
      // is doing the removing. Guessing which files belong to a product is how
      // data gets lost, so the dialog says plainly that we do not.
      await mount();
      fireEvent.click(screen.getByRole('button', { name: 'Uninstall' }));

      expect(screen.getByText(/never deletes files itself/)).toBeTruthy();
    });

    it('does nothing when cancelled', async () => {
      const { uninstall } = await mount();

      fireEvent.click(screen.getByRole('button', { name: 'Uninstall' }));
      fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));

      expect(uninstall).not.toHaveBeenCalled();
    });

    it('gives the dismiss control a name distinct from Cancel', async () => {
      // Two controls in one dialog sharing an accessible name reads as
      // "Cancel, button. Cancel, button." to a screen reader, with no way to
      // tell which is which. Found by `getByRole` refusing an ambiguous match.
      await mount();
      fireEvent.click(screen.getByRole('button', { name: 'Uninstall' }));

      expect(screen.getAllByRole('button', { name: 'Cancel' })).toHaveLength(1);
      expect(screen.getByRole('button', { name: 'Close this dialog' })).toBeTruthy();
    });

    it("asks the backend to launch the named entry's own uninstaller on confirmation", async () => {
      const { uninstall } = await mount(
        snapshot({ apps: [app({ uninstallString: '"C:\\Thing\\unins.exe" /silent' })] }),
      );

      fireEvent.click(screen.getByRole('button', { name: 'Uninstall' }));
      fireEvent.click(screen.getByRole('button', { name: 'Open uninstaller' }));

      // The identity, not the command line: the backend re-reads the command
      // from the registry, so the webview cannot ask it to run anything else.
      expect(uninstall).toHaveBeenCalledWith(
        expect.objectContaining({ keyName: 'key-Thing', source: 'machineNative' }),
      );
    });

    it('reports a failure to start the uninstaller', async () => {
      const uninstall = vi.fn<Uninstaller>().mockRejectedValue(new Error('access denied'));
      const reader = vi.fn<() => Promise<AppsSnapshot>>().mockResolvedValue(snapshot());
      render(<AppsScreen reader={reader} uninstall={uninstall} />);
      await screen.findByRole('heading', { name: 'Installed apps' });

      fireEvent.click(screen.getByRole('button', { name: 'Uninstall' }));
      fireEvent.click(screen.getByRole('button', { name: 'Open uninstaller' }));

      expect(await screen.findByText(/Could not start the uninstaller/)).toBeTruthy();
    });
  });

  it('renders in Romanian without falling back to key paths', async () => {
    await i18n.changeLanguage('ro');
    const reader = vi.fn<() => Promise<AppsSnapshot>>().mockResolvedValue(snapshot());
    render(<AppsScreen reader={reader} />);

    expect(await screen.findByRole('heading', { name: 'Aplicații instalate' })).toBeTruthy();
    expect(screen.getByRole('radio', { name: 'Dimensiune' })).toBeTruthy();
  });
});
