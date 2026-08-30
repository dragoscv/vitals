/**
 * Benchmarks: the data shape, and the rules that decide whether a score is
 * allowed to be believed.
 *
 * # A score without its conditions is worthless
 *
 * Every other benchmark on Windows prints one number. That number is a
 * function of the machine *and* of the minute it was taken in: a laptop that
 * dropped to 15 W on battery, a thermal-throttled desktop, or a run that
 * happened to coincide with a Windows Update install all produce a figure
 * that looks exactly as authoritative as a clean one. This module therefore
 * treats `trustworthy` as derived, not decorative — `distrustReasons` returns
 * the concrete list of what was wrong, and the screen is required to print
 * it next to the score rather than dimming the row and hoping.
 *
 * # The median is the headline, not the mean
 *
 * `runs` holds every individual pass. The headline uses the median because a
 * benchmark's error distribution is one-sided: interference can only ever
 * make a run slower, never faster. A single scheduling stall or a background
 * indexer waking up drags the mean down permanently, while the median simply
 * ignores it. The mean would be the right choice if noise were symmetric
 * around the true value, and here it is not. The spread is surfaced
 * separately so that discarding the outlier is visible rather than silent.
 */

export const benchmarkIds = [
  'cpuSingleThread',
  'cpuMultiThread',
  'memoryBandwidth',
  'memoryLatency',
  'diskSequentialRead',
  'diskSequentialWrite',
  'diskRandomRead',
  'diskRandomWrite',
  'gpuCompute',
  'gpuRender',
] as const;

export type BenchmarkId = (typeof benchmarkIds)[number];

export const benchmarkGroups = ['cpu', 'memory', 'disk', 'gpu'] as const;
export type BenchmarkGroup = (typeof benchmarkGroups)[number];

/**
 * Which heading a benchmark sits under.
 *
 * Derived from the id prefix rather than carried in the DTO: the backend
 * already encodes the grouping in the name, and a second source of truth
 * would let the two disagree over which section a benchmark belongs to.
 */
export function groupOf(id: BenchmarkId): BenchmarkGroup {
  if (id.startsWith('cpu')) return 'cpu';
  if (id.startsWith('memory')) return 'memory';
  if (id.startsWith('disk')) return 'disk';
  return 'gpu';
}

export interface BenchmarkInfo {
  readonly id: BenchmarkId;
  readonly available: boolean;
  /** Why not, when `available` is false. A key into `reason.*`, or free text. */
  readonly unavailableReason: string | null;
  readonly estimatedSeconds: number;
}

export interface RunConditionsDto {
  readonly powerPlan: string | null;
  readonly onBattery: boolean;
  readonly throttled: boolean;
  /** Fraction 0..1 of the machine busy with something that is not us. */
  readonly backgroundLoad: number;
  readonly ambientStartTemp: number | null;
  readonly ambientEndTemp: number | null;
  /** The backend's own verdict that conditions invalidated the measurement. */
  readonly tainted: boolean;
}

export interface BenchmarkResultDto {
  readonly id: BenchmarkId;
  readonly score: number;
  readonly runs: readonly number[];
  /** MB/s, ns, ops/s — differs per benchmark, so it is never assumed here. */
  readonly unit: string;
  readonly durationMs: number;
  /**
   * Coefficient of variation across runs, as a **percentage** (5.09 means
   * 5.09%). Null when there was only one run.
   *
   * The scale matters and is easy to get wrong: `BenchmarkResult::variability`
   * in `vitals-bench` already multiplies by 100, and `is_trustworthy` compares
   * against `5.0`. Reading it as a 0..1 fraction makes every threshold here
   * fire a hundred times too late and prints "0.05%" for a 5% spread.
   */
  readonly variability: number | null;
  readonly trustworthy: boolean;
  readonly conditions: RunConditionsDto;
  readonly timestampMs: number;
}

export interface BenchmarkSuiteDto {
  readonly results: readonly BenchmarkResultDto[];
  readonly totalDurationMs: number;
}

/**
 * Above this spread the runs disagree with each other enough that the median
 * is no longer a description of the machine.
 *
 * 5% is the point at which a re-run would plausibly reorder two CPUs the user
 * is comparing, which is the only thing a benchmark number is ever used for.
 *
 * A percentage, matching `variability`, and deliberately the same 5.0 that
 * `BenchmarkResult::is_trustworthy` uses in Rust — if the two ever disagree
 * the screen would contradict the `trustworthy` flag it is rendering.
 */
export const VARIABILITY_LIMIT = 5;

/**
 * Background activity above this makes the machine a poor test subject.
 *
 * A percentage of total machine CPU, matching `RunConditions::is_tainted`.
 */
export const BACKGROUND_LOAD_LIMIT = 10;

export type DistrustReason =
  'throttled' | 'onBattery' | 'backgroundLoad' | 'variability' | 'tainted';

/**
 * Everything wrong with the conditions this result was taken under.
 *
 * Returned even when `trustworthy` is true, so the screen can show a clean
 * run's conditions as evidence rather than as an absence. A benchmark that
 * only explains itself when it failed asks the user to trust silence.
 */
export function distrustReasons(result: BenchmarkResultDto): readonly DistrustReason[] {
  const reasons: DistrustReason[] = [];
  if (result.conditions.throttled) reasons.push('throttled');
  if (result.conditions.onBattery) reasons.push('onBattery');
  if (result.conditions.backgroundLoad > BACKGROUND_LOAD_LIMIT) reasons.push('backgroundLoad');
  if (result.variability !== null && result.variability > VARIABILITY_LIMIT) {
    reasons.push('variability');
  }
  // Last, and only if nothing more specific applied: "conditions were bad" is
  // the least useful sentence we can show, so it is a fallback for a taint the
  // backend saw and we cannot name.
  if (result.conditions.tainted && reasons.length === 0) reasons.push('tainted');
  return reasons;
}

/** Whether the screen should present this score as a real measurement. */
export function isTrustworthy(result: BenchmarkResultDto): boolean {
  return result.trustworthy && distrustReasons(result).length === 0;
}

/**
 * The middle value of the runs.
 *
 * Falls back to the backend's own `score` when there are no runs, rather than
 * inventing 0 — a zero score is a claim about the hardware, and "we have no
 * passes to summarise" is not that claim.
 */
export function medianOf(result: BenchmarkResultDto): number {
  if (result.runs.length === 0) return result.score;
  const sorted = [...result.runs].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  if (sorted.length % 2 === 1) return sorted[mid] ?? result.score;
  return ((sorted[mid - 1] ?? 0) + (sorted[mid] ?? 0)) / 2;
}

export interface Spread {
  readonly min: number;
  readonly max: number;
}

/** The best and worst pass. Null when there is nothing to compare. */
export function spreadOf(result: BenchmarkResultDto): Spread | null {
  if (result.runs.length < 2) return null;
  return { min: Math.min(...result.runs), max: Math.max(...result.runs) };
}

/** The benchmarks the backend says it can actually run. */
export function availableIds(infos: readonly BenchmarkInfo[]): readonly BenchmarkId[] {
  return infos.filter((info) => info.available).map((info) => info.id);
}

/**
 * How long the chosen set will take, in seconds.
 *
 * Shown before the user commits, because these run for tens of seconds each
 * and peg the machine while they do. Starting a minute-long operation that
 * makes the computer unresponsive without saying so first is a trap, not a
 * feature.
 */
export function estimatedSeconds(
  infos: readonly BenchmarkInfo[],
  selected: ReadonlySet<BenchmarkId>,
): number {
  return infos
    .filter((info) => info.available && selected.has(info.id))
    .reduce((total, info) => total + info.estimatedSeconds, 0);
}

/** Groups in a fixed order, so sections do not reshuffle between reads. */
export function byGroup(
  infos: readonly BenchmarkInfo[],
): readonly { readonly group: BenchmarkGroup; readonly infos: readonly BenchmarkInfo[] }[] {
  return benchmarkGroups
    .map((group) => ({ group, infos: infos.filter((info) => groupOf(info.id) === group) }))
    .filter((section) => section.infos.length > 0);
}
