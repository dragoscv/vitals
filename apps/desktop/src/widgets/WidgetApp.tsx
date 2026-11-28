/**
 * A placed widget, running in its own window.
 *
 * The window knows three things and nothing else: which widget it is (its
 * Tauri label), what that widget is configured to show (the backend's spec,
 * pushed to it), and the machine's current readings (the same `vitals://frame`
 * broadcast the HUD listens to). It never reads the settings store — its
 * capability grants no store permission — so the spec arriving over an event is
 * the only way it can know it is pinned, and the reason `onConfig` exists.
 *
 * # Why the panel is not the window
 *
 * The window is transparent and slightly larger than the panel at rest, so the
 * morph from compact to expanded grows the panel inside the window rather than
 * resizing the OS window. Resizing a window on every hover would be a drag
 * through the window manager for a purely visual effect, and on Windows it
 * flickers. The window is created at the size of the largest state (see
 * `geometryFor`), which is also why the drag region is the whole window: the
 * user grabs the panel by its content, not by a strip.
 */

import { MousePointer2, MousePointerBan, Pin, PinOff, Settings, X } from 'lucide-react';
import { LazyMotion, MotionConfig, domAnimation } from 'motion/react';
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { cn } from '@vitals/ui';

import { describe } from '../features/widgets/describe';
import { useFrameWidgetInput, type FrameSource } from '../features/widgets/live';
import { widgetIdFromLabel, type WidgetInstance } from '../features/widgets/model';
import {
  tauriWidgetBridge,
  type WidgetBridge,
  type WidgetFlags,
} from '../features/widgets/windows';
import { WidgetPreview } from '../features/widgets/WidgetPreview';
import { widgetWindow, type WidgetWindow } from './lib/window';

export interface WidgetAppProps {
  /** Injected by tests; defaults to the `vitals://frame` event channel. */
  readonly source?: FrameSource;
  /** Injected by tests; defaults to the real command bridge. */
  readonly bridge?: WidgetBridge;
  /** Injected by tests; defaults to this Tauri window. */
  readonly window?: WidgetWindow;
  /** The widget's id. Read from the window label when omitted. */
  readonly id?: string;
  readonly locale?: string;
}

/**
 * A pointer that moved further than this between press and release was a drag,
 * not a click. In logical pixels — the same unit Tauri reports window
 * positions in — and small enough that a deliberate click never trips it.
 */
const DRAG_SLOP = 4;

export function WidgetApp({
  source,
  bridge = tauriWidgetBridge,
  window: win = widgetWindow,
  id,
  locale,
}: WidgetAppProps = {}): React.JSX.Element {
  const { t, i18n } = useTranslation();

  // Resolved once. Reading the label needs the window module, which is a
  // dynamic import, so the first render has no id and shows the placeholder.
  const [resolvedId, setResolvedId] = useState<string | null>(id ?? null);
  useEffect(() => {
    if (id !== undefined) return;
    let live = true;
    void win.label().then((label) => {
      if (live) setResolvedId(label === null ? null : widgetIdFromLabel(label));
    });
    return () => {
      live = false;
    };
  }, [id, win]);

  const [spec, setSpec] = useState<WidgetInstance | null>(null);
  // The flags the backend last told this window about, or `null` before it has
  // said anything. Written from the command answer and the config echo rather
  // than mirrored in an effect: the widget is the only place that can flip a
  // pin on a window nobody else is looking at, and a copy refreshed by an
  // effect would re-render the whole panel a second time for every echo.
  const [flags, setFlags] = useState<WidgetFlags | null>(null);
  useEffect(() => {
    if (resolvedId === null) return;
    let live = true;
    // Asked for as well as subscribed to: the backend emits the spec when it
    // creates the window, but that event can arrive before this effect runs
    // (the window is created, then the webview boots), and a widget that waits
    // for a frame it already missed shows em dashes forever.
    void bridge.get(resolvedId).then((found) => {
      if (!live) return;
      setSpec(found);
      if (found !== null) {
        setFlags({ alwaysOnTop: found.alwaysOnTop, clickThrough: found.clickThrough });
      }
    });
    const dispose = bridge.onConfig((next) => {
      setSpec(next);
      setFlags({ alwaysOnTop: next.alwaysOnTop, clickThrough: next.clickThrough });
    });
    return () => {
      live = false;
      dispose();
    };
  }, [bridge, resolvedId]);

  // Optimistic until the backend echoes its own truth back through `onConfig`.
  // The defaults are the model's, so a window with no host (a browser preview)
  // opens pinned and clickable and its toggles still move.
  const pinned = flags?.alwaysOnTop ?? true;
  const clickThrough = flags?.clickThrough ?? false;

  const [sticky, setSticky] = useState(false);
  const [hovered, setHovered] = useState(false);
  const expanded = sticky || hovered;

  const input = useFrameWidgetInput(source);
  const description = useMemo(
    () => (spec === null ? null : describe(spec.kind, input, locale ?? i18n.language)),
    [spec, input, locale, i18n.language],
  );

  const patch = useCallback(
    (next: { alwaysOnTop: boolean; clickThrough: boolean }) => {
      setFlags(next);
      if (resolvedId !== null) void bridge.setFlags(resolvedId, next);
      // The window itself follows immediately rather than waiting for the
      // round trip: a pin that lags a frame reads as a broken button.
      void win.setAlwaysOnTop(next.alwaysOnTop);
      void win.setIgnoreCursorEvents(next.clickThrough);
    },
    [bridge, resolvedId, win],
  );

  const togglePin = useCallback(() => {
    patch({ alwaysOnTop: !pinned, clickThrough });
  }, [patch, pinned, clickThrough]);

  const toggleClickThrough = useCallback(() => {
    patch({ alwaysOnTop: pinned, clickThrough: !clickThrough });
  }, [patch, pinned, clickThrough]);

  const pressAt = useRef<{ x: number; y: number } | null>(null);
  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      if (event.button !== 0) return;
      if ((event.target as HTMLElement).closest('button') !== null) return;
      pressAt.current = { x: event.clientX, y: event.clientY };
      // The explicit call is what makes the panel draggable at all: the
      // `data-tauri-drag-region` attribute only covers the element it is on and
      // is ignored while a webview devtools session is attached.
      void win.startDragging();
    },
    [win],
  );

  const onClick = useCallback((event: React.MouseEvent<HTMLDivElement>) => {
    const start = pressAt.current;
    pressAt.current = null;
    // A drag ends with a synthetic click; toggling on it would collapse the
    // widget every time it was moved.
    if (
      start !== null &&
      Math.abs(event.clientX - start.x) + Math.abs(event.clientY - start.y) > DRAG_SLOP
    ) {
      return;
    }
    setSticky((current) => !current);
  }, []);

  if (resolvedId === null || spec === null || description === null) {
    return (
      <Shell>
        <div className="flex h-full w-full items-center justify-center rounded-xl border border-[rgb(255_255_255/0.08)] bg-[rgb(20_20_24/0.92)] px-3 text-center text-xs text-[var(--color-fg-muted)]">
          {t('widgets.unavailable')}
        </div>
      </Shell>
    );
  }

  return (
    <Shell>
      <div
        data-tauri-drag-region
        onPointerDown={onPointerDown}
        onClick={onClick}
        onPointerEnter={() => setHovered(true)}
        onPointerLeave={() => setHovered(false)}
        onFocusCapture={() => setHovered(true)}
        onBlurCapture={() => setHovered(false)}
        className="group relative h-full w-full"
      >
        <WidgetPreview spec={spec} description={description} expanded={expanded} />

        {/* A click-through window is visually identical to one that is not, and
            its clicks land on whatever is behind it, so this ring is the only
            signal the user gets. */}
        {clickThrough && (
          <span
            aria-hidden
            className="pointer-events-none absolute inset-0 rounded-xl ring-1 ring-[var(--color-accent)]"
          />
        )}

        <div
          className={cn(
            'absolute top-1 right-1 flex items-center gap-0.5',
            // Opacity, never `display: none`: the toolbar has to stay in the
            // tab order, or the only way out of click-through from the widget
            // side would be a mouse the window is ignoring.
            'opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100 motion-reduce:transition-none',
          )}
        >
          <ToolbarButton
            label={pinned ? t('widgets.toolbar.unpin') : t('widgets.toolbar.pin')}
            pressed={pinned}
            onClick={togglePin}
          >
            {pinned ? (
              <Pin aria-hidden className="size-3" />
            ) : (
              <PinOff aria-hidden className="size-3" />
            )}
          </ToolbarButton>
          <ToolbarButton
            label={
              clickThrough
                ? t('widgets.toolbar.clickThroughOff')
                : t('widgets.toolbar.clickThrough')
            }
            title={
              clickThrough
                ? t('widgets.toolbar.clickThroughHint')
                : t('widgets.toolbar.clickThrough')
            }
            pressed={clickThrough}
            onClick={toggleClickThrough}
          >
            {clickThrough ? (
              <MousePointerBan aria-hidden className="size-3" />
            ) : (
              <MousePointer2 aria-hidden className="size-3" />
            )}
          </ToolbarButton>
          <ToolbarButton
            label={t('widgets.toolbar.settings')}
            onClick={() => void bridge.openSettings(resolvedId)}
          >
            <Settings aria-hidden className="size-3" />
          </ToolbarButton>
          <ToolbarButton
            label={t('widgets.toolbar.close')}
            onClick={() => void bridge.close(resolvedId)}
          >
            <X aria-hidden className="size-3" />
          </ToolbarButton>
        </div>
      </div>
    </Shell>
  );
}

/**
 * The Motion runtime, installed by the window rather than by the app.
 *
 * A widget window is its own webview and never mounts `MotionProvider`, so
 * without this the `m.*` components inside `WidgetPreview` would throw. The
 * feature set is `domAnimation` and `strict` keeps it that way (ADR-0017) —
 * which is also why the morph animates height and opacity rather than using
 * the `layout` prop, as layout animations need the larger set.
 *
 * Reduced motion is read from the operating system here, not from the app's own
 * three-state setting: the capability grants this window no store access, and
 * the OS preference is the honest approximation for a window that cannot ask.
 * `useReducedMotion` in the preview reads the same source, so the two agree.
 */
function Shell({ children }: { readonly children: ReactNode }): React.JSX.Element {
  return (
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">{children}</MotionConfig>
    </LazyMotion>
  );
}

/**
 * One toolbar button.
 *
 * Six logical pixels of target around a three-pixel glyph: the glyph is what
 * the eye reads, the target is what the hand has to hit, and a widget is often
 * the smallest thing on a 4K screen.
 */
function ToolbarButton({
  label,
  title,
  pressed,
  onClick,
  children,
}: {
  readonly label: string;
  readonly title?: string | undefined;
  readonly pressed?: boolean | undefined;
  readonly onClick: () => void;
  readonly children: ReactNode;
}): React.JSX.Element {
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={pressed}
      title={title ?? label}
      onClick={onClick}
      className={cn(
        'grid size-6 place-items-center rounded-md text-[var(--color-fg-muted)]',
        'transition-colors motion-reduce:transition-none',
        'hover:bg-[rgb(255_255_255/0.10)] hover:text-[var(--color-fg-default)]',
        'focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--color-accent)]',
        pressed === true && 'text-[var(--color-accent)]',
      )}
    >
      {children}
    </button>
  );
}
