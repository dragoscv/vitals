import { describe, expect, it } from 'vitest';

import {
  availableIds,
  BACKGROUND_LOAD_LIMIT,
  benchmarkIds,
  byGroup,
  distrustReasons,
  estimatedSeconds,
  groupOf,
  isTrustworthy,
  medianOf,
  spreadOf,
  VARIABILITY_LIMIT,
  type BenchmarkId,
  type BenchmarkInfo,
  type BenchmarkResultDto,
  type RunConditionsDto,
} from './model';

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

function info(overrides: Partial<BenchmarkInfo> = {}): BenchmarkInfo {
  return {
    id: 'cpuSingleThread',
    available: true,
    unavailableReason: null,
    estimatedSeconds: 10,
    ...overrides,
  };
}

describe('groupOf', () => {
  it('assigns every id to a group', () => {
    // Derived from the id prefix, so a new backend id must land somewhere
    // rather than silently disappearing from the selection list.
    for (const id of benchmarkIds) {
      expect(['cpu', 'memory', 'disk', 'gpu']).toContain(groupOf(id));
    }
  });

  it('reads the prefix, not the position', () => {
    expect(groupOf('memoryLatency')).toBe('memory');
    expect(groupOf('diskRandomWrite')).toBe('disk');
    expect(groupOf('gpuRender')).toBe('gpu');
  });
});

describe('medianOf', () => {
  it('takes the middle of an odd number of runs', () => {
    expect(medianOf(result({ runs: [10, 30, 20] }))).toBe(20);
  });

  it('averages the two middles of an even number of runs', () => {
    expect(medianOf(result({ runs: [10, 20, 30, 40] }))).toBe(25);
  });

  it('ignores a single catastrophic outlier, which the mean would not', () => {
    // The whole reason the median is the headline: interference can only make
    // a pass slower, so one stalled run drags an average down permanently
    // while leaving the median untouched.
    const runs = [1000, 1010, 990, 1005, 12];
    expect(medianOf(result({ runs }))).toBe(1000);
    const mean = runs.reduce((a, b) => a + b, 0) / runs.length;
    expect(mean).toBeLessThan(900);
  });

  it('falls back to the reported score rather than inventing zero', () => {
    // Zero is a claim about the hardware. "No passes to summarise" is not.
    expect(medianOf(result({ runs: [], score: 777 }))).toBe(777);
  });
});

describe('spreadOf', () => {
  it('reports the best and worst pass', () => {
    expect(spreadOf(result({ runs: [5, 9, 7] }))).toEqual({ min: 5, max: 9 });
  });

  it('reports nothing when there is only one pass to compare', () => {
    expect(spreadOf(result({ runs: [5] }))).toBeNull();
  });
});

describe('distrustReasons', () => {
  it('finds nothing wrong with a clean run', () => {
    expect(distrustReasons(result())).toEqual([]);
    expect(isTrustworthy(result())).toBe(true);
  });

  it('names thermal throttling', () => {
    expect(distrustReasons(result({ conditions: conditions({ throttled: true }) }))).toContain(
      'throttled',
    );
  });

  it('names battery power, which halves performance on most laptops', () => {
    expect(distrustReasons(result({ conditions: conditions({ onBattery: true }) }))).toContain(
      'onBattery',
    );
  });

  it('names background load above the limit', () => {
    expect(distrustReasons(result({ conditions: conditions({ backgroundLoad: 40 }) }))).toContain(
      'backgroundLoad',
    );
  });

  it('tolerates the small background load every real machine has', () => {
    expect(distrustReasons(result({ conditions: conditions({ backgroundLoad: 3 }) }))).toEqual([]);
  });

  it('names an excessive spread between runs', () => {
    expect(distrustReasons(result({ variability: 40 }))).toContain('variability');
  });

  it('reads variability and background load as percentages, not fractions', () => {
    // Both fields arrive already scaled to 0..100: `BenchmarkResult::
    // variability` in `vitals-bench` multiplies by 100, and `background_load`
    // is a CPU percentage. An earlier draft of this screen treated them as
    // 0..1 fractions, which made every threshold fire a hundred times too
    // late — a 42% spread read as 0.42% and was published as trustworthy,
    // which is precisely the lie this screen exists to prevent.
    expect(VARIABILITY_LIMIT).toBe(5);
    expect(BACKGROUND_LOAD_LIMIT).toBe(10);

    // 6% must trip the limit; on the fraction reading it would not.
    expect(distrustReasons(result({ variability: 6 }))).toContain('variability');
    expect(distrustReasons(result({ conditions: conditions({ backgroundLoad: 12 }) }))).toContain(
      'backgroundLoad',
    );

    // And a genuinely quiet run must still come back clean.
    expect(distrustReasons(result({ variability: 4 }))).not.toContain('variability');
  });

  it('does not treat a missing variability as a bad one', () => {
    // Null means one run, not a wild one. Reporting it as a distrust reason
    // would flag every single-pass benchmark as unreliable on no evidence.
    expect(distrustReasons(result({ variability: null, runs: [10] }))).toEqual([]);
  });

  it('falls back to the backend taint only when nothing specific applies', () => {
    // "Conditions were bad" is the least useful sentence available, so it is
    // never shown alongside a reason that actually names the problem.
    const vague = result({ conditions: conditions({ tainted: true }) });
    expect(distrustReasons(vague)).toEqual(['tainted']);

    const specific = result({ conditions: conditions({ tainted: true, throttled: true }) });
    expect(distrustReasons(specific)).toEqual(['throttled']);
  });

  it('distrusts a result the backend already flagged, whatever the conditions say', () => {
    expect(isTrustworthy(result({ trustworthy: false }))).toBe(false);
  });
});

describe('availableIds', () => {
  it('returns only what the backend says it can run', () => {
    const ids = availableIds([
      info({ id: 'cpuSingleThread' }),
      info({ id: 'gpuRender', available: false, unavailableReason: 'needsGraphicsContext' }),
    ]);
    expect(ids).toEqual<BenchmarkId[]>(['cpuSingleThread']);
  });
});

describe('estimatedSeconds', () => {
  it('adds up only the selected, available benchmarks', () => {
    const infos = [
      info({ id: 'cpuSingleThread', estimatedSeconds: 10 }),
      info({ id: 'cpuMultiThread', estimatedSeconds: 20 }),
      info({ id: 'diskRandomRead', available: false, estimatedSeconds: 60 }),
    ];
    const selected = new Set<BenchmarkId>(['cpuSingleThread', 'cpuMultiThread', 'diskRandomRead']);

    // The unavailable one contributes nothing: promising a minute of work
    // that cannot run would make the estimate the user plans around wrong.
    expect(estimatedSeconds(infos, selected)).toBe(30);
  });

  it('is zero when nothing is selected', () => {
    expect(estimatedSeconds([info()], new Set())).toBe(0);
  });
});

describe('byGroup', () => {
  it('keeps a fixed section order regardless of arrival order', () => {
    const sections = byGroup([
      info({ id: 'gpuRender' }),
      info({ id: 'cpuSingleThread' }),
      info({ id: 'memoryLatency' }),
    ]);
    expect(sections.map((section) => section.group)).toEqual(['cpu', 'memory', 'gpu']);
  });

  it('omits a group with no benchmarks rather than rendering an empty card', () => {
    const sections = byGroup([info({ id: 'cpuSingleThread' })]);
    expect(sections).toHaveLength(1);
  });
});
