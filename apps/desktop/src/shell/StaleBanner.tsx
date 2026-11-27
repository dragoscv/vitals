/**
 * Says, once, that the live readings have gone quiet.
 *
 * The sampler pushes a frame a second; when the stream stops — a wedged
 * sampler thread, a dropped event channel, a machine suspending — the meters
 * simply freeze on their last value and look perfectly healthy. A screenshot
 * of a frozen dashboard is indistinguishable from a still machine, so the
 * user has no way to tell that what they are reading is minutes old. This is
 * the one line that says so, above everything else.
 *
 * It is deliberately small and unstyled beyond a warn tint: it is a footnote
 * about the data, not an error to act on, and it vanishes on its own the
 * instant a frame lands again.
 */

import { AlertTriangle } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import type { Snapshot } from '@/lib/metrics';
import { latestMetrics, subscribeToMetrics } from '@/lib/metrics';

import { samplingIntervalMs } from '../settings/schema';
import { useSettings } from '../settings/store';
import { hasTauriHost } from './host';
import { SHELL_NS } from './strings';

/**
 * How many missed intervals count as stale.
 *
 * Two, not one: the sampler runs late under load, and a single late frame is
 * normal. The requirement is the same as the phone's last-seen chip — twice
 * the current cadence, whatever the user chose, never a hard-coded second.
 */
export const STALE_AFTER_INTERVALS = 2;

/**
 * How often the age is re-measured while nothing arrives.
 *
 * A frame wakes this component on its own; the timer exists for the opposite
 * case, where the stream has stopped and nothing else would ever re-render to
 * notice. Half a second keeps the banner within one tick of the threshold for
 * every cadence the UI offers.
 */
export const TICK_MS = 500;

export interface StaleFeed {
  /** How long ago the last frame reached this window. */
  readonly ageMs: number;
  /** Epoch milliseconds of that frame, for the "since" sentence. */
  readonly sinceMs: number;
}

/**
 * Whether the feed has gone quiet.
 *
 * Pure, so the threshold is testable without a clock or a DOM. `null` covers
 * both "no frame yet" (nothing to be stale about) and "fresh enough".
 */
export function staleFeed(
  last: Snapshot | null,
  now: number,
  intervalMs: number,
): StaleFeed | null {
  if (last === null) return null;
  const ageMs = now - last.timestampMs;
  if (ageMs <= STALE_AFTER_INTERVALS * intervalMs) return null;
  return { ageMs, sinceMs: last.timestampMs };
}

/**
 * The age of the last frame this window received, or `null` while the feed is
 * current. Backed by the session-long stream rather than the dashboard's own
 * subscription, so it is live even when the dashboard is not on screen — the
 * whole point is to notice when metrics stop, wherever the user happens to be.
 */
export function useStaleFeed(intervalMs: number): StaleFeed | null {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(() => latestMetrics());
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    // Without a host there is no stream to be stale about, and `listen()`
    // dereferences an internals global that does not exist.
    if (!hasTauriHost()) return;
    let live = true;
    let stop: (() => void) | null = null;
    void subscribeToMetrics({
      onSnapshot(next: Snapshot) {
        setSnapshot(next);
        // Stamp the clock at receipt rather than waiting for the next tick,
        // so the banner disappears the instant the readings resume.
        setNow(Date.now());
      },
    })
      .then((un) => {
        if (live) stop = un;
        else un();
      })
      .catch(() => {
        // No stream to watch. A sampler that never delivered is reported by
        // the dashboard itself; there is nothing extra to say here.
      });
    return () => {
      live = false;
      stop?.();
    };
  }, []);

  useEffect(() => {
    const id = setInterval(() => {
      setNow(Date.now());
    }, TICK_MS);
    return () => {
      clearInterval(id);
    };
  }, []);

  return staleFeed(snapshot, now, intervalMs);
}

export function StaleBanner(): React.JSX.Element | null {
  const { t, i18n } = useTranslation(SHELL_NS);
  const rate = useSettings((state) => state.settings.samplingRate);
  const stale = useStaleFeed(samplingIntervalMs[rate]);
  if (stale === null) return null;

  const time = new Intl.DateTimeFormat(i18n.language, {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  }).format(new Date(stale.sinceMs));

  return (
    <div
      role="status"
      className="flex shrink-0 items-center justify-center gap-2 border-b border-[var(--color-status-warn)] bg-[var(--color-bg-inset)] px-4 py-1.5 text-xs text-[var(--color-warn-text)]"
    >
      <AlertTriangle aria-hidden className="size-3.5 shrink-0" />
      <span>{t('stale.banner', { time })}</span>
    </div>
  );
}
