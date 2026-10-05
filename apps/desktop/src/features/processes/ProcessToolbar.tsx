/**
 * Search, filter chips, grouping toggle and the column picker.
 */

import { useTranslation } from 'react-i18next';

import {
  Checkbox,
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
  Button,
  SearchInput,
  SegmentedControl,
} from '@vitals/ui';

import { ExportButton } from '../../components/ExportButton';
import type { ExportColumn } from '../../lib/export';
import { COLUMNS, type ColumnId } from './columns';
import type { KindFilter, ProcessRow } from './model';
import { fallback } from './strings';

export interface ProcessToolbarProps {
  readonly query: string;
  readonly onQueryChange: (value: string) => void;
  readonly kind: KindFilter;
  readonly onKindChange: (kind: KindFilter) => void;
  readonly grouped: boolean;
  readonly onGroupedChange: (grouped: boolean) => void;
  readonly visible: readonly ColumnId[];
  readonly onToggleColumn: (id: ColumnId) => void;
  readonly shown: number;
  readonly total: number;
  /** The rows as drawn — filtered, sorted — so the file matches the screen. */
  readonly exportRows: readonly ProcessRow[];
  readonly exportColumns: readonly ExportColumn<ProcessRow>[];
}

export function ProcessToolbar(props: ProcessToolbarProps): React.JSX.Element {
  const { t } = useTranslation();

  const kinds: readonly { value: KindFilter; key: string }[] = [
    { value: 'all', key: 'process.filter.all' },
    { value: 'apps', key: 'process.filter.apps' },
    { value: 'background', key: 'process.filter.background' },
    { value: 'system', key: 'process.filter.system' },
  ];

  return (
    // Wraps rather than overflowing: at the 720 px window minimum the one-line
    // toolbar was 141 px wider than the content column and dragged the whole
    // page sideways with it.
    <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-[var(--color-border-subtle)] px-3 py-2">
      <SearchInput
        className="w-64 max-w-full min-w-40 flex-initial"
        value={props.query}
        onValueChange={props.onQueryChange}
        placeholder={t('process.search.placeholder', fallback('process.search.placeholder'))}
        aria-label={t('process.search.placeholder', fallback('process.search.placeholder'))}
        clearLabel={t('common.close')}
        resultsAnnouncement={t('process.results', fallback('process.results'), {
          shown: props.shown,
          total: props.total,
        })}
      />

      <SegmentedControl<KindFilter>
        value={props.kind}
        onValueChange={props.onKindChange}
        ariaLabel={t('process.filter.all', fallback('process.filter.all'))}
        options={kinds.map((entry) => ({
          value: entry.value,
          label: t(entry.key, fallback(entry.key as never)),
        }))}
      />

      <Checkbox
        checked={props.grouped}
        onCheckedChange={(checked) => props.onGroupedChange(checked === true)}
        label={
          <span className="text-2xs text-[var(--color-fg-muted)]">
            {t('process.group.byApp', fallback('process.group.byApp'))}
          </span>
        }
      />

      <div className="flex-1" />

      {/* No "Order held" pill here any more, although rows still hold their
          positions while the pointer is over the table. The pill appeared on
          hover, its width wrapped Export onto a second line, and the whole
          table jumped down a row under the cursor — the exact thing holding
          the order exists to prevent. Nothing in this toolbar may change
          size with hover state. */}
      <span className="text-2xs text-[var(--color-fg-muted)] tabular-nums">
        {t('process.results', fallback('process.results'), {
          shown: props.shown,
          total: props.total,
        })}
      </span>

      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button size="sm" variant="ghost">
            {t('process.columns', fallback('process.columns'))}
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          {COLUMNS.map((column) => (
            <DropdownMenuCheckboxItem
              key={column.id}
              checked={props.visible.includes(column.id)}
              disabled={column.required}
              onSelect={(event) => {
                // Without this the menu closes on every toggle, so hiding
                // three columns means reopening it three times.
                event.preventDefault();
                props.onToggleColumn(column.id);
              }}
            >
              {t(column.labelKey, fallback(column.labelKey as never))}
            </DropdownMenuCheckboxItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>

      <ExportButton name="processes" rows={props.exportRows} columns={props.exportColumns} />
    </div>
  );
}
