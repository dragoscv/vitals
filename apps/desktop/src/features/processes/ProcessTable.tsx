/**
 * The virtualised process table.
 *
 * Only the rows in view are mounted. Six hundred rows × twelve cells is 7 200
 * DOM nodes to reconcile every second; at that size React spends longer
 * diffing than the frame budget allows and the table stutters — in an app
 * whose entire purpose is to show you what is stuttering.
 *
 * Grid semantics are explicit (`role="grid"`/`row`/`gridcell`) because
 * virtualisation breaks the implicit table accessibility tree: the DOM
 * contains forty rows out of six hundred, so `aria-rowcount` and
 * `aria-rowindex` are the only way a screen reader can report "row 412 of
 * 587" rather than "row 12 of 40".
 */

import { useVirtualizer } from '@tanstack/react-virtual';
import {
  forwardRef,
  memo,
  useCallback,
  useMemo,
  useRef,
  type ComponentPropsWithoutRef,
  type KeyboardEvent,
} from 'react';
import { useTranslation } from 'react-i18next';

import { ContextMenu, ContextMenuTrigger, cn } from '@vitals/ui';

import { COLUMN_BY_ID, type ColumnId } from './columns';
import { OVERSCAN, ROW_HEIGHT, UNKNOWN } from './constants';
import type { ProcessRow } from './model';
import { ProcessMenu } from './ProcessMenu';
import { fallback } from './strings';

export interface ProcessTableProps {
  readonly rows: readonly ProcessRow[];
  readonly columns: readonly ColumnId[];
  readonly widths: Readonly<Partial<Record<ColumnId, number>>>;
  readonly sortColumn: ColumnId;
  readonly sortDirection: 'asc' | 'desc';
  readonly onSort: (column: ColumnId) => void;
  readonly onResize: (column: ColumnId, width: number) => void;
  readonly selected: ReadonlySet<string>;
  readonly focusedId: string | null;
  readonly expanded: ReadonlySet<string>;
  readonly onToggleExpand: (id: string) => void;
  readonly onRowPointerDown: (
    id: string,
    event: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean },
  ) => void;
  readonly onFocusRow: (id: string) => void;
  readonly onKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void;
  readonly onHoverChange: (hovering: boolean) => void;
  readonly onMenuOpenChange: (open: boolean) => void;
  readonly menuProps: (row: ProcessRow) => React.ComponentProps<typeof ProcessMenu>;
  readonly locale: string;
}

export function ProcessTable(props: ProcessTableProps): React.JSX.Element {
  const { t } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);

  const columns = useMemo(
    () =>
      props.columns
        .map((id) => COLUMN_BY_ID.get(id))
        .filter((c): c is NonNullable<typeof c> => c !== undefined),
    [props.columns],
  );

  // The React Compiler cannot memoise `useVirtualizer`'s returned functions
  // and therefore skips optimising this component. That is acceptable here
  // and the disable is deliberate rather than a shrug: the expensive part of
  // a tick is the rows, each of which is separately memoised below, and the
  // virtualiser's own output is a handful of numbers recomputed per scroll
  // frame regardless. Nothing from it is passed into a memoised boundary,
  // which is the case the rule exists to catch.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: props.rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: OVERSCAN,
    // A viewport assumed until layout reports a real one. Without it the
    // measured height is 0 on the first pass and the window is empty, so the
    // table renders nothing until a resize happens — which in a headless
    // environment never does.
    initialRect: { width: 1024, height: 640 },
    // A measured height of 0 is treated as "not laid out yet", not as "no
    // room". Windows reports 0 for a brief moment during restore from
    // minimised and after a monitor change; taking it literally empties the
    // table and, because nothing then triggers another measurement, it stays
    // empty until the user resizes the window.
    observeElementRect: (instance, cb) => {
      const element = instance.scrollElement;
      if (element === null || element === undefined) return undefined;
      const measure = (): void => {
        const rect = element.getBoundingClientRect();
        cb({
          width: rect.width > 0 ? rect.width : 1024,
          height: rect.height > 0 ? rect.height : 640,
        });
      };
      measure();
      const observer = new ResizeObserver(measure);
      observer.observe(element);
      return () => observer.disconnect();
    },
    // Keying by process identity rather than by index means React reuses a
    // row's DOM for the same process when the order changes, instead of
    // repainting every row below an insertion.
    getItemKey: (index) => props.rows[index]?.id ?? index,
  });

  const totalWidth = columns.reduce(
    (sum, column) => sum + (props.widths[column.id] ?? column.width),
    0,
  );

  const gridTemplate = columns
    .map((column) => `${props.widths[column.id] ?? column.width}px`)
    .join(' ');

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      onPointerEnter={() => props.onHoverChange(true)}
      onPointerLeave={() => props.onHoverChange(false)}
    >
      <div
        ref={scrollRef}
        className="min-h-0 flex-1 overflow-auto"
        role="grid"
        aria-label={t('process.table.label', fallback('process.table.label'))}
        aria-rowcount={props.rows.length + 1}
        aria-colcount={columns.length}
        // One tab stop for the whole grid with arrow keys inside, which is the
        // WAI-ARIA grid pattern. Six hundred tab stops would be unusable.
        tabIndex={0}
        onKeyDown={props.onKeyDown}
      >
        <div style={{ minWidth: totalWidth }}>
          <div
            role="row"
            aria-rowindex={1}
            className="sticky top-0 z-10 grid border-b border-[var(--color-border-default)] bg-[var(--color-bg-raised)]"
            style={{ gridTemplateColumns: gridTemplate }}
          >
            {columns.map((column, index) => {
              const active = props.sortColumn === column.id;
              return (
                <div
                  key={column.id}
                  role="columnheader"
                  aria-colindex={index + 1}
                  aria-sort={
                    active ? (props.sortDirection === 'asc' ? 'ascending' : 'descending') : 'none'
                  }
                  className="relative flex h-7 items-center"
                >
                  <button
                    type="button"
                    onClick={() => props.onSort(column.id)}
                    aria-label={t('a11y.sortBy', {
                      column: t(column.labelKey, fallback(column.labelKey as never)),
                    })}
                    className={cn(
                      'text-2xs flex h-full w-full items-center gap-1 px-2 font-medium',
                      column.align === 'end' && 'justify-end',
                      active ? 'text-[var(--color-fg-default)]' : 'text-[var(--color-fg-muted)]',
                      'hover:text-[var(--color-fg-default)] focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-[var(--color-accent)]',
                    )}
                  >
                    <span className="truncate">
                      {t(column.labelKey, fallback(column.labelKey as never))}
                    </span>
                    {active && (
                      <span aria-hidden="true">{props.sortDirection === 'asc' ? '▲' : '▼'}</span>
                    )}
                  </button>
                  <ResizeHandle
                    column={column.id}
                    current={props.widths[column.id] ?? column.width}
                    min={column.minWidth}
                    onResize={props.onResize}
                    label={t(column.labelKey, fallback(column.labelKey as never))}
                  />
                </div>
              );
            })}
          </div>

          <div role="rowgroup" className="relative" style={{ height: virtualizer.getTotalSize() }}>
            {virtualizer.getVirtualItems().map((item) => {
              const row = props.rows[item.index];
              if (row === undefined) return null;
              return (
                <ContextMenu key={item.key} onOpenChange={props.onMenuOpenChange}>
                  <ContextMenuTrigger asChild>
                    <Row
                      row={row}
                      columns={columns}
                      gridTemplate={gridTemplate}
                      top={item.start}
                      rowIndex={item.index + 2}
                      selected={props.selected.has(row.id)}
                      focused={props.focusedId === row.id}
                      expanded={props.expanded.has(row.id)}
                      onToggleExpand={props.onToggleExpand}
                      onPointerDown={props.onRowPointerDown}
                      onFocusRow={props.onFocusRow}
                      locale={props.locale}
                    />
                  </ContextMenuTrigger>
                  <ProcessMenu {...props.menuProps(row)} />
                </ContextMenu>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

interface RowProps extends Omit<
  ComponentPropsWithoutRef<'div'>,
  'style' | 'onFocus' | 'onPointerDown'
> {
  readonly row: ProcessRow;
  readonly columns: readonly NonNullable<ReturnType<typeof COLUMN_BY_ID.get>>[];
  readonly gridTemplate: string;
  readonly top: number;
  readonly rowIndex: number;
  readonly selected: boolean;
  readonly focused: boolean;
  readonly expanded: boolean;
  readonly onToggleExpand: (id: string) => void;
  readonly onPointerDown: (
    id: string,
    event: { shiftKey: boolean; ctrlKey: boolean; metaKey: boolean },
  ) => void;
  readonly onFocusRow: (id: string) => void;
  readonly locale: string;
}

/**
 * One row.
 *
 * Memoised on the row object and its interaction flags. Row objects are
 * rebuilt only when their process actually changed — the delta reconciler
 * upstream reuses the object otherwise — so a tick in which forty visible
 * processes are idle re-renders none of them.
 *
 * The ref and the remaining props are forwarded because Radix's
 * `ContextMenuTrigger asChild` clones this element to attach its own
 * handlers. Dropping them silently disables right-click *and* Shift+F10 on
 * every row, which removes the only route to any row action.
 */
const Row = memo(
  forwardRef<HTMLDivElement, RowProps>(function Row(props, ref) {
    const { t } = useTranslation();
    const {
      row,
      columns,
      gridTemplate,
      top,
      rowIndex,
      selected,
      focused,
      expanded,
      onToggleExpand,
      onPointerDown,
      onFocusRow,
      locale,
      ...rest
    } = props;

    return (
      <div
        ref={ref}
        role="row"
        aria-rowindex={rowIndex}
        aria-selected={selected}
        aria-expanded={row.childIds.length > 0 ? expanded : undefined}
        aria-level={row.depth + 1}
        data-testid={`process-row-${row.process.key.pid}`}
        data-focused={focused ? 'true' : undefined}
        {...rest}
        onPointerDown={(event) =>
          onPointerDown(row.id, {
            shiftKey: event.shiftKey,
            ctrlKey: event.ctrlKey,
            metaKey: event.metaKey,
          })
        }
        onFocus={() => onFocusRow(row.id)}
        className={cn(
          'text-2xs absolute inset-x-0 grid items-center',
          // Transitions are deliberately absent. An animated row move is the
          // opposite of what this screen needs: it extends the window in which
          // a click lands on the wrong process.
          selected
            ? 'bg-[var(--color-accent-subtle)] text-[var(--color-fg-default)]'
            : 'hover:bg-[var(--color-bg-subtle)]',
          focused && 'outline-2 -outline-offset-2 outline-[var(--color-accent)]',
        )}
        style={{
          height: ROW_HEIGHT,
          transform: `translateY(${top}px)`,
          gridTemplateColumns: gridTemplate,
        }}
      >
        {columns.map((column, index) => {
          const isName = column.id === 'name';
          const raw = column.render(row, locale);
          const text = column.id === 'state' ? t(`process.state.${row.process.state}`) : raw;

          return (
            <div
              key={column.id}
              role="gridcell"
              aria-colindex={index + 1}
              className={cn(
                'truncate px-2',
                column.align === 'end' && 'text-right font-mono tabular-nums',
                text === UNKNOWN && 'text-[var(--color-fg-subtle)]',
              )}
              style={isName ? { paddingInlineStart: 8 + row.depth * 14 } : undefined}
              title={isName ? row.process.name : undefined}
            >
              {isName && row.childIds.length > 0 ? (
                <span className="flex items-center gap-1">
                  <button
                    type="button"
                    aria-label={t(
                      expanded ? 'process.tree.collapse' : 'process.tree.expand',
                      fallback(expanded ? 'process.tree.collapse' : 'process.tree.expand'),
                      { name: row.process.name },
                    )}
                    onPointerDown={(event) => event.stopPropagation()}
                    onClick={() => onToggleExpand(row.id)}
                    className="inline-flex size-4 shrink-0 items-center justify-center rounded text-[var(--color-fg-subtle)] hover:text-[var(--color-fg-default)] focus-visible:outline-2 focus-visible:outline-[var(--color-accent)]"
                  >
                    <span aria-hidden="true">{expanded ? '▾' : '▸'}</span>
                  </button>
                  <span className="truncate">{text}</span>
                </span>
              ) : (
                text
              )}
            </div>
          );
        })}
      </div>
    );
  }),
);

function ResizeHandle({
  column,
  current,
  min,
  onResize,
  label,
}: {
  readonly column: ColumnId;
  readonly current: number;
  readonly min: number;
  readonly onResize: (column: ColumnId, width: number) => void;
  readonly label: string;
}): React.JSX.Element {
  const startX = useRef(0);
  const startWidth = useRef(current);

  const onPointerMove = useCallback(
    (event: PointerEvent) => {
      onResize(column, Math.max(min, startWidth.current + (event.clientX - startX.current)));
    },
    [column, min, onResize],
  );

  return (
    <div
      // A separator role with arrow-key handling, because drag-only resizing
      // is unreachable without a pointer and this is a persisted preference a
      // keyboard user has as much reason to change as anyone.
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuenow={current}
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key === 'ArrowLeft') {
          event.preventDefault();
          onResize(column, Math.max(min, current - 8));
        } else if (event.key === 'ArrowRight') {
          event.preventDefault();
          onResize(column, current + 8);
        }
      }}
      onPointerDown={(event) => {
        event.preventDefault();
        startX.current = event.clientX;
        startWidth.current = current;
        const up = (): void => {
          globalThis.removeEventListener('pointermove', onPointerMove);
          globalThis.removeEventListener('pointerup', up);
        };
        globalThis.addEventListener('pointermove', onPointerMove);
        globalThis.addEventListener('pointerup', up);
      }}
      className="hover:bg-[var(--color-accent)]/40 absolute inset-y-0 -right-1 w-2 cursor-col-resize focus-visible:bg-[var(--color-accent)]"
    />
  );
}
