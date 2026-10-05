import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';

import { i18n, initI18n } from '@vitals/i18n';

import { BenchmarksScreen } from './BenchmarksScreen';
import type {
  BenchmarkId,
  BenchmarkInfo,
  BenchmarkResultDto,
  BenchmarkSuiteDto,
  RunConditionsDto,
} from './model';
import { registerBenchmarksStrings } from './strings';
import type { BenchmarksSource } from './useBenchmarks';

beforeAll(async () => {
  await initI18n();
  registerBenchmarksStrings();
});

beforeEach(async () => {
  await i18n.changeLanguage('en');
});

const IMPLEMENTED: readonly BenchmarkId[] = [
  'cpuSingleThread',
  'cpuMultiThread',
  'memoryBandwidth',
  'memoryLatency',
];

function info(overrides: Partial<BenchmarkInfo> = {}): BenchmarkInfo {
  return {
    id: 'cpuSingleThread',
    available: true,
    unavailableReason: null,
    estimatedSeconds: 12,
    ...overrides,
  };
}

/** The listing the real backend returns today: four real, six placeholders. */
function listing(): readonly BenchmarkInfo[] {
  return [
    ...IMPLEMENTED.map((id) => info({ id })),
    info({
      id: 'diskSequentialRead',
      available: false,
      unavailableReason: 'needsConsent',
      estimatedSeconds: 30,
    }),
    info({
      id: 'gpuRender',
      available: false,
      unavailableReason: 'needsGraphicsContext',
      estimatedSeconds: 20,
    }),
  ];
}

function conditions(overrides: Partial<RunConditionsDto> = {}): RunConditionsDto {
  return {
    powerPlan: 'Balanced',
    onBattery: false,
    throttled: false,
    backgroundLoad: 2,
    ambientStartTemp: 41,
    ambientEndTemp: 58,
    tainted: false,
    ...overrides,
  };
}

function result(overrides: Partial<BenchmarkResultDto> = {}): BenchmarkResultDto {
  return {
    id: 'cpuSingleThread',
    score: 1000,
    runs: [990, 1000, 1010],
    unit: 'ops/s',
    durationMs: 12_000,
    variability: 1,
    trustworthy: true,
    conditions: conditions(),
    timestampMs: 1_700_000_000_000,
    ...overrides,
  };
}

function suite(results: readonly BenchmarkResultDto[] = [result()]): BenchmarkSuiteDto {
  return { results, totalDurationMs: 48_000 };
}

function makeSource(overrides: Partial<BenchmarksSource> = {}): BenchmarksSource {
  return {
    list: vi.fn<BenchmarksSource['list']>().mockResolvedValue(listing()),
    run: vi.fn<BenchmarksSource['run']>().mockResolvedValue(suite()),
    ...overrides,
  };
}

async function mount(source = makeSource(), title = 'Benchmarks') {
  render(<BenchmarksScreen source={source} />);
  await screen.findByRole('heading', { name: title, level: 2 });
  return source;
}

describe('BenchmarksScreen', () => {
  it('shows skeletons until the listing settles', () => {
    render(<BenchmarksScreen source={makeSource({ list: () => new Promise(() => undefined) })} />);
    expect(document.querySelector('[aria-busy="true"]')).toBeTruthy();
  });

  it('reaches a terminal state when the listing fails', async () => {
    await mount(makeSource({ list: vi.fn().mockRejectedValue(new Error('no backend')) }));

    expect(await screen.findByRole('alert')).toBeTruthy();
    expect(document.querySelector('[aria-busy="true"]')).toBeNull();
  });

  describe('benchmarks that cannot run', () => {
    it('shows them rather than hiding them', async () => {
      // Hiding them makes the feature look complete and leaves the user
      // wondering why their disk is never measured.
      await mount();

      expect(screen.getByText('Disk — sequential read')).toBeTruthy();
      expect(screen.getByText('GPU — render')).toBeTruthy();
    });

    it('disables the control instead of letting it fail on click', async () => {
      await mount();

      const box = screen.getByRole('checkbox', { name: 'Disk — sequential read' });
      expect(box.getAttribute('data-disabled')).not.toBeNull();
    });

    it('states the reason, in words, next to the control', async () => {
      await mount();

      expect(screen.getByText(/needs your permission first/)).toBeTruthy();
      expect(screen.getByText(/needs a graphics context/)).toBeTruthy();
    });

    it('exposes that reason to assistive technology, not just visually', async () => {
      // A disabled control whose explanation lives only in adjacent grey text
      // is unexplained for anyone not reading the layout.
      await mount();

      const box = screen.getByRole('checkbox', { name: 'GPU — render' });
      const describedBy = box.getAttribute('aria-describedby');
      expect(describedBy).toBe('gpuRender-reason');
      expect(document.getElementById(describedBy ?? '')?.textContent).toMatch(/graphics context/);
    });

    it('never includes them in a run', async () => {
      const source = await mount();
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      await waitFor(() => {
        expect(source.run).toHaveBeenCalled();
      });
      const ids = vi.mocked(source.run).mock.calls[0]?.[0] ?? [];
      expect([...ids].sort()).toEqual([...IMPLEMENTED].sort());
    });
  });

  describe('before starting', () => {
    it('says how long the selected set will take', async () => {
      // Four implemented benchmarks at 12 s each.
      await mount();
      expect(screen.getByText(/about 48 seconds/)).toBeTruthy();
    });

    it('says the machine becomes unresponsive', async () => {
      await mount();
      expect(screen.getByText(/make your computer unresponsive/)).toBeTruthy();
      expect(screen.getByText(/do not use the computer until it finishes/i)).toBeTruthy();
    });

    it('refuses to start with nothing selected', async () => {
      const source = await mount();
      fireEvent.click(screen.getByRole('button', { name: 'Clear the selection' }));

      expect(screen.getByText('Select at least one benchmark to begin.')).toBeTruthy();
      expect(screen.getByRole('button', { name: 'Run benchmarks' }).hasAttribute('disabled')).toBe(
        true,
      );
      expect(source.run).not.toHaveBeenCalled();
    });

    it('recomputes the estimate as the selection changes', async () => {
      await mount();
      fireEvent.click(screen.getByRole('checkbox', { name: 'CPU — all threads' }));

      expect(screen.getByText(/about 36 seconds/)).toBeTruthy();
    });
  });

  describe('while running', () => {
    it('names the active benchmark and warns the numbers are not final', async () => {
      await mount(makeSource({ run: () => new Promise(() => undefined) }));
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      const status = await screen.findByRole('status', { name: 'Benchmark progress' });
      expect(status.textContent).toMatch(/Measuring CPU — single thread/);
      expect(status.textContent).toMatch(/not final/);
    });

    it('announces politely, never assertively', async () => {
      // An assertive region interrupts whatever the user is reading. A minute
      // of progress updates doing that makes the app unusable with a reader.
      await mount(makeSource({ run: () => new Promise(() => undefined) }));
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      const status = await screen.findByRole('status', { name: 'Benchmark progress' });
      expect(status.getAttribute('aria-live')).not.toBe('assertive');
    });

    it('does not start a second run on top of the first', async () => {
      const source = await mount(
        makeSource({
          run: vi.fn<BenchmarksSource['run']>(() => new Promise(() => undefined)),
        }),
      );
      const button = screen.getByRole('button', { name: 'Run benchmarks' });

      fireEvent.click(button);
      await screen.findByRole('status', { name: 'Benchmark progress' });
      fireEvent.click(button);

      expect(source.run).toHaveBeenCalledTimes(1);
    });
  });

  describe('results', () => {
    it('formats the score with the unit the backend supplied', async () => {
      // MB/s for bandwidth, ns for latency, ops/s for CPU — a hardcoded unit
      // would mislabel two thirds of the results.
      await mount(
        makeSource({
          run: vi
            .fn()
            .mockResolvedValue(
              suite([
                result({ id: 'memoryBandwidth', runs: [40_000], unit: 'MB/s', score: 40_000 }),
              ]),
            ),
        }),
      );
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      // The headline median and the sole run print the same string, so both
      // occurrences are expected — this asserts the unit, not the count.
      expect((await screen.findAllByText('40,000 MB/s')).length).toBeGreaterThan(0);
    });

    it('shows every individual run, not just the summary', async () => {
      // 100 and 900 have the same median as 490 and 510, and only one of
      // those pairs is a measurement.
      await mount(
        makeSource({ run: vi.fn().mockResolvedValue(suite([result({ runs: [100, 500, 900] })])) }),
      );
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      expect(await screen.findByText('100 ops/s')).toBeTruthy();
      // 500 is both the median and a run, hence two nodes.
      expect(screen.getAllByText('500 ops/s')).toHaveLength(2);
      expect(screen.getByText('900 ops/s')).toBeTruthy();
    });

    it('reports the spread as a real figure', async () => {
      await mount(
        makeSource({
          run: vi.fn().mockResolvedValue(suite([result({ runs: [900, 1000, 1100] })])),
        }),
      );
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      expect(await screen.findByText(/Best 1,100 ops\/s, worst 900 ops\/s/)).toBeTruthy();
      expect(screen.getByText(/Spread between runs: 1.0%/)).toBeTruthy();
    });

    it('marks a clean run as clean', async () => {
      await mount();
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      expect(await screen.findByText('Conditions were clean')).toBeTruthy();
    });

    it('always shows the conditions, including on a trustworthy result', async () => {
      // A benchmark that only explains itself when it failed is asking the
      // user to trust silence.
      await mount();
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      expect(await screen.findByText('Power plan: Balanced')).toBeTruthy();
      expect(screen.getByText(/41°C at the start, 58°C at the end/)).toBeTruthy();
    });

    it('says a temperature was not reported rather than printing zero', async () => {
      await mount(
        makeSource({
          run: vi.fn().mockResolvedValue(
            suite([
              result({
                conditions: conditions({ ambientStartTemp: null, ambientEndTemp: null }),
              }),
            ]),
          ),
        }),
      );
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      expect(await screen.findByText('Temperature: not reported')).toBeTruthy();
    });
  });

  describe('untrustworthy results', () => {
    it('is visually distinct and says which condition spoiled it', async () => {
      await mount(
        makeSource({
          run: vi.fn().mockResolvedValue(
            suite([
              result({
                trustworthy: false,
                conditions: conditions({ throttled: true, onBattery: true }),
              }),
            ]),
          ),
        }),
      );
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      expect(await screen.findByText('Do not trust this figure')).toBeTruthy();
      expect(screen.getByText(/thermally throttled/)).toBeTruthy();
      expect(screen.getByText(/on battery/)).toBeTruthy();
    });

    it('quantifies a spread complaint rather than asserting it', async () => {
      await mount(
        makeSource({
          run: vi
            .fn()
            .mockResolvedValue(
              suite([result({ trustworthy: false, variability: 42, runs: [600, 1000, 1400] })]),
            ),
        }),
      );
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      expect(await screen.findByText(/disagreed with each other by 42.0%/)).toBeTruthy();
    });

    it('quantifies background interference', async () => {
      await mount(
        makeSource({
          run: vi
            .fn()
            .mockResolvedValue(
              suite([
                result({ trustworthy: false, conditions: conditions({ backgroundLoad: 35 }) }),
              ]),
            ),
        }),
      );
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      expect(await screen.findByText(/using 35.0% of the machine/)).toBeTruthy();
    });

    it('still shows the number, because the measurement is real', async () => {
      // Suppressing it would leave the user with nothing at all; the badge and
      // the reason are what make it provisional rather than authoritative.
      await mount(
        makeSource({
          run: vi
            .fn()
            .mockResolvedValue(suite([result({ trustworthy: false, runs: [1000, 1000, 1000] })])),
        }),
      );
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      expect((await screen.findAllByText('1,000 ops/s')).length).toBeGreaterThan(0);
    });
  });

  describe('failure', () => {
    it('ends the spinner and states the error when a run fails', async () => {
      await mount(makeSource({ run: vi.fn().mockRejectedValue(new Error('cpu probe died')) }));
      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));

      expect(await screen.findByText(/cpu probe died/)).toBeTruthy();
      expect(screen.queryByRole('status', { name: 'Benchmark progress' })).toBeNull();
      expect(screen.getByRole('button', { name: 'Run benchmarks' }).hasAttribute('disabled')).toBe(
        false,
      );
    });

    it('keeps the previous run on screen when a re-run fails', async () => {
      const run = vi
        .fn<BenchmarksSource['run']>()
        .mockResolvedValueOnce(suite())
        .mockRejectedValueOnce(new Error('second attempt failed'));
      await mount(makeSource({ run }));

      fireEvent.click(screen.getByRole('button', { name: 'Run benchmarks' }));
      const again = await screen.findByRole('button', { name: 'Run again' });
      fireEvent.click(again);

      expect(await screen.findByText(/last completed run/)).toBeTruthy();
      expect(screen.getAllByText('1,000 ops/s').length).toBeGreaterThan(0);
    });
  });

  it('offers nothing to read until a run has happened', async () => {
    await mount();
    expect(screen.getByText('Nothing measured yet')).toBeTruthy();
  });

  describe('Romanian', () => {
    it('renders real Romanian, not key paths', async () => {
      await i18n.changeLanguage('ro');
      await mount(
        makeSource({
          run: vi.fn().mockResolvedValue(suite([result({ trustworthy: false, variability: 42 })])),
        }),
        'Teste de performanță',
      );

      expect(screen.getByText('Procesor — un singur fir')).toBeTruthy();

      fireEvent.click(screen.getByRole('button', { name: 'Rulează testele' }));
      expect(await screen.findByText('Nu te baza pe această cifră')).toBeTruthy();
      expect(document.body.textContent).not.toMatch(/benchmarks\.[a-z]/i);
      expect(document.body.textContent).not.toMatch(/trust\.reason\./);
    });

    it('uses the 20+ plural form, which takes "de"', async () => {
      // Romanian counts in three: 1, 2..19, and 20+. Two forms is the standard
      // machine-translation tell and reads wrong at exactly the durations a
      // benchmark suite produces.
      await i18n.changeLanguage('ro');
      await mount(makeSource(), 'Teste de performanță');

      expect(screen.getByText(/48 de secunde/)).toBeTruthy();
    });
  });
});

describe('the benchmark row menu', () => {
  const openMenu = async (target: HTMLElement): Promise<HTMLElement> => {
    fireEvent.contextMenu(target, { button: 2, clientX: 5, clientY: 5 });
    await act(async () => {});
    return screen.findByRole('menu');
  };

  it('opens on a right click with select, run only, copy result and refresh', async () => {
    await mount();

    const menu = await openMenu(screen.getByTestId('choice-cpuMultiThread'));

    expect(
      within(menu)
        .getAllByRole('menuitem')
        .map((item) => item.textContent),
    ).toEqual(['Leave out of the run', 'Run only this', 'Copy result', 'Refresh the list']);
    // Nothing has run yet, so there is no result to copy.
    expect(
      within(menu)
        .getByText('Copy result')
        .closest('[role="menuitem"]')
        ?.getAttribute('aria-disabled'),
    ).toBe('true');
  });

  it('does not open on a left-button contextmenu', async () => {
    await mount();

    fireEvent.contextMenu(screen.getByTestId('choice-cpuMultiThread'), {
      button: 0,
      clientX: 0,
      clientY: 0,
    });
    await act(async () => {});

    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('runs exactly the one test when run only this is chosen', async () => {
    const source = await mount();

    fireEvent.click(
      within(await openMenu(screen.getByTestId('choice-memoryLatency'))).getByText('Run only this'),
    );

    await waitFor(() => {
      expect(source.run).toHaveBeenCalledWith(['memoryLatency']);
    });
  });

  it('deselects a test from the menu, and the checkbox follows', async () => {
    await mount();

    fireEvent.click(
      within(await openMenu(screen.getByTestId('choice-cpuMultiThread'))).getByText(
        'Leave out of the run',
      ),
    );

    expect(
      screen.getByRole('checkbox', { name: 'CPU — all threads' }).getAttribute('aria-checked'),
    ).toBe('false');
  });

  it('offers nothing but refresh for a test the backend cannot run', async () => {
    await mount();

    const menu = await openMenu(screen.getByTestId('choice-gpuRender'));
    const disabled = within(menu)
      .getAllByRole('menuitem')
      .filter((item) => item.getAttribute('aria-disabled') === 'true')
      .map((item) => item.textContent);

    expect(disabled).toEqual(['Include in the run', 'Run only this', 'Copy result']);
  });
});
