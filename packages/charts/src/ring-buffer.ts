/**
 * A fixed-capacity circular buffer of numbers backed by a typed array.
 *
 * Every live chart in Vitals is fed by one of these. The alternative —
 * `array.push()` then `array.shift()` — is O(n) per sample because `shift`
 * reindexes the whole array, and it allocates continuously, which means the
 * garbage collector runs during animation and drops frames.
 *
 * A `Float32Array` written at a moving head index allocates exactly once, for
 * the lifetime of the chart.
 */
export class RingBuffer {
  readonly #data: Float32Array;
  readonly #capacity: number;
  #head = 0;
  #size = 0;

  constructor(capacity: number) {
    if (!Number.isInteger(capacity) || capacity <= 0) {
      throw new RangeError(`RingBuffer capacity must be a positive integer, got ${capacity}`);
    }
    this.#capacity = capacity;
    this.#data = new Float32Array(capacity);
  }

  get capacity(): number {
    return this.#capacity;
  }

  /** Number of samples currently held, up to {@link capacity}. */
  get size(): number {
    return this.#size;
  }

  get isFull(): boolean {
    return this.#size === this.#capacity;
  }

  /** Appends a sample, overwriting the oldest once full. */
  push(value: number): void {
    this.#data[this.#head] = value;
    this.#head = (this.#head + 1) % this.#capacity;
    if (this.#size < this.#capacity) this.#size += 1;
  }

  /**
   * Reads by age: index 0 is the oldest retained sample, `size - 1` the newest.
   *
   * Returns `undefined` out of range rather than throwing, because charts
   * routinely read a window wider than the data collected so far and an
   * exception there would be noise, not information.
   */
  at(index: number): number | undefined {
    if (index < 0 || index >= this.#size) return undefined;
    const start = this.#size === this.#capacity ? this.#head : 0;
    return this.#data[(start + index) % this.#capacity];
  }

  /** The most recent sample, or `undefined` when empty. */
  get last(): number | undefined {
    return this.#size === 0 ? undefined : this.at(this.#size - 1);
  }

  /**
   * Highest and lowest retained values, in one pass.
   *
   * Combined because autoscaling needs both every frame, and two separate
   * traversals of a 600-point buffer per chart per frame is measurable when
   * twenty widgets are on screen.
   */
  extent(): { min: number; max: number } {
    if (this.#size === 0) return { min: 0, max: 0 };

    let min = Infinity;
    let max = -Infinity;
    for (let i = 0; i < this.#size; i += 1) {
      const v = this.at(i);
      // Gaps are recorded as NaN (see `pushGap`) and must not poison the scale.
      if (v === undefined || Number.isNaN(v)) continue;
      if (v < min) min = v;
      if (v > max) max = v;
    }

    return Number.isFinite(min) ? { min, max } : { min: 0, max: 0 };
  }

  /** Mean of retained samples, ignoring gaps. */
  average(): number {
    let sum = 0;
    let count = 0;
    for (let i = 0; i < this.#size; i += 1) {
      const v = this.at(i);
      if (v === undefined || Number.isNaN(v)) continue;
      sum += v;
      count += 1;
    }
    return count === 0 ? 0 : sum / count;
  }

  /**
   * Records a missing sample.
   *
   * Distinct from pushing 0: a dropped frame is not "the CPU went to zero".
   * The renderer breaks the line at a gap instead of drawing a cliff down to
   * the axis and back, which would be a visible lie about what happened.
   */
  pushGap(): void {
    this.push(Number.NaN);
  }

  /** Copies the retained samples into a new array, oldest first. */
  toArray(): number[] {
    const out = new Array<number>(this.#size);
    for (let i = 0; i < this.#size; i += 1) {
      out[i] = this.at(i) ?? Number.NaN;
    }
    return out;
  }

  clear(): void {
    this.#head = 0;
    this.#size = 0;
    this.#data.fill(0);
  }
}
