/**
 * The overlay's view of the machine.
 *
 * Frames arrive on the same `vitals://frame` event the dashboard uses, but the
 * HUD ignores the process lists entirely: it shows three numbers, and folding
 * thousands of delta rows into a map that nothing reads would make the
 * lightest window in the product the most expensive one.
 */

import { useEffect, useState } from 'react';

import { gpuPercent, memoryPercent, type Frame, type SystemMetrics } from '@vitals/protocol';

/** Event name, matching `apps/desktop/src-tauri/src/sampling.rs`. */
const FRAME_EVENT = 'vitals://frame';

/** How many readings each sparkline remembers — about a minute at 1 Hz. */
export const HISTORY_LENGTH = 40;

/** Everything the overlay renders. Immutable; a new object per frame. */
export interface HudState {
  readonly system: SystemMetrics | null;
  readonly cpuHistory: readonly number[];
  readonly memoryHistory: readonly number[];
  readonly gpuHistory: readonly number[];
}

export const INITIAL_HUD_STATE: HudState = {
  system: null,
  cpuHistory: [],
  memoryHistory: [],
  gpuHistory: [],
};

/** Folds one frame into the previous state. Pure, so a test can drive it. */
export function reduce(previous: HudState, frame: Frame): HudState {
  const system = frame.payload.system;
  return {
    system,
    cpuHistory: push(previous.cpuHistory, system.cpu.total),
    memoryHistory: push(previous.memoryHistory, memoryPercent(system)),
    gpuHistory: push(previous.gpuHistory, gpuPercent(system)),
  };
}

/**
 * A gap in a trace is a fact, not a blank: an unmeasurable reading is carried
 * as `null` so the sparkline can break the line rather than draw a plunge to
 * zero that never happened.
 */
function push(history: readonly number[], value: number | null): readonly number[] {
  if (value === null) return history;
  const next = history.length >= HISTORY_LENGTH ? history.slice(1) : history.slice();
  next.push(value);
  return next;
}

/** Subscribes to `vitals://frame`. Returns a disposer. */
export type FrameSource = (onFrame: (frame: Frame) => void) => () => void;

/** The real source: Tauri's event channel. */
export function tauriFrames(onFrame: (frame: Frame) => void): () => void {
  let unlisten: (() => void) | undefined;
  let live = true;

  void import('@tauri-apps/api/event')
    .then((module) => module.listen<Frame>(FRAME_EVENT, (event) => onFrame(event.payload)))
    .then((dispose) => {
      // The window can close before `listen` resolves; without this the
      // listener outlives the component that asked for it.
      if (live) unlisten = dispose;
      else dispose();
    })
    .catch(() => {
      // No host, or the event system is unavailable. The overlay then shows
      // em dashes forever, which is the honest reading — see AGENTS.md.
    });

  return () => {
    live = false;
    unlisten?.();
  };
}

/** Subscribes a component to the frame stream. */
export function useHudState(source: FrameSource = tauriFrames): HudState {
  const [state, setState] = useState<HudState>(INITIAL_HUD_STATE);

  useEffect(() => {
    return source((frame) => {
      setState((previous) => reduce(previous, frame));
    });
  }, [source]);

  return state;
}
