/**
 * The widget registry, persisted and mirrored into real windows.
 *
 * Two debounced writers, and the split matters. The disk write is 250 ms —
 * the same interval the dashboard layout uses — because a drag emits a
 * position change per pointer move and each save is an open, serialise and
 * fsync. The push to Rust is faster (120 ms) so the window follows the setting
 * without a visible lag, and it is cheap despite the frequency: the backend
 * compares the incoming spec against the window it already has and returns
 * early when only the position moved, which is the case for every tick of a
 * drag. That comparison is why this can re-send the whole list rather than
 * track a dirty subset.
 *
 * The list is never pruned from the sync answer. A widget window the backend
 * refused to build and one the user closed are indistinguishable in a list of
 * ids, so treating a missing id as "delete it" would let one unbuildable spec
 * — a window-manager refusal, a display that went away — silently delete the
 * user's configuration. Closure arrives as its own event, with a reason.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { reportToLog } from '@/lib/logToFile';
import { createKeyedBackend, type KeyedBackend } from '@/settings/persistence';

import {
  addWidget,
  newWidgetId,
  parseWidgets,
  removeWidget,
  sameRegistry,
  toWindowSpec,
  updateWidget,
  type WidgetInstance,
  type WidgetKind,
  type WidgetRegistry,
} from './model';
import { tauriWidgetBridge, type WidgetBridge } from './windows';

const KEY = 'widgets';
const WRITE_DEBOUNCE_MS = 250;
const SYNC_DEBOUNCE_MS = 120;

export interface WidgetController {
  readonly widgets: WidgetRegistry;
  /** False until the stored list has been read; the screen shows a skeleton. */
  readonly hydrated: boolean;
  /** The ids the host reports as on screen, for the screen's "open" marks. */
  readonly open: readonly string[];
  /** A widget added from a string id, so a test can name it and a cascade is deterministic. */
  add: (kind: WidgetKind, id?: string) => void;
  remove: (id: string) => void;
  update: (id: string, patch: Partial<Omit<WidgetInstance, 'id' | 'kind'>>) => void;
  reset: () => void;
}

export interface WidgetStoreOptions {
  readonly backend?: KeyedBackend;
  readonly bridge?: WidgetBridge;
}

export function useWidgets(options: WidgetStoreOptions = {}): WidgetController {
  const { backend = createKeyedBackend(KEY), bridge = tauriWidgetBridge } = options;

  const [widgets, setWidgets] = useState<WidgetRegistry>([]);
  const [hydrated, setHydrated] = useState(false);
  const [open, setOpen] = useState<readonly string[]>([]);

  // Both defaults allocate (or are module constants), and a caller may pass a
  // fresh object literal, so neither can be a dependency. The refs are stable
  // and the effects read `.current` when they need it.
  const backendRef = useRef(backend);
  const bridgeRef = useRef(bridge);
  useEffect(() => {
    backendRef.current = backend;
    bridgeRef.current = bridge;
  });

  useEffect(() => {
    let cancelled = false;

    void backendRef.current
      .load()
      .then((raw) => {
        if (cancelled) return;
        setWidgets(parseWidgets(raw));
      })
      .catch((error: unknown) => {
        // Defaults, never a throw: a corrupt file must not stop the app from
        // opening, which would turn a cosmetic problem into a broken product.
        reportToLog('warn', error, 'widgets');
      })
      .finally(() => {
        if (!cancelled) setHydrated(true);
      });

    return () => {
      cancelled = true;
    };
  }, [backendRef]);

  useEffect(() => {
    if (!hydrated) return;

    const timer = setTimeout(() => {
      void backendRef.current.save(widgets).catch((error: unknown) => {
        reportToLog('warn', error, 'widgets');
      });
    }, WRITE_DEBOUNCE_MS);

    return () => {
      clearTimeout(timer);
    };
  }, [widgets, hydrated, backendRef]);

  useEffect(() => {
    if (!hydrated) return;

    const specs = widgets.map(toWindowSpec);

    // A burst of edits — a drag reporting a position per frame — collapses to
    // one push, because each run of this effect cancels the previous timer.
    const timer = setTimeout(() => {
      void bridgeRef.current
        .sync(specs)
        .then((live) => {
          setOpen(live);
          const wanted = new Set(specs.map((spec) => spec.id));
          const missing = live.filter((id) => !wanted.has(id));
          if (missing.length > 0) {
            // Not an error to act on — the ids that failed are already logged by
            // the backend — but a list that disagrees with the screen forever in
            // silence is exactly the bug nobody reports.
            reportToLog(
              'warn',
              new Error(`widget windows refused: ${missing.join(', ')}`),
              'widgets',
            );
          }
        })
        .catch((error: unknown) => {
          // `sync` never rejects; this is the belt to its braces, because a
          // window that silently never appears is invisible otherwise.
          reportToLog('warn', error, 'widgets');
        });
    }, SYNC_DEBOUNCE_MS);

    return () => {
      clearTimeout(timer);
    };
  }, [widgets, hydrated, bridgeRef]);

  // The window host is the authority on where a window is: it is the side that
  // sees the drag. Applying its report here keeps the stored position the one
  // the user last chose, so a restart does not undo the move.
  useEffect(() => {
    if (!hydrated) return;
    return bridgeRef.current.onMoved((move) => {
      setWidgets((current) => updateWidget(current, move.id, { x: move.x, y: move.y }));
    });
  }, [hydrated, bridgeRef]);

  // Closed from inside the widget, the only way a widget can be closed without
  // the settings screen being open. Its own event rather than a missing id in a
  // sync answer — see the module note.
  useEffect(() => {
    if (!hydrated) return;
    return bridgeRef.current.onClosed(({ id }) => {
      setWidgets((current) => removeWidget(current, id));
    });
  }, [hydrated, bridgeRef]);

  // A flag flipped from the widget's own toolbar. Without this the settings
  // screen would show a pin as off after the user turned it on in the widget.
  useEffect(() => {
    if (!hydrated) return;
    return bridgeRef.current.onChanged((spec) => {
      setWidgets((current) =>
        updateWidget(current, spec.id, {
          x: spec.x,
          y: spec.y,
          opacity: spec.opacity,
          alwaysOnTop: spec.alwaysOnTop,
          clickThrough: spec.clickThrough,
        }),
      );
    });
  }, [hydrated, bridgeRef]);

  const add = useCallback((kind: WidgetKind, id?: string) => {
    setWidgets((current) => {
      const next = addWidget(current, kind, id ?? newWidgetId());
      // The same array at the cap, so a click that could not add anything does
      // not schedule a write that would rewrite the file identically.
      return sameRegistry(current, next) ? current : next;
    });
  }, []);

  const remove = useCallback((id: string) => {
    setWidgets((current) => removeWidget(current, id));
  }, []);

  const update = useCallback((id: string, patch: Partial<Omit<WidgetInstance, 'id' | 'kind'>>) => {
    setWidgets((current) => updateWidget(current, id, patch));
  }, []);

  const reset = useCallback(() => {
    setWidgets([]);
  }, []);

  return { widgets, hydrated, open, add, remove, update, reset };
}
