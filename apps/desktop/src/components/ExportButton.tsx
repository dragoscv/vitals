/**
 * The "Export" affordance every table screen shares.
 *
 * A dropdown rather than two buttons: the toolbar row is already crowded on
 * narrow windows, and the choice between CSV and JSON is one the user makes
 * once per export, not one that needs permanent real estate.
 *
 * The rows passed in are the rows on screen — after the search and the filter,
 * in the current sort order. Exporting the unfiltered set would be surprising:
 * someone who filtered to `chrome` and pressed Export wants the chrome rows,
 * and the file name says what screen it came from, not what was typed. The
 * count in the menu label makes what will be written explicit.
 *
 * The serialiser is imported lazily. Every table screen renders this button,
 * so anything it imports statically lands in every screen's chunk; the export
 * code is only needed on click.
 */

import { Download } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { i18n } from '@vitals/i18n';
import {
  Button,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from '@vitals/ui';

import type { ExportColumn, ExportKind } from '../lib/export';
import { reportFailure } from '../lib/reportFailure';
import { EXPORT_NS, registerExportStrings } from './exportStrings';

export interface ExportButtonProps<Row> {
  /** Goes into the file name: `vitals-<name>-<stamp>.csv`. Not translated. */
  readonly name: string;
  readonly rows: readonly Row[];
  readonly columns: readonly ExportColumn<Row>[];
  /** Overrides the writer in tests; defaults to the Blob download. */
  readonly save?: (filename: string, contents: string, kind: ExportKind) => void;
}

/**
 * Registers the bundle on first use rather than from a bootstrap the shell
 * owns. Every feature registers its own strings from its lazy import; this
 * component has no such import of its own, so it registers itself the first
 * time it renders after i18n is ready. `hasResourceBundle` keeps it to one
 * registration however many screens mount it.
 */
function ensureStrings(): void {
  if (i18n.isInitialized && !i18n.hasResourceBundle('en', EXPORT_NS)) {
    registerExportStrings();
  }
}

export function ExportButton<Row>({
  name,
  rows,
  columns,
  save,
}: ExportButtonProps<Row>): React.JSX.Element {
  ensureStrings();
  const { t } = useTranslation(EXPORT_NS);
  const empty = rows.length === 0;

  const run = (kind: ExportKind): void => {
    // A chunk that fails to load or a `save` that throws used to vanish into
    // the console: the menu closed and nothing downloaded.
    void reportFailure(
      import('../lib/export').then((exporter) => {
        const contents =
          kind === 'csv' ? exporter.toCsv(rows, columns) : exporter.toJson(rows, columns);
        const filename = exporter.exportFilename(name, kind);
        (save ?? exporter.saveExport)(filename, contents, kind);
      }),
      t('button'),
    );
  };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        {/* Disabled with the reason in the label rather than hidden: a control
            that vanishes when the table is empty reads as a bug the first time
            someone filters everything out. */}
        <Button
          variant="ghost"
          size="sm"
          disabled={empty}
          title={empty ? t('nothingHint') : undefined}
          aria-label={empty ? t('nothing') : t('button')}
        >
          <Download aria-hidden className="size-4" />
          {t('button')}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuLabel>{t('menuLabel', { count: rows.length })}</DropdownMenuLabel>
        <DropdownMenuItem
          onSelect={() => {
            run('csv');
          }}
        >
          <span className="flex flex-col">
            <span>{t('csv')}</span>
            <span className="text-2xs text-[var(--color-fg-muted)]">{t('csvHint')}</span>
          </span>
        </DropdownMenuItem>
        <DropdownMenuItem
          onSelect={() => {
            run('json');
          }}
        >
          <span className="flex flex-col">
            <span>{t('json')}</span>
            <span className="text-2xs text-[var(--color-fg-muted)]">{t('jsonHint')}</span>
          </span>
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
