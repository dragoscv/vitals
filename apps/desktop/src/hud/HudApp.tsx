import { Pin, PinOff, MousePointer2, MousePointerBan, X } from 'lucide-react';
import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { gpuPercent, memoryPercent } from '@vitals/protocol';
import { cn } from '@vitals/ui';

import { HudRow } from './components/HudRow';
import { useHudState, type FrameSource } from './lib/live';
import { hudWindow, type HudWindow } from './lib/window';

export interface HudAppProps {
  /** Injected by tests; defaults to the `vitals://frame` event channel. */
  readonly source?: FrameSource;
  /** Injected by tests; defaults to this Tauri window. */
  readonly window?: HudWindow;
  readonly locale?: string;
}

/**
 * The always-on-top overlay.
 *
 * Three readings and nothing else. Every addition to this window costs the
 * user screen space they did not choose to give up, so anything that can be
 * read in the main window belongs there instead.
 */
export function HudApp({ source, window: win = hudWindow, locale = 'en' }: HudAppProps = {}) {
  const { t } = useTranslation();
  const state = useHudState(source);
  const [pinned, setPinned] = useState(true);
  const [clickThrough, setClickThrough] = useState(false);

  const system = state.system;
  const cpu = system?.cpu ?? null;

  const togglePin = useCallback(() => {
    const next = !pinned;
    setPinned(next);
    void win.setAlwaysOnTop(next);
  }, [pinned, win]);

  const toggleClickThrough = useCallback(() => {
    const next = !clickThrough;
    setClickThrough(next);
    void win.setIgnoreCursorEvents(next);
  }, [clickThrough, win]);

  const onPointerDown = useCallback(
    (event: React.PointerEvent<HTMLDivElement>) => {
      // `data-tauri-drag-region` handles this in a packaged build, but it only
      // applies to the element it is on and is ignored while a webview
      // devtools session is attached. Starting the drag explicitly makes the
      // whole panel draggable in both cases; the guards keep a toolbar click
      // and a right-click menu from being swallowed by it.
      if (event.button !== 0) return;
      if ((event.target as HTMLElement).closest('button') !== null) return;
      void win.startDragging();
    },
    [win],
  );

  return (
    <div
      data-tauri-drag-region
      onPointerDown={onPointerDown}
      // `group` drives the hover-revealed toolbar. rgb(…/0.82) rather than a
      // backdrop-filter: blur on a transparent Windows webview is expensive
      // and renders inconsistently over a fullscreen game, which is where an
      // overlay earns its keep.
      className={cn(
        'flex h-full w-full flex-col justify-center gap-1.5 rounded-xl px-3 py-2',
        'group bg-[rgb(20_20_24/0.82)] text-[var(--color-fg-default)]',
        clickThrough
          ? // A window that ignores the cursor looks identical to one that
            // does not. The border is the only signal the user has that their
            // clicks are landing on whatever is behind it.
            'border border-[var(--color-accent)]'
          : 'border border-[rgb(255_255_255/0.08)]',
      )}
    >
      <div className="flex items-center justify-between">
        <span className="text-[10px] leading-none font-semibold tracking-wide text-[var(--color-fg-muted)]">
          {t('hud.title')}
        </span>
        <div
          className={cn(
            'flex items-center gap-0.5',
            // Hidden until hover so the overlay is three numbers at rest, but
            // never hidden from assistive technology or the keyboard —
            // opacity keeps it focusable, `display: none` would not.
            'opacity-0 transition-opacity duration-150 group-hover:opacity-100 focus-within:opacity-100',
            'motion-reduce:transition-none',
          )}
        >
          <ToolbarButton
            label={pinned ? t('hud.unpin') : t('hud.pin')}
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
            label={clickThrough ? t('hud.clickThroughOff') : t('hud.clickThrough')}
            title={clickThrough ? t('hud.clickThroughHint') : undefined}
            pressed={clickThrough}
            onClick={toggleClickThrough}
          >
            {clickThrough ? (
              <MousePointerBan aria-hidden className="size-3" />
            ) : (
              <MousePointer2 aria-hidden className="size-3" />
            )}
          </ToolbarButton>
          <ToolbarButton label={t('hud.close')} onClick={() => void win.close()}>
            <X aria-hidden className="size-3" />
          </ToolbarButton>
        </div>
      </div>

      <HudRow
        label={t('metric.cpu')}
        percent={cpu?.total ?? null}
        history={state.cpuHistory}
        stroke="var(--color-chart-cpu)"
        locale={locale}
        temperature={cpu?.temperature ?? null}
      />
      <HudRow
        label={t('metric.memory')}
        percent={system === null ? null : memoryPercent(system)}
        history={state.memoryHistory}
        stroke="var(--color-chart-memory)"
        locale={locale}
      />
      <HudRow
        label={t('metric.gpu')}
        percent={system === null ? null : gpuPercent(system)}
        history={state.gpuHistory}
        stroke="var(--color-chart-gpu)"
        locale={locale}
      />
    </div>
  );
}

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
  readonly children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      {...(title !== undefined && { title })}
      {...(pressed !== undefined && { 'aria-pressed': pressed })}
      onClick={onClick}
      className={cn(
        'grid size-5 place-items-center rounded text-[var(--color-fg-muted)]',
        'hover:bg-[rgb(255_255_255/0.1)] hover:text-[var(--color-fg-default)]',
        'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--color-accent)]',
      )}
    >
      {children}
    </button>
  );
}
