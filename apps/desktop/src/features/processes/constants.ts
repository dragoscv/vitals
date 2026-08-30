/**
 * Bit values of `ProcessFlags`, mirrored from `crates/vitals-core/src/process.rs`.
 *
 * ts-rs erases the bitflags struct to a bare `number`, so the meaning of each
 * bit does not survive the boundary. Mirroring it here is the only option
 * short of editing the generated protocol package; the Rust side is the
 * source of truth and these must be changed together.
 */
export const ProcessFlag = {
  Signed: 1 << 0,
  SignatureBroken: 1 << 1,
  Elevated: 1 << 2,
  EfficiencyMode: 1 << 3,
  Wow64: 1 << 4,
  Critical: 1 << 5,
  HasWindow: 1 << 6,
  Packaged: 1 << 7,
  Preexisting: 1 << 8,
  ShortLived: 1 << 9,
  Debugged: 1 << 10,
  Managed: 1 << 11,
} as const;

export function hasFlag(flags: number, flag: number): boolean {
  return (flags & flag) !== 0;
}

/**
 * Rendered wherever a value is genuinely unknown.
 *
 * Never a zero. "0%" and "not measurable" look identical to a reader but mean
 * opposite things, and a monitoring tool that guesses is worse than one that
 * admits the gap.
 */
export const UNKNOWN = '—';

/** Row height in pixels. Fixed, because the virtualiser measures nothing. */
export const ROW_HEIGHT = 28;

/**
 * Rows rendered beyond the viewport on each side.
 *
 * Six is enough to cover a wheel flick between animation frames without
 * pushing the per-tick render cost back up towards the un-virtualised case.
 */
export const OVERSCAN = 6;
