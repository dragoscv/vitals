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
  Tooltip,
  TooltipProvider,
} from '@vitals/ui';

import { COLUMNS, type ColumnId } from './columns';
import type { KindFilter } from './model';
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
  /** True while row positions are held because the pointer is over the table. */
  readonly orderHeld: boolean;
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
    <div className="flex shrink-0 items-center gap-2 border-b border-[var(--color-border-subtle)] px-3 py-2">
      <SearchInput
        className="w-64"
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

      {/* Surfaced rather than silent: a user who notices rows have stopped
          moving must be able to find out why, and that the values are still
          live. An invisible freeze looks like a hung app. */}
      {props.orderHeld && (
        // Its own provider rather than relying on one further up: this chip
        // must explain itself wherever the screen is mounted, and a missing
        // ancestor provider is a runtime throw, not a degraded tooltip.
        <TooltipProvider>
          <Tooltip content={t('process.order.pausedHint', fallback('process.order.pausedHint'))}>
            <span
              tabIndex={0}
              data-testid="order-held"
              className="rounded-full border border-[var(--color-border-default)] px-2 py-0.5 text-2xs text-[var(--color-fg-subtle)]"
            >
              {t('process.order.paused', fallback('process.order.paused'))}
            </span>
          </Tooltip>
        </TooltipProvider>
      )}

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
    </div>
  );
}
