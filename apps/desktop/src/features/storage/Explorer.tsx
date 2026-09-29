/**
 * Navigating a finished scan: a map, a breadcrumb and a list, in one card.
 *
 * # The tree stays in Rust
 *
 * The webview never holds the tree. It asks for one folder at a time
 * (`children`) and for a laid-out map of it (`map`), which the backend caps at
 * 5,000 rectangles with anything too small to see folded into one. A full `C:`
 * scan is 1.9 million folders; this screen touches a few thousand of them.
 *
 * # The map is a picture; the list is the interface
 *
 * The canvas is decoration for a keyboard or screen-reader user and says so
 * in its label. Every action it offers — open a folder, go up, reveal, copy —
 * is also on the list below it, which is a real list of buttons. The map is
 * how a sighted mouse user finds the big thing; the list is how anyone acts
 * on it.
 *
 * # Zoom is a viewport, not a re-layout
 *
 * Opening a folder first animates the viewport onto that folder's rectangle
 * in the current map (so the eye follows it), then swaps in the new layout,
 * which by construction fills the same frame. Reduced motion skips the tween.
 */

import { Check, ChevronRight, ChevronUp, FileText, Folder, Layers, Plus } from 'lucide-react';
import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { resizeCanvas } from '@vitals/charts';
import {
  Badge,
  Button,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
  EmptyState,
  IconButton,
  SegmentedControl,
  formatBytes,
  formatCount,
  useReducedMotion,
} from '@vitals/ui';
import { useVirtualizer } from '@tanstack/react-virtual';

import { errorMessage, isCommandError } from '../../lib/commandError';
import {
  FULL_VIEW,
  drawMap,
  hitTest,
  lerpView,
  openTarget,
  type MapPalette,
  type Viewport,
} from './mapDraw';
import type {
  BasketItem,
  LargeFile,
  MapCell,
  MapShape,
  ScanSnapshot,
  StorageListing,
  StorageNode,
} from './model';
import { inBasket, leafName } from './model';
import { STORAGE_NS } from './strings';
import type { StorageSource } from './useStorage';

export const exploreViews = ['icicle', 'treemap', 'list', 'largest', 'files'] as const;
export type ExploreView = (typeof exploreViews)[number];

const ZOOM_MS = 260;
const ROW_HEIGHT = 36;

/**
 * A measured size of 0 is "not laid out yet", not "no room" (same reasoning
 * as `ProcessTable`): taking it literally renders an empty list that nothing
 * re-measures, which is what a headless test and a window restored from
 * minimised both see.
 */
function observeRect(
  instance: { scrollElement: Element | null | undefined },
  cb: (rect: { width: number; height: number }) => void,
): (() => void) | undefined {
  const element = instance.scrollElement;
  if (element === null || element === undefined) return undefined;
  const measure = (): void => {
    const rect = element.getBoundingClientRect();
    cb({ width: rect.width > 0 ? rect.width : 800, height: rect.height > 0 ? rect.height : 400 });
  };
  measure();
  const observer = new ResizeObserver(measure);
  observer.observe(element);
  return () => {
    observer.disconnect();
  };
}

export interface ExplorerProps {
  readonly source: StorageSource;
  readonly snapshot: ScanSnapshot;
  readonly locale: string;
  readonly view: ExploreView;
  readonly onViewChange: (view: ExploreView) => void;
  /** The flat "largest folders anywhere" table, rendered for that view. */
  readonly largest: ReactNode;
  /** What is in review for the Recycle Bin. */
  readonly basket: readonly BasketItem[];
  /** Adds the item when absent, removes it when present. */
  readonly onToggleBasket: (item: BasketItem) => void;
  /** Bumped when the kept scan changed; the folder in view is re-read. */
  readonly revision: number;
}

/** `parent\name`, without doubling the separator after a drive root. */
function childPath(parent: string, name: string): string {
  return parent.endsWith('\\') ? `${parent}${name}` : `${parent}\\${name}`;
}

/**
 * The add/remove control on a row. A separate button beside the row, not
 * inside it: the row is itself a button, and nesting one in another is
 * invalid and unreachable by keyboard.
 */
function BasketToggle({
  item,
  selected,
  onToggle,
}: {
  readonly item: BasketItem;
  readonly selected: boolean;
  readonly onToggle: (item: BasketItem) => void;
}) {
  const { t } = useTranslation(STORAGE_NS);
  return (
    <IconButton
      size="sm"
      variant={selected ? 'primary' : 'ghost'}
      aria-pressed={selected}
      label={
        selected
          ? t('basket.removeItem', { name: item.name })
          : t('basket.addItem', { name: item.name })
      }
      title={selected ? t('basket.inBasket') : t('basket.add')}
      icon={selected ? <Check aria-hidden /> : <Plus aria-hidden />}
      onClick={() => {
        onToggle(item);
      }}
    />
  );
}

/**
 * Keyed by scan, so a new scan starts at its own root with fresh state: a
 * node id means nothing in another scan's tree.
 */
export function Explorer(props: ExplorerProps) {
  return <ScanExplorer key={props.snapshot.scanId} {...props} />;
}

function ScanExplorer({
  source,
  snapshot,
  locale,
  view,
  onViewChange,
  largest,
  basket,
  onToggleBasket,
  revision,
}: ExplorerProps) {
  const { t } = useTranslation(STORAGE_NS);
  const [focus, setFocus] = useState(snapshot.rootNode);
  const [listing, setListing] = useState<StorageListing | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [released, setReleased] = useState(false);

  const fail = useCallback((cause: unknown) => {
    // `not-found` is what a released or replaced scan answers; it is a state
    // to explain, not an error to show in red.
    if (isCommandError(cause) && cause.kind === 'not-found') setReleased(true);
    else setError(errorMessage(cause));
  }, []);

  useEffect(() => {
    let live = true;
    source
      .children(snapshot.scanId, focus)
      .then((next) => {
        if (!live) return;
        setListing(next);
        setError(null);
      })
      .catch((cause: unknown) => {
        if (!live) return;
        // The folder in view was itself recycled: go to the nearest one
        // that is still in the scan rather than showing an error.
        if (isCommandError(cause) && cause.kind === 'not-found' && focus !== snapshot.rootNode) {
          setFocus(snapshot.rootNode);
          return;
        }
        fail(cause);
      });
    return () => {
      live = false;
    };
  }, [source, snapshot.scanId, snapshot.rootNode, focus, fail, revision]);

  const reveal = useCallback(
    (path: string) => {
      source.reveal(path).catch(fail);
    },
    [source, fail],
  );

  const here = listing?.ancestry.at(-1) ?? null;
  const parent = listing !== null && listing.ancestry.length > 1 ? listing.ancestry.at(-2) : null;

  if (released) {
    return <p className="text-2xs text-[var(--color-fg-muted)]">{t('explore.released')}</p>;
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <div className="flex flex-wrap items-center gap-2">
        <SegmentedControl
          value={view}
          ariaLabel={t('explore.views')}
          onValueChange={onViewChange}
          options={exploreViews.map((id) => ({ value: id, label: t(`explore.view.${id}`) }))}
        />
      </div>

      {view !== 'files' && view !== 'largest' && listing !== null && (
        <Breadcrumb
          ancestry={listing.ancestry}
          onOpen={setFocus}
          upLabel={t('explore.up')}
          label={t('explore.breadcrumb')}
          {...(parent !== null && parent !== undefined && { up: parent.node })}
        />
      )}

      {error !== null && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {t('explore.failed', { message: error })}
        </p>
      )}

      {view === 'largest' ? (
        largest
      ) : view === 'files' ? (
        <FileList
          files={snapshot.largestFiles}
          locale={locale}
          onReveal={reveal}
          basket={basket}
          onToggleBasket={onToggleBasket}
        />
      ) : (
        <>
          {(view === 'icicle' || view === 'treemap') && here !== null && (
            <MapCanvas
              source={source}
              scanId={snapshot.scanId}
              focus={focus}
              shape={view}
              here={here}
              locale={locale}
              onOpen={setFocus}
              onFail={fail}
              basket={basket}
              onToggleBasket={onToggleBasket}
              revision={revision}
            />
          )}
          {listing !== null && here !== null && (
            <FolderList
              here={here}
              items={listing.children}
              locale={locale}
              onOpen={setFocus}
              onReveal={reveal}
              compact={view !== 'list'}
              basket={basket}
              onToggleBasket={onToggleBasket}
            />
          )}
        </>
      )}
    </div>
  );
}

function Breadcrumb({
  ancestry,
  onOpen,
  up,
  upLabel,
  label,
}: {
  readonly ancestry: readonly StorageNode[];
  readonly onOpen: (node: number) => void;
  readonly up?: number;
  readonly upLabel: string;
  readonly label: string;
}) {
  return (
    <nav aria-label={label} className="flex min-w-0 items-center gap-1 text-2xs">
      <Button
        variant="ghost"
        size="sm"
        disabled={up === undefined}
        aria-label={upLabel}
        title={upLabel}
        onClick={() => {
          if (up !== undefined) onOpen(up);
        }}
      >
        <ChevronUp aria-hidden className="size-4" />
      </Button>
      <ol className="flex min-w-0 flex-wrap items-center gap-0.5">
        {ancestry.map((node, index) => {
          const last = index === ancestry.length - 1;
          return (
            <li key={node.node} className="flex min-w-0 items-center gap-0.5">
              {index > 0 && (
                <ChevronRight aria-hidden className="size-3 text-[var(--color-fg-subtle)]" />
              )}
              {last ? (
                <span aria-current="location" className="truncate font-medium">
                  {node.name}
                </span>
              ) : (
                <button
                  type="button"
                  className="truncate rounded px-1 text-[var(--color-fg-muted)] hover:text-[var(--color-fg-default)] focus-visible:outline-2 focus-visible:outline-[var(--color-accent)]"
                  onClick={() => {
                    onOpen(node.node);
                  }}
                >
                  {node.name}
                </button>
              )}
            </li>
          );
        })}
      </ol>
    </nav>
  );
}

/** Reads the palette from CSS so light, dark and high contrast just work. */
function readPalette(element: HTMLElement): MapPalette {
  const style = getComputedStyle(element);
  const v = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback;
  return {
    branches: [
      v('--color-chart-cpu', '#4f8cff'),
      v('--color-chart-memory', '#a06cff'),
      v('--color-chart-disk', '#28b487'),
      v('--color-chart-network', '#f0a030'),
      v('--color-chart-gpu', '#e0507a'),
      v('--color-chart-power', '#30b0c8'),
    ],
    files: v('--color-chart-neutral', '#8a8f98'),
    smaller: v('--color-border-subtle', '#40444c'),
    warn: v('--color-status-warn', '#e0a020'),
    edge: v('--color-bg-canvas', v('--color-bg-default', '#111')),
    text: v('--color-fg-on-accent', '#fff'),
    hover: v('--color-fg-default', '#fff'),
  };
}

function MapCanvas({
  source,
  scanId,
  focus,
  shape,
  here,
  locale,
  onOpen,
  onFail,
  basket,
  onToggleBasket,
  revision,
}: {
  readonly source: StorageSource;
  readonly scanId: number;
  readonly focus: number;
  readonly shape: MapShape;
  readonly here: StorageNode;
  readonly locale: string;
  readonly onOpen: (node: number) => void;
  readonly onFail: (cause: unknown) => void;
  readonly basket: readonly BasketItem[];
  readonly onToggleBasket: (item: BasketItem) => void;
  readonly revision: number;
}) {
  const { t } = useTranslation(STORAGE_NS);
  const reduced = useReducedMotion();
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const [cells, setCells] = useState<readonly MapCell[]>([]);
  const [hovered, setHovered] = useState<MapCell | null>(null);
  const [viewport, setViewport] = useState<Viewport>(FULL_VIEW);
  const [size, setSize] = useState({ width: 0, height: 0 });
  const animation = useRef<number | null>(null);

  // Track the element's size so the map re-lays out (treemap) and redraws.
  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas === null) return undefined;
    const measure = () => {
      const rect = canvas.getBoundingClientRect();
      setSize({ width: Math.floor(rect.width), height: Math.floor(rect.height) });
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(canvas);
    return () => {
      observer.disconnect();
    };
  }, []);

  // A treemap depends on the aspect ratio; rounded so a one-pixel resize
  // does not trigger a round trip.
  const aspect =
    shape === 'treemap' && size.height > 0 ? Math.round((size.width / size.height) * 20) / 20 : 1;

  useEffect(() => {
    let live = true;
    source
      .map(scanId, focus, shape, aspect)
      .then((next) => {
        if (!live) return;
        setCells(next);
        setViewport(FULL_VIEW);
        setHovered(null);
      })
      .catch((cause: unknown) => {
        if (live) onFail(cause);
      });
    return () => {
      live = false;
    };
  }, [source, scanId, focus, shape, aspect, onFail, revision]);

  const label = useCallback(
    (cell: MapCell): string | null => {
      if (cell.kind === 'directory') return cell.name;
      if (cell.kind === 'smaller')
        return t('explore.smaller', { count: cell.count, n: formatCount(cell.count, locale) });
      return t('explore.ownFiles', { count: cell.count, n: formatCount(cell.count, locale) });
    },
    [t, locale],
  );

  useEffect(() => {
    const canvas = canvasRef.current;
    if (canvas === null || size.width === 0) return;
    const { width, height, ctx } = resizeCanvas(canvas);
    if (ctx === null) return;
    drawMap(ctx, {
      cells,
      shape,
      view: viewport,
      width,
      height,
      palette: readPalette(canvas),
      hovered,
      label,
    });
  }, [cells, shape, viewport, size, hovered, label]);

  useEffect(
    () => () => {
      if (animation.current !== null) cancelAnimationFrame(animation.current);
    },
    [],
  );

  const open = (cell: MapCell) => {
    if (cell.kind !== 'directory' || !cell.openable || cell.depth === 0) return;
    if (reduced) {
      onOpen(cell.node);
      return;
    }
    // In an icicle the zoom widens the column; the rows stay rows.
    const target: Viewport =
      shape === 'icicle'
        ? { x0: cell.x0, x1: cell.x1, y0: cell.y0, y1: 1 + cell.y0 }
        : { x0: cell.x0, x1: cell.x1, y0: cell.y0, y1: cell.y1 };
    const from = viewport;
    const start = performance.now();
    const step = (now: number) => {
      const k = Math.min(1, (now - start) / ZOOM_MS);
      // Ease-out cubic: fast start, gentle landing.
      setViewport(lerpView(from, target, 1 - (1 - k) ** 3));
      if (k < 1) animation.current = requestAnimationFrame(step);
      else {
        animation.current = null;
        onOpen(cell.node);
      }
    };
    animation.current = requestAnimationFrame(step);
  };

  const pointAt = (event: React.PointerEvent<HTMLCanvasElement> | React.MouseEvent) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return hitTest(
      cells,
      viewport,
      rect.width,
      rect.height,
      event.clientX - rect.left,
      event.clientY - rect.top,
    );
  };

  const tip =
    hovered === null
      ? null
      : `${label(hovered) ?? ''} · ${formatBytes(hovered.allocated, locale)} · ${t(
          'explore.share',
          {
            percent: `${Math.round((hovered.allocated / Math.max(1, here.allocated)) * 1000) / 10} %`,
            parent: here.name,
          },
        )}${
          hovered.kind === 'directory' && hovered.depth > 0
            ? ` · ${t(
                inBasket(basket, childPath(here.path, hovered.name ?? ''))
                  ? 'basket.mapInBasket'
                  : 'basket.mapHint',
              )}`
            : ''
        }`;

  return (
    // Flexes to the card's height rather than a fixed one: a fixed height
    // overflowed the card at 1440x900, and the Clean-up card below covered
    // the map, which then swallowed no clicks at all (found live).
    <div className="relative min-h-32 flex-[3_1_0%]">
      <canvas
        ref={canvasRef}
        role="img"
        aria-label={t('explore.mapLabel', { path: here.path })}
        title={tip ?? undefined}
        className="size-full cursor-pointer rounded-md"
        onPointerMove={(event) => {
          const next = pointAt(event);
          if (next !== hovered) setHovered(next);
        }}
        onPointerLeave={() => {
          setHovered(null);
        }}
        onClick={(event) => {
          // A click on a file block or a leaf folder opens the folder that
          // holds it rather than doing nothing (see `openTarget`).
          const hit = pointAt(event);
          const target = hit === null ? null : openTarget(cells, shape, hit);
          if (target !== null) open(target);
        }}
        onContextMenu={(event) => {
          // Right-click puts the folder under the pointer in review. Only
          // direct children: a deeper cell's path is not known here without
          // a round trip, and the list below offers every folder anyway.
          const hit = pointAt(event);
          if (hit === null || hit.kind !== 'directory' || hit.depth !== 1 || hit.name === null)
            return;
          event.preventDefault();
          onToggleBasket({
            path: childPath(here.path, hit.name),
            name: hit.name,
            kind: 'folder',
            allocated: hit.allocated,
          });
        }}
      />
      {tip !== null && (
        <p
          aria-hidden
          className="pointer-events-none absolute bottom-1 left-1 max-w-[90%] truncate rounded bg-[var(--color-bg-elevated,rgba(0,0,0,.7))] px-2 py-0.5 text-2xs"
        >
          {tip}
        </p>
      )}
    </div>
  );
}

function FolderList({
  here,
  items,
  locale,
  onOpen,
  onReveal,
  compact,
  basket,
  onToggleBasket,
}: {
  readonly here: StorageNode;
  readonly items: readonly StorageNode[];
  readonly locale: string;
  readonly onOpen: (node: number) => void;
  readonly onReveal: (path: string) => void;
  readonly compact: boolean;
  readonly basket: readonly BasketItem[];
  readonly onToggleBasket: (item: BasketItem) => void;
}) {
  const { t } = useTranslation(STORAGE_NS);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const total = Math.max(1, here.allocated);

  // Deliberate: see ProcessTable for why the compiler skips this component.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
    initialRect: { width: 800, height: 400 },
    observeElementRect: observeRect,
    getItemKey: (index) => items[index]?.node ?? index,
  });

  if (items.length === 0 && here.ownFiles === 0) {
    return <EmptyState title={t('explore.emptyFolder')} />;
  }

  return (
    <div className={`flex min-h-24 flex-col gap-1 ${compact ? 'flex-[2_1_0%]' : 'flex-1'}`}>
      {here.ownFiles > 0 && (
        <p className="flex items-center gap-1.5 text-2xs text-[var(--color-fg-muted)]">
          <FileText aria-hidden className="size-3.5" />
          {t('explore.ownFiles', {
            count: here.ownFiles,
            n: formatCount(here.ownFiles, locale),
          })}{' '}
          · {formatBytes(here.ownAllocated, locale)}
        </p>
      )}
      <div
        ref={scrollRef}
        className="pane-scroll rounded-md border border-[var(--color-border-subtle)]"
      >
        <ul
          className="relative w-full"
          style={{ height: `${String(virtualizer.getTotalSize())}px` }}
        >
          {virtualizer.getVirtualItems().map((row) => {
            const item = items[row.index];
            if (item === undefined) return null;
            const share = item.allocated / total;
            const entry: BasketItem = {
              path: item.path,
              name: item.name,
              kind: 'folder',
              allocated: item.allocated,
            };
            const selected = inBasket(basket, item.path);
            return (
              <li
                key={row.key}
                className="absolute inset-x-0 flex items-center gap-1 border-t border-[var(--color-border-subtle)] pr-1.5"
                style={{
                  top: 0,
                  height: ROW_HEIGHT,
                  transform: `translateY(${String(row.start)}px)`,
                }}
              >
                <ContextMenu>
                  <ContextMenuTrigger asChild>
                    <button
                      type="button"
                      className="flex h-full min-w-0 flex-1 items-center gap-2 px-2.5 text-left hover:bg-[var(--color-accent-subtle)] focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--color-accent)] disabled:cursor-default"
                      disabled={!item.hasChildren}
                      aria-label={
                        item.hasChildren ? t('explore.open', { name: item.name }) : item.name
                      }
                      onClick={() => {
                        onOpen(item.node);
                      }}
                    >
                      <Folder
                        aria-hidden
                        className="size-4 shrink-0 text-[var(--color-fg-subtle)]"
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-sm" title={item.path}>
                          {item.name}
                        </span>
                        {/* The share bar: how much of this folder it is. */}
                        <span
                          aria-hidden
                          className="mt-0.5 block h-1 rounded-full bg-[var(--color-accent)] opacity-70 transition-[width] duration-300"
                          style={{ width: `${String(Math.max(0.5, share * 100))}%` }}
                        />
                      </span>
                      {item.incomplete !== null && (
                        <Badge
                          tone="warn"
                          title={t('incompleteHint', { reason: t(`skip.${item.incomplete}`) })}
                        >
                          {t('incomplete')}
                        </Badge>
                      )}
                      <span className="tnum w-20 shrink-0 text-right font-mono text-2xs">
                        {formatBytes(item.allocated, locale)}
                      </span>
                      <span className="tnum hidden w-20 shrink-0 text-right font-mono text-2xs text-[var(--color-fg-subtle)] @3xl/main:block">
                        {formatCount(item.files, locale)}
                      </span>
                    </button>
                  </ContextMenuTrigger>
                  <ContextMenuContent>
                    <ContextMenuItem
                      disabled={!item.hasChildren}
                      onSelect={() => {
                        onOpen(item.node);
                      }}
                    >
                      {t('explore.menu.open')}
                    </ContextMenuItem>
                    <ContextMenuItem
                      onSelect={() => {
                        onReveal(item.path);
                      }}
                    >
                      {t('explore.menu.reveal')}
                    </ContextMenuItem>
                    <ContextMenuItem
                      onSelect={() => {
                        void globalThis.navigator?.clipboard?.writeText(item.path);
                      }}
                    >
                      {t('explore.menu.copy')}
                    </ContextMenuItem>
                    <ContextMenuItem
                      onSelect={() => {
                        onToggleBasket(entry);
                      }}
                    >
                      {selected ? t('basket.inBasketRemove') : t('basket.add')}
                    </ContextMenuItem>
                  </ContextMenuContent>
                </ContextMenu>
                <BasketToggle item={entry} selected={selected} onToggle={onToggleBasket} />
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
}

function FileList({
  files,
  locale,
  onReveal,
  basket,
  onToggleBasket,
}: {
  readonly files: readonly LargeFile[];
  readonly locale: string;
  readonly onReveal: (path: string) => void;
  readonly basket: readonly BasketItem[];
  readonly onToggleBasket: (item: BasketItem) => void;
}) {
  const { t } = useTranslation(STORAGE_NS);
  const scrollRef = useRef<HTMLDivElement | null>(null);
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: files.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
    initialRect: { width: 800, height: 400 },
    observeElementRect: observeRect,
  });
  const largest = Math.max(1, files[0]?.allocated ?? 1);
  const rows = files;

  if (rows.length === 0) return <EmptyState icon={<Layers />} title={t('explore.filesEmpty')} />;

  return (
    <div
      ref={scrollRef}
      className="pane-scroll rounded-md border border-[var(--color-border-subtle)]"
    >
      <ul className="relative w-full" style={{ height: `${String(virtualizer.getTotalSize())}px` }}>
        {virtualizer.getVirtualItems().map((row) => {
          const file = rows[row.index];
          if (file === undefined) return null;
          const slash = file.path.lastIndexOf('\\');
          const name = file.path.slice(slash + 1);
          const entry: BasketItem = {
            path: file.path,
            name: leafName(file.path),
            kind: 'file',
            allocated: file.allocated,
          };
          const selected = inBasket(basket, file.path);
          return (
            <li
              key={row.key}
              className="absolute inset-x-0 flex items-center gap-1 border-t border-[var(--color-border-subtle)] pr-1.5"
              style={{
                top: 0,
                height: ROW_HEIGHT,
                transform: `translateY(${String(row.start)}px)`,
              }}
            >
              <ContextMenu>
                <ContextMenuTrigger asChild>
                  <button
                    type="button"
                    className="flex h-full min-w-0 flex-1 items-center gap-2 px-2.5 text-left hover:bg-[var(--color-accent-subtle)] focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--color-accent)]"
                    title={file.path}
                    onClick={() => {
                      onReveal(file.path);
                    }}
                  >
                    <FileText
                      aria-hidden
                      className="size-4 shrink-0 text-[var(--color-fg-subtle)]"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm">{name}</span>
                      <span className="block truncate text-2xs text-[var(--color-fg-subtle)]">
                        {file.path.slice(0, Math.max(0, slash))}
                      </span>
                    </span>
                    <span
                      aria-hidden
                      className="hidden h-1 w-24 overflow-hidden rounded-full bg-[var(--color-border-subtle)] @3xl/main:block"
                    >
                      <span
                        className="block h-full bg-[var(--color-accent)]"
                        style={{ width: `${String((file.allocated / largest) * 100)}%` }}
                      />
                    </span>
                    <span className="tnum w-20 shrink-0 text-right font-mono text-2xs">
                      {formatBytes(file.allocated, locale)}
                    </span>
                  </button>
                </ContextMenuTrigger>
                <ContextMenuContent>
                  <ContextMenuItem
                    onSelect={() => {
                      onReveal(file.path);
                    }}
                  >
                    {t('explore.menu.reveal')}
                  </ContextMenuItem>
                  <ContextMenuItem
                    onSelect={() => {
                      void globalThis.navigator?.clipboard?.writeText(file.path);
                    }}
                  >
                    {t('explore.menu.copy')}
                  </ContextMenuItem>
                  <ContextMenuItem
                    onSelect={() => {
                      onToggleBasket(entry);
                    }}
                  >
                    {selected ? t('basket.inBasketRemove') : t('basket.add')}
                  </ContextMenuItem>
                </ContextMenuContent>
              </ContextMenu>
              <BasketToggle item={entry} selected={selected} onToggle={onToggleBasket} />
            </li>
          );
        })}
      </ul>
    </div>
  );
}
