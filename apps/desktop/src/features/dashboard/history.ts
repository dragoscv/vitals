/**
 * Rolling history of system metrics, collected app-wide.
 *
 * # Why this is a module singleton and not a hook
 *
 * The obvious design is a `useHistory` hook owning ring buffers per widget.
 * It is also wrong, and visibly so: React unmounts the dashboard when the user
 * navigates to Processes, the buffers go with it, and coming back shows an
 * empty chart that refills from zero. A history chart that forgets its history
 * whenever you look at something else is worse than no chart, because it
 * silently implies the machine was idle for the last two minutes.
 *
 * So collection starts once, at boot, and runs for the lifetime of the window
 * regardless of which route is mounted. Widgets read a buffer that was already
 * filling before they existed.
 *
 * # Why the cost of that is acceptable
 *
 * One `Float64Array` of 180 samples per series. At the ~20 series tracked here
 * that is under 30 KB, fixed, forever — the buffers never grow, because a ring
 * buffer overwrites rather than appends. The alternative (retaining raw frames
 * and deriving series on demand) would hold ~600 process records per tick and
 * grow without bound.
 *
 * # Gaps are recorded, not interpolated
 *
 * When the sampler stalls or a frame is dropped, {@link pushGap} marks a break
 * so the renderer lifts the pen instead of drawing a straight line across the
 * missing interval. A line implies "we measured this and it was steady", which
 * is exactly the opposite of what happened.
 */

import { RingBuffer } from '@vitals/charts';

import type { Snapshot } from '@/lib/metrics';

/**
 * Samples retained per series.
 *
 * 180 is three minutes at the default 1 Hz, and 90 seconds at the `fast` rate.
 * Chosen against the question the dashboard exists to answer — "what just
 * happened to my machine" — rather than a round number: a spike the user
 * noticed and came to investigate is seconds to a couple of minutes old, and
 * anything older belongs in the History feature, which persists to disk.
 */
export const HISTORY_CAPACITY = 180;

/**
 * How long without a frame counts as a break in the data.
 *
 * Generous relative to the 1000 ms nominal tick: a sampler that runs slightly
 * late is normal, and marking every 1100 ms interval as a gap would shred the
 * line into disconnected fragments. Two and a half seconds means a genuine
 * stall, not jitter.
 */
export const GAP_THRESHOLD_MS = 2500;

/** Series tracked for every machine, regardless of hardware. */
export interface CoreSeries {
  readonly cpu: RingBuffer;
  readonly cpuKernel: RingBuffer;
  readonly memoryUsed: RingBuffer;
  readonly memoryPercent: RingBuffer;
  readonly diskRead: RingBuffer;
  readonly diskWrite: RingBuffer;
  readonly netRx: RingBuffer;
  readonly netTx: RingBuffer;
}

/**
 * A per-device series, keyed by the device's stable id.
 *
 * Kept in a map rather than an array because devices can disappear between
 * frames — unplugging a USB drive or a dock removes a disk and a NIC — and an
 * index-based structure would shift every subsequent device's history onto the
 * wrong chart at the moment of removal.
 *
 * Keyed by the numeric id the protocol uses, not by name: names are not unique
 * (two identical GPUs report the same string) and the id is documented stable
 * for the lifetime of a boot, which is exactly the lifetime of these buffers.
 */
export type DeviceSeries = Map<number, RingBuffer>;

export interface MetricHistory {
  readonly core: CoreSeries;
  /** Per-GPU maximum engine utilisation. */
  readonly gpu: DeviceSeries;
  readonly gpuMemory: DeviceSeries;
  readonly diskActive: DeviceSeries;
  /**
   * Increments whenever samples land.
   *
   * The buffers are mutable and deliberately outside React state; this counter
   * is the one cheap value a component can subscribe to in order to know a
   * repaint is due. Pushing the arrays themselves through state would allocate
   * ~20 copies of 180 floats every second to convey one bit of information.
   */
  readonly revision: number;
  /** Wall-clock time of the most recent sample, or 0 before the first. */
  readonly lastTimestampMs: number;
  /** Samples collected since the window opened, for "not enough data yet". */
  readonly sampleCount: number;
}

function createCore(): CoreSeries {
  return {
    cpu: new RingBuffer(HISTORY_CAPACITY),
    cpuKernel: new RingBuffer(HISTORY_CAPACITY),
    memoryUsed: new RingBuffer(HISTORY_CAPACITY),
    memoryPercent: new RingBuffer(HISTORY_CAPACITY),
    diskRead: new RingBuffer(HISTORY_CAPACITY),
    diskWrite: new RingBuffer(HISTORY_CAPACITY),
    netRx: new RingBuffer(HISTORY_CAPACITY),
    netTx: new RingBuffer(HISTORY_CAPACITY),
  };
}

/**
 * A history collector.
 *
 * Exported as a factory as well as a singleton so tests can drive one in
 * isolation without the module-level instance leaking state between cases.
 */
export class HistoryCollector {
  private state: MetricHistory = {
    core: createCore(),
    gpu: new Map(),
    gpuMemory: new Map(),
    diskActive: new Map(),
    revision: 0,
    lastTimestampMs: 0,
    sampleCount: 0,
  };

  private readonly listeners = new Set<() => void>();

  get current(): MetricHistory {
    return this.state;
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /**
   * Folds one frame into every series.
   *
   * Totals across devices are summed here rather than in the widgets so that
   * the "Disk" and "Network" charts agree with each other and with the
   * per-device panels, whatever set of devices happens to be present.
   */
  push(snapshot: Snapshot): void {
    const { core } = this.state;
    const previous = this.state.lastTimestampMs;

    if (previous > 0 && snapshot.timestampMs - previous > GAP_THRESHOLD_MS) {
      this.markGap();
    }

    const { cpu, memory, disks, networks, gpus } = snapshot.system;

    core.cpu.push(cpu.total);
    core.cpuKernel.push(cpu.kernel);
    core.memoryUsed.push(memory.used);
    core.memoryPercent.push(memory.total > 0 ? (memory.used / memory.total) * 100 : 0);

    let read = 0;
    let write = 0;
    for (const disk of disks) {
      read += disk.read;
      write += disk.write;
      pushInto(this.state.diskActive, disk.id, disk.activeTime);
    }
    core.diskRead.push(read);
    core.diskWrite.push(write);

    let rx = 0;
    let tx = 0;
    for (const nic of networks) {
      rx += nic.rx;
      tx += nic.tx;
    }
    core.netRx.push(rx);
    core.netTx.push(tx);

    for (const gpu of gpus) {
      pushInto(this.state.gpu, gpu.id, gpu.utilization);
      if (gpu.memoryUsed !== null) pushInto(this.state.gpuMemory, gpu.id, gpu.memoryUsed);
    }

    this.state = {
      ...this.state,
      revision: this.state.revision + 1,
      lastTimestampMs: snapshot.timestampMs,
      sampleCount: this.state.sampleCount + 1,
    };
    this.emit();
  }

  /**
   * Records a break in every series.
   *
   * Applied across the board rather than per-series because the cause is
   * always the sampler: if one metric is missing for an interval they all are,
   * and a chart that keeps drawing while its neighbour has a gap suggests the
   * gap is specific to that device.
   */
  markGap(): void {
    const { core } = this.state;
    for (const buffer of Object.values(core)) buffer.pushGap();
    for (const map of [this.state.gpu, this.state.gpuMemory, this.state.diskActive]) {
      for (const buffer of map.values()) buffer.pushGap();
    }
    this.state = { ...this.state, revision: this.state.revision + 1 };
    this.emit();
  }

  /** Test seam. Also used when the sampling rate changes, which invalidates
   *  the time axis — 180 samples no longer mean the same span. */
  reset(): void {
    this.state = {
      core: createCore(),
      gpu: new Map(),
      gpuMemory: new Map(),
      diskActive: new Map(),
      revision: this.state.revision + 1,
      lastTimestampMs: 0,
      sampleCount: 0,
    };
    this.emit();
  }

  private emit(): void {
    for (const listener of this.listeners) listener();
  }
}

function pushInto(map: DeviceSeries, key: number, value: number): void {
  let buffer = map.get(key);
  if (buffer === undefined) {
    buffer = new RingBuffer(HISTORY_CAPACITY);
    // A device that appears mid-session has no history, and padding it with
    // zeroes would draw a flat line implying it was idle and present. Leaving
    // the buffer short lets the renderer start the line where the data does.
    map.set(key, buffer);
  }
  buffer.push(value);
}

/** The instance the running application collects into. */
export const history = new HistoryCollector();
