/**
 * This TV's readings, polled once a second while a screen shows them and
 * not at all otherwise — the TV is doing its real job (showing television)
 * and a monitor that costs it frames is worse than none.
 */

import { useEffect, useState } from 'react';

import { EMPTY_DEVICE, readDevice, readLoad, systemInfo, type DeviceSnapshot } from '../lib/device';
import { SPARK_POINTS } from '../lib/live';

export interface DeviceLive {
  readonly available: boolean;
  readonly loaded: boolean;
  readonly snapshot: DeviceSnapshot;
  readonly cpu: readonly number[];
  readonly memory: readonly number[];
}

function push(series: readonly number[], v: number | null): number[] {
  const next = series.length >= SPARK_POINTS ? series.slice(1) : [...series];
  next.push(v ?? Number.NaN);
  return next;
}

export function useDeviceLive(active: boolean): DeviceLive {
  const info = systemInfo();
  const [state, setState] = useState<DeviceLive>({
    available: info !== null,
    loaded: false,
    snapshot: EMPTY_DEVICE,
    cpu: [],
    memory: [],
  });

  useEffect(() => {
    if (!active || info === null) return;
    let alive = true;
    let slow = 0;
    const tick = async () => {
      // Storage, network and display change rarely; read them every ten
      // seconds and only load and memory every second.
      const full = slow % 10 === 0;
      slow++;
      const next = full ? await readDevice(info) : await readLoad(info);
      if (!alive) return;
      setState((prev) => {
        const snapshot = { ...prev.snapshot, ...next };
        const used =
          snapshot.memoryTotal !== null &&
          snapshot.memoryAvailable !== null &&
          snapshot.memoryTotal > 0
            ? ((snapshot.memoryTotal - snapshot.memoryAvailable) / snapshot.memoryTotal) * 100
            : null;
        return {
          available: true,
          loaded: prev.loaded || full,
          snapshot,
          cpu: push(prev.cpu, snapshot.cpuLoad),
          memory: push(prev.memory, used),
        };
      });
    };
    void tick();
    const timer = setInterval(() => void tick(), 1000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [active, info]);

  return state;
}
