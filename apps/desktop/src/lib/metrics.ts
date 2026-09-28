/**
 * Receives metrics frames from the Rust sampler.
 *
 * Frames are **pushed** over an event channel rather than polled with
 * `invoke`. Polling would run serialisation of thousands of process rows on
 * the webview's main thread several times a second, which stutters the very
 * UI that exists to show you what is stuttering.
 *
 * Deltas are reconciled here so consumers always see a complete picture.
 *
 * # One listener for the session, not one per screen
 *
 * Each subscriber used to run its own `listen()` and its own reconciler,
 * starting from an empty map. A delta only lists what changed, so a screen
 * opened between keyframes (every 30 ticks) folded ~90 changed rows onto
 * nothing: Processes showed "116 of 116" on a machine running 780, for up to
 * thirty seconds (measured live, 2026-09-28). Now one listener is installed
 * on first use and kept for the life of the window, so the map is always
 * whole, and a new subscriber is handed the current snapshot immediately —
 * which is also what lets a screen's first render already have data.
 *
 * Kept when the last subscriber leaves on purpose: `<Activity>` tears down a
 * hidden screen's effects, so "nobody subscribed" is the normal state between
 * navigations, and unlistening there is exactly what made the next screen
 * start blind. Folding one delta a second costs nothing measurable.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { Frame, Process, SystemMetrics } from '@vitals/protocol';

/** Event names, matching `apps/desktop/src-tauri/src/sampling.rs`. */
const FRAME_EVENT = 'vitals://frame';
const ERROR_EVENT = 'vitals://sampler-error';

/** A reconciled snapshot: the current state of everything. */
export interface Snapshot {
  readonly system: SystemMetrics;
  /** Every live process, keyed by `pid:startTime`. */
  readonly processes: ReadonlyMap<string, Process>;
  /** Monotonic frame number, for detecting gaps. */
  readonly seq: number;
  /** Real elapsed time since the previous frame. */
  readonly elapsedMs: number;
  /** Wall-clock time of this frame. */
  readonly timestampMs: number;
}

/**
 * Stable key for a process across frames.
 *
 * PID alone is not unique over time: Windows recycles PIDs aggressively, and
 * a table keyed on PID alone will silently apply one process's metrics to a
 * different process that inherited its number.
 */
export function processKey(process: Process): string {
  return `${process.key.pid}:${process.key.startTime}`;
}

export interface MetricsListener {
  onSnapshot(snapshot: Snapshot): void;
  onError?(message: string): void;
  /**
   * Called when a frame is missed.
   *
   * Deltas are only valid against a shared baseline, so a gap means the
   * picture may be stale until the next keyframe arrives.
   */
  onGap?(missed: number): void;
}

/**
 * Subscribes to metrics frames.
 *
 * The listener receives the latest snapshot at once if one exists, then
 * every frame after it.
 *
 * Returns a function that unsubscribes; call it on unmount or the listener
 * leaks for the lifetime of the window.
 */
export async function subscribeToMetrics(listener: MetricsListener): Promise<UnlistenFn> {
  await ensureHub();
  listeners.add(listener);
  if (latest !== null) listener.onSnapshot(latest);
  return () => {
    listeners.delete(listener);
  };
}

/** The most recent reconciled snapshot, or `null` before the first frame. */
export function latestMetrics(): Snapshot | null {
  return latest;
}

const listeners = new Set<MetricsListener>();
let latest: Snapshot | null = null;
let hub: Promise<void> | null = null;

function ensureHub(): Promise<void> {
  // A failed install is retried by the next subscriber rather than cached:
  // otherwise one early failure would blind every screen for the session.
  hub ??= installHub().catch((error: unknown) => {
    hub = null;
    throw error;
  });
  return hub;
}

async function installHub(): Promise<void> {
  // Mutable across frames by design: a delta describes a change to this map,
  // and rebuilding it wholesale each frame would discard the bandwidth saving
  // the delta encoding exists to provide.
  let processes = new Map<string, Process>();
  let lastSeq = 0;
  // Deltas are meaningless until a keyframe has given them a baseline. The
  // window usually starts listening after the sampler's first frame, so it
  // asks for one (below) and holds deltas back until it lands.
  let baseline = false;

  await listen<Frame>(FRAME_EVENT, (event) => {
    const frame = event.payload;

    // Detect gaps before applying, so a caller can show a "reconnecting"
    // state rather than silently rendering stale rows.
    if (lastSeq > 0 && frame.seq > lastSeq + 1) {
      for (const listener of listeners) listener.onGap?.(frame.seq - lastSeq - 1);
    }
    lastSeq = frame.seq;

    if (frame.payload.kind === 'keyframe') {
      processes = new Map(frame.payload.processes.map((p) => [processKey(p), p]));
      baseline = true;
    } else {
      if (!baseline) return;
      // Exits are applied BEFORE changes. A PID recycled within one frame
      // appears in both lists, and removing after adding would delete the
      // new process instead of the old one.
      for (const pid of frame.payload.exited) {
        for (const [key, process] of processes) {
          if (process.key.pid === pid) {
            processes.delete(key);
          }
        }
      }

      for (const process of frame.payload.changed) {
        processes.set(processKey(process), process);
      }
    }

    latest = {
      system: frame.payload.system,
      processes,
      seq: frame.seq,
      elapsedMs: frame.elapsedMs,
      timestampMs: frame.timestampMs,
    };
    for (const listener of listeners) listener.onSnapshot(latest);
  });

  await listen<string>(ERROR_EVENT, (event) => {
    for (const listener of listeners) listener.onError?.(event.payload);
  });

  // Without this the first complete picture waits for the periodic keyframe,
  // up to thirty seconds after launch or a webview reload. Fire-and-forget:
  // an older backend without the command still reaches a keyframe by itself.
  invoke('request_keyframe').catch(() => undefined);
}
