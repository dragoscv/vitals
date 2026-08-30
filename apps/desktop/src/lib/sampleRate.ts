/**
 * Pushes the effective sample rate to the Rust sampler.
 *
 * The Settings dialog has had a sampling-rate control and a "throttle when
 * hidden" switch since the shell was built, and the Rust side has had a
 * `set_sample_rate` command to receive them. Nothing joined the two: the
 * settings wrote to the store, the store persisted them, and the sampler ran
 * at its default forever. Both halves looked finished in review.
 *
 * ## Why the effective rate is not just the setting
 *
 * The user's choice is the rate for a *visible* window. A task manager that
 * keeps sampling at 2 Hz while minimised is the exact self-defeating
 * behaviour this project exists to avoid, so visibility is folded in here
 * rather than being a second thing the backend has to reason about.
 */

import type { SampleRate } from '@vitals/protocol';

import { hasTauriHost } from '../shell/host';
import type { AppSettings, SamplingRate } from '../settings/schema';

/**
 * The protocol rate for each setting, when the window is visible.
 *
 * The protocol has six rates and the UI offers three. The three omitted ones
 * are not user choices: `realtime` is for a focused live graph, and `low` and
 * `background` are what throttling drops to. Exposing all six would be
 * offering the user a way to make the app useless.
 */
const VISIBLE: Record<SamplingRate, SampleRate> = {
  fast: 'high',
  normal: 'normal',
  slow: 'low',
};

/**
 * Where each setting drops to when the window is hidden.
 *
 * Not a single shared value: someone who chose "relaxed" while visible should
 * not be sampled *more* often once hidden, which a flat `background` for
 * everyone would avoid but a flat `low` would not.
 */
const HIDDEN: Record<SamplingRate, SampleRate> = {
  fast: 'low',
  normal: 'background',
  slow: 'background',
};

/**
 * Resolves the rate to send, given the setting and whether the window shows.
 *
 * Pure, so the policy above is testable without a window or a Tauri host.
 */
export function effectiveRate(
  settings: Pick<AppSettings, 'samplingRate' | 'throttleWhenHidden'>,
  visible: boolean,
): SampleRate {
  if (visible || !settings.throttleWhenHidden) {
    return VISIBLE[settings.samplingRate];
  }

  return HIDDEN[settings.samplingRate];
}

/**
 * Sends a rate to the sampler, if there is one to send it to.
 *
 * Failures are swallowed deliberately. The consequence of a dropped rate
 * change is that sampling continues at the previous cadence — not something
 * worth a toast, and certainly not worth an unhandled rejection in a
 * background subscription.
 */
export async function pushSampleRate(rate: SampleRate): Promise<void> {
  if (!hasTauriHost()) return;

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_sample_rate', { rate });
  } catch {
    // Intentionally ignored; see above.
  }
}
