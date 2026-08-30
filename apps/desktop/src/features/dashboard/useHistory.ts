/**
 * Subscribes a component to the history collector.
 *
 * `useSyncExternalStore` rather than state in the collector, because the
 * collector must be usable outside React — it starts at boot and keeps
 * collecting while the dashboard is unmounted, which is the whole reason it
 * exists as a module singleton.
 *
 * The snapshot returned is the collector's state object, whose identity
 * changes once per tick while the ring buffers inside it are mutated in place.
 * That is deliberate: the buffers hold ~180 floats each across ~20 series, and
 * copying them to satisfy immutability would allocate tens of kilobytes a
 * second to communicate one bit — "there is new data" — which the `revision`
 * counter already carries.
 */

import { useCallback, useRef, useSyncExternalStore } from 'react';

import { history as sharedHistory, type HistoryCollector, type MetricHistory } from './history';

export function useHistory(collector: HistoryCollector = sharedHistory): MetricHistory {
  const ref = useRef(collector);
  ref.current = collector;

  const subscribe = useCallback((listener: () => void) => ref.current.subscribe(listener), []);
  const getSnapshot = useCallback(() => ref.current.current, []);

  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
