/**
 * Receives metrics frames from the Rust sampler.
 *
 * Frames are **pushed** over an event channel rather than polled with
 * `invoke`. Polling would run serialisation of thousands of process rows on
 * the webview's main thread several times a second, which stutters the very
 * UI that exists to show you what is stuttering.
 *
 * Deltas are reconciled here so consumers always see a complete picture.
 */

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
 * Returns a function that unsubscribes; call it on unmount or the listener
 * leaks for the lifetime of the window.
 */
export async function subscribeToMetrics(listener: MetricsListener): Promise<UnlistenFn> {
  // Mutable across frames by design: a delta describes a change to this map,
  // and rebuilding it wholesale each frame would discard the bandwidth saving
  // the delta encoding exists to provide.
  let processes = new Map<string, Process>();
  let lastSeq = 0;

  const unlistenFrame = await listen<Frame>(FRAME_EVENT, (event) => {
    const frame = event.payload;

    // Detect gaps before applying, so a caller can show a "reconnecting"
    // state rather than silently rendering stale rows.
    if (lastSeq > 0 && frame.seq > lastSeq + 1) {
      listener.onGap?.(frame.seq - lastSeq - 1);
    }
    lastSeq = frame.seq;

    if (frame.payload.kind === 'keyframe') {
      processes = new Map(frame.payload.processes.map((p) => [processKey(p), p]));
    } else {
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

    listener.onSnapshot({
      system: frame.payload.system,
      processes,
      seq: frame.seq,
      elapsedMs: frame.elapsedMs,
      timestampMs: frame.timestampMs,
    });
  });

  const unlistenError = await listen<string>(ERROR_EVENT, (event) => {
    listener.onError?.(event.payload);
  });

  return () => {
    unlistenFrame();
    unlistenError();
  };
}
