import { useEffect, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { EyeOff, Maximize2, Minimize2, Minus, Power, X } from 'lucide-react';

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
  cn,
  focusRing,
} from '@vitals/ui';

import { SHELL_NS } from './strings';
import { getWindowControls, type WindowControls } from './windowControls';

/**
 * Height of the caption bar, in pixels.
 *
 * 32px matches the Windows 11 title bar so the window does not read as
 * subtly wrong next to a native one, and it is the height the standard
 * 46×32 caption buttons are designed for.
 */
export const TITLE_BAR_HEIGHT = 32;

interface CaptionButtonProps {
  readonly label: string;
  readonly onClick: () => void;
  readonly children: ReactNode;
  readonly danger?: boolean;
}

/**
 * A caption button.
 *
 * Not the shared `IconButton`: caption buttons are a platform convention, not
 * an app control. Windows draws them as full-bleed 46×32 rectangles with no
 * radius and no gap, flush to the window edge, and close turns red. Reusing
 * the rounded app button here would be immediately recognisable as "not a real
 * window".
 *
 * The buttons sit outside the drag region rather than opting out with
 * `data-tauri-drag-region` inverse markup, because a drag region swallows the
 * initial mousedown and makes the first click on a caption button feel dead.
 */
function CaptionButton({ label, onClick, children, danger = false }: CaptionButtonProps) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      className={cn(
        'inline-flex h-8 w-[46px] shrink-0 items-center justify-center',
        'text-[var(--color-fg-muted)]',
        'transition-colors duration-(--duration-instant)',
        danger
          ? 'hover:bg-[var(--color-status-danger)] hover:text-[var(--color-fg-on-accent)]'
          : 'hover:bg-[var(--color-bg-inset)] hover:text-[var(--color-fg-default)]',
        // Inset so the ring is not clipped by the window edge, where the
        // default 2px offset would put half of it outside the client area.
        focusRing,
        'focus-visible:-outline-offset-2',
      )}
    >
      {children}
    </button>
  );
}

/**
 * Caption glyphs.
 *
 * Drawn inline rather than imported from lucide so they match Segoe Fluent
 * Icons' geometry: 10×10 at a 1px stroke, which is what every other window on
 * the desktop uses. lucide's stroke is 2px at 24px and reads noticeably
 * heavier next to a native window.
 */
const glyph = {
  minimise: <path d="M0 5.5h11" />,
  maximise: <rect x="0.5" y="0.5" width="10" height="10" rx="0.5" />,
  restore: (
    <>
      <rect x="0.5" y="3" width="7.5" height="7.5" rx="0.5" />
      <path d="M3 3V0.5h7.5V8H8" />
    </>
  ),
  close: <path d="M0.5 0.5l10 10M10.5 0.5l-10 10" />,
} as const;

function Glyph({ children }: { readonly children: ReactNode }) {
  return (
    <svg
      viewBox="-0.5 -0.5 12 12"
      className="size-[11px]"
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      strokeWidth="1"
    >
      {children}
    </svg>
  );
}

export interface TitleBarProps {
  /** Rendered in the centre of the bar. Typically the active section name. */
  readonly children?: ReactNode;
  /** Injected in tests; defaults to the real Tauri window. */
  readonly controls?: WindowControls;
}

export function TitleBar({ children, controls }: TitleBarProps) {
  const { t } = useTranslation(SHELL_NS);
  const [api] = useState<WindowControls>(() => controls ?? getWindowControls());
  const [maximised, setMaximised] = useState(false);

  // Maximise state is observed, not assumed. A user can maximise by
  // double-clicking the drag region, dragging to the top edge, or pressing
  // Win+Up — none of which go through our button, and all of which would
  // otherwise leave the glyph showing the wrong action.
  useEffect(() => {
    let live = true;

    void api.isMaximized().then((value) => {
      if (live) setMaximised(value);
    });

    const unlisten = api.onResized(() => {
      void api.isMaximized().then((value) => {
        if (live) setMaximised(value);
      });
    });

    return () => {
      live = false;
      void unlisten.then((off) => off());
    };
  }, [api]);

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>
        <header
          className="shell-chrome relative flex shrink-0 items-center"
          style={{ height: TITLE_BAR_HEIGHT }}
        >
          {/*
           * The drag region is a sibling layer rather than the header itself so
           * that adding a control to the bar can never accidentally make it
           * draggable — a draggable button does not fire click reliably, and the
           * bug looks like a broken button rather than a layout mistake.
           */}
          <div data-tauri-drag-region className="absolute inset-0" />

          <div className="pointer-events-none relative flex h-full min-w-0 flex-1 items-center gap-2 px-3">
            {/*
             * A mark rather than bare text: the accent dot breathes while the
             * window is open, which is the product's one ambient sign of life.
             * Decorative — the section name beside it is the accessible content.
             */}
            <span
              aria-hidden="true"
              className="vitals-live-dot size-1.5 shrink-0 rounded-full bg-[var(--color-accent)] text-[var(--color-accent)]"
            />
            <span className="truncate text-2xs font-medium text-[var(--color-fg-muted)]">
              {children}
            </span>
          </div>

          <div className="relative flex h-full items-center">
            <CaptionButton label={t('window.minimise')} onClick={() => void api.minimize()}>
              <Glyph>{glyph.minimise}</Glyph>
            </CaptionButton>
            <CaptionButton
              label={maximised ? t('window.restore') : t('window.maximise')}
              onClick={() => void api.toggleMaximize()}
            >
              <Glyph>{maximised ? glyph.restore : glyph.maximise}</Glyph>
            </CaptionButton>
            <CaptionButton danger label={t('window.close')} onClick={() => void api.close()}>
              <Glyph>{glyph.close}</Glyph>
            </CaptionButton>
          </div>
        </header>
      </ContextMenuTrigger>
      {/*
       * The window's own menu on right-click, as every Windows title bar has.
       * A frameless window loses the system menu, and without this the
       * webview's Back/Reload/Inspect menu appeared in its place. Quit is here
       * because the × hides to the tray, and the tray is the only other way out.
       */}
      <ContextMenuContent className="min-w-52" aria-label={t('window.menu')}>
        <ContextMenuItem onSelect={() => void api.minimize()}>
          <Minus className="size-4" aria-hidden="true" />
          {t('window.minimise')}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => void api.toggleMaximize()}>
          {maximised ? (
            <Minimize2 className="size-4" aria-hidden="true" />
          ) : (
            <Maximize2 className="size-4" aria-hidden="true" />
          )}
          {maximised ? t('window.restore') : t('window.maximise')}
        </ContextMenuItem>
        <ContextMenuItem onSelect={() => void api.hide()}>
          <EyeOff className="size-4" aria-hidden="true" />
          {t('window.hideToTray')}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onSelect={() => void api.close()}>
          <X className="size-4" aria-hidden="true" />
          {t('window.close')}
        </ContextMenuItem>
        <ContextMenuItem destructive onSelect={() => void api.quit()}>
          <Power className="size-4" aria-hidden="true" />
          {t('window.quit')}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}
