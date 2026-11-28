/**
 * The widget tree's view of the machine.
 *
 * Two ways in, because the two places that render a widget cannot reach the
 * same source. The configuration screen lives in the main window, which already
 * has a sampler subscription and a shared history collector, so it hands the
 * system object down. A widget window is its own webview: it sees neither, and
 * subscribes to the same `vitals://frame` broadcast the HUD uses.
 *
 * Both paths roll the same traces through the same function, so a widget cannot
 * render two different sparklines depending on which window drew it.
 */

import { useEffect, useState } from 'react';

import { gpuPercent, memoryPercent, type Frame, type SystemMetrics } from '@vitals/protocol';

import { totalDiskRead, totalNetworkRx, type WidgetInput } from './describe';

/**
 * How many readings each trace remembers — about forty seconds at 1 Hz.
 *
 * Short on purpose: a widget is glanced at, and a longer window would make the
 * line so flat that a real spike stops reading as a spike.
 */
export const TRACE_LENGTH = 40;

/** Event name, matching `apps/desktop/src-tauri/src/sampling.rs`. */
const FRAME_EVENT = 'vitals://frame';

/**
 * A gap in a trace is a fact, not a blank: an unmeasurable reading is carried
 * as `null` and breaks the line rather than drawing a plunge to zero that never
 * happened. Copied from `hud/lib/live.ts`, which made the same call.
 */
function push(history: readonly number[], value: number | null): readonly number[] {
  if (value === null) return history;
  const next = history.length >= TRACE_LENGTH ? history.slice(1) : history.slice();
  next.push(value);
  return next;
}

/** Folds one system reading into the previous traces. Pure, so a test can drive it. */
export function rollUp(previous: WidgetInput, system: SystemMetrics | null): WidgetInput {
  // No reading, no advance. Returning the previous object unchanged matters:
  // `useSyncExternalStore` and React both compare by identity, so a new empty
  // object per absent frame would re-render every widget on a stalled sampler.
  if (system === null) return previous;
  return {
    system,
    cpuHistory: push(previous.cpuHistory, system.cpu.total),
    memoryHistory: push(previous.memoryHistory, memoryPercent(system)),
    gpuHistory: push(previous.gpuHistory, gpuPercent(system)),
    netHistory: push(previous.netHistory, totalNetworkRx(system)),
    diskHistory: push(previous.diskHistory, totalDiskRead(system)),
  };
}

/** Folds one frame into the previous traces. */
export function reduceWidgetInput(previous: WidgetInput, frame: Frame): WidgetInput {
  return rollUp(previous, frame.payload.system);
}

const INITIAL: WidgetInput = {
  system: null,
  cpuHistory: [],
  memoryHistory: [],
  gpuHistory: [],
  netHistory: [],
  diskHistory: [],
};

/**
 * Rolls traces from a system reading the caller already has.
 *
 * Deliberately not a subscription: the screen passes `useSystemSnapshot`'s
 * value, so one sampler subscription and one history collector serve the whole
 * window (the lesson from `features/dashboard` — one listener per session, not
 * one per screen).
 */
export function useWidgetInput(system: SystemMetrics | null): WidgetInput {
  const [input, setInput] = useState<WidgetInput>(INITIAL);

  // Keyed on the identity of the reading rather than on every render: the
  // sampler builds a new object per frame, so this fires once per frame and not
  // once per keystroke in the settings form beside the preview.
  useEffect(() => {
    setInput((previous) => rollUp(previous, system));
  }, [system]);

  return input;
}

/** Subscribes to `vitals://frame`. Returns a disposer. */
export type FrameSource = (onFrame: (frame: Frame) => void) => () => void;

/** Subscribes to a Tauri event, tolerating a window that closes mid-`listen`. */
function tauriEvent<T>(name: string, handler: (payload: T) => void): () => void {
  let unlisten: (() => void) | undefined;
  let live = true;

  void import('@tauri-apps/api/event')
    .then((module) => module.listen<T>(name, (event) => handler(event.payload)))
    .then((dispose) => {
      // The window can be destroyed before `listen` resolves — the backend
      // closes widgets on `sync_widgets` — and without this the listener
      // outlives the window that asked for it.
      if (live) unlisten = dispose;
      else dispose();
    })
    .catch(() => {
      // No host (a browser, or a window closing): the widget then shows em
      // dashes forever, which is the honest reading — see AGENTS.md.
    });

  return () => {
    live = false;
    unlisten?.();
  };
}

/** The real source for a widget window: Tauri's broadcast. */
export function tauriWidgetFrames(onFrame: (frame: Frame) => void): () => void {
  return tauriEvent<Frame>(FRAME_EVENT, onFrame);
}

/**
 * Rolls traces from the frame broadcast — the widget window's own path.
 *
 * Only the five numbers each trace needs are kept: a widget shows a handful of
 * readings, so folding the process deltas into a map nothing reads would make
 * the lightest windows in the product the most expensive ones.
 */
export function useFrameWidgetInput(source: FrameSource = tauriWidgetFrames): WidgetInput {
  const [input, setInput] = useState<WidgetInput>(INITIAL);

  useEffect(() => {
    return source((frame) => {
      setInput((previous) => reduceWidgetInput(previous, frame));
    });
  }, [source]);

  return input;
}
