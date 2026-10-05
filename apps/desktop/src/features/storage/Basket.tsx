/**
 * The review basket: what the user picked, its total, one confirmation, and
 * what happened to each item.
 *
 * # One confirmation, and it states what happens
 *
 * The dialog lists every item with its size and says, in the button itself,
 * that they go to the Recycle Bin. The shell's own prompts are off (the
 * backend suppresses them), so this is the only confirmation there is; a
 * second dialog that looks different is one people learn to click through.
 *
 * # The report is per item
 *
 * "3 of 5 recycled" is not enough when one of the other two is locked by a
 * program: the user needs to know which, and who holds it. Each line says
 * what happened and why, and a locked item names the programs Restart
 * Manager found. Items that could not be moved stay in the basket.
 */

import { Copy, FileText, Folder, FolderSearch, ListMinus, Trash2, X } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
  DialogContent,
  DialogRoot,
  IconButton,
  formatBytes,
  formatCount,
} from '@vitals/ui';

import { errorMessage } from '../../lib/commandError';
import { useRowMenu } from '../../lib/useRowMenu';
import {
  basketTotal,
  type BasketItem,
  type Holder,
  type RecycleItem,
  type RecycleReport,
} from './model';
import { STORAGE_NS } from './strings';

export interface BasketBarProps {
  readonly basket: readonly BasketItem[];
  readonly locale: string;
  readonly onReview: () => void;
  readonly onClear: () => void;
}

/** The strip under the explorer: how much is in review, and the way in. */
export function BasketBar({ basket, locale, onReview, onClear }: BasketBarProps) {
  const { t } = useTranslation(STORAGE_NS);
  if (basket.length === 0) {
    return <p className="text-2xs text-[var(--color-fg-subtle)]">{t('basket.hint')}</p>;
  }
  return (
    <section
      aria-label={t('basket.heading')}
      className="flex flex-wrap items-center gap-2 rounded-md border border-[var(--color-accent-border)] bg-[var(--color-accent-subtle)] px-2.5 py-1.5"
    >
      <Trash2 aria-hidden className="size-4 text-[var(--color-fg-muted)]" />
      <p className="min-w-0 flex-1 text-sm" aria-live="polite">
        {t('basket.summary', {
          count: basket.length,
          n: formatCount(basket.length, locale),
          size: formatBytes(basketTotal(basket), locale),
        })}
      </p>
      <Button variant="ghost" size="sm" onClick={onClear}>
        {t('basket.clear')}
      </Button>
      <Button variant="primary" size="sm" onClick={onReview}>
        {t('basket.review')}
      </Button>
    </section>
  );
}

export interface BasketDialogProps {
  readonly open: boolean;
  readonly basket: readonly BasketItem[];
  readonly locale: string;
  readonly recycling: boolean;
  readonly report: RecycleReport | null;
  readonly error: string | null;
  readonly onRemove: (path: string) => void;
  /** Shows the item in File Explorer; the menu omits the action without it. */
  readonly onReveal?: (path: string) => Promise<void>;
  readonly onConfirm: () => void;
  readonly onClose: () => void;
}

export function BasketDialog({
  open,
  basket,
  locale,
  recycling,
  report,
  error,
  onRemove,
  onReveal,
  onConfirm,
  onClose,
}: BasketDialogProps) {
  const { t } = useTranslation(STORAGE_NS);
  const showingReport = report !== null;
  const total = basketTotal(basket);
  const menu = useRowMenu();
  // A failed reveal is reported here rather than dropped: the user asked for
  // a window and nothing appeared, which needs a reason.
  const [revealError, setRevealError] = useState<string | null>(null);
  const shownError = error ?? revealError;

  return (
    <DialogRoot
      open={open}
      onOpenChange={(next) => {
        // Not closable mid-operation: the report is what tells the user
        // what happened, and it arrives when the operation ends.
        if (!next && !recycling) onClose();
      }}
    >
      <DialogContent
        size="lg"
        title={showingReport ? t('basket.reportTitle') : t('basket.confirmTitle')}
        description={
          showingReport
            ? recycledCount(report) === 0
              ? t('basket.reportNone')
              : recycledCount(report) === report.items.length
                ? t('basket.reportAll', { count: recycledCount(report) })
                : t('basket.reportSome', {
                    count: recycledCount(report),
                    left: report.items.length - recycledCount(report),
                  })
            : t('basket.confirmBody')
        }
        closeLabel={t('basket.close')}
        footer={
          showingReport ? (
            <Button variant="primary" size="sm" onClick={onClose}>
              {t('basket.done')}
            </Button>
          ) : (
            <>
              <Button variant="ghost" size="sm" onClick={onClose} disabled={recycling}>
                {t('basket.cancel')}
              </Button>
              {/* Primary, not danger: the danger style means irreversible in
                  this app, and a recycled item can be restored from the bin. */}
              <Button
                size="sm"
                variant="primary"
                disabled={recycling || basket.length === 0}
                onClick={onConfirm}
              >
                <Trash2 aria-hidden className="size-4" />
                {recycling
                  ? t('basket.recycling')
                  : t('basket.confirm', {
                      count: basket.length,
                      n: formatCount(basket.length, locale),
                      size: formatBytes(total, locale),
                    })}
              </Button>
            </>
          )
        }
      >
        {shownError !== null && (
          <p role="alert" className="mb-2 text-2xs text-[var(--color-status-danger)]">
            {t('basket.failed', { message: shownError })}
          </p>
        )}
        {showingReport ? (
          <ul className="flex flex-col gap-1.5">
            {report.items.map((item) => (
              <ReportRow key={item.path} item={item} locale={locale} />
            ))}
          </ul>
        ) : (
          <ul className="flex flex-col gap-1" onKeyDown={menu.onKeyDown}>
            {basket.map((item) => (
              <ContextMenu key={item.path} {...menu.rootProps(item.path)}>
                <ContextMenuTrigger asChild>
                  <li
                    tabIndex={0}
                    data-testid="basket-item"
                    className="flex items-center gap-2 rounded-md border border-[var(--color-border-subtle)] px-2 py-1 focus-visible:outline-2 focus-visible:outline-[var(--color-accent)]"
                    onContextMenu={menu.onContextMenu}
                  >
                    {item.kind === 'folder' ? (
                      <Folder
                        aria-hidden
                        className="size-4 shrink-0 text-[var(--color-fg-subtle)]"
                      />
                    ) : (
                      <FileText
                        aria-hidden
                        className="size-4 shrink-0 text-[var(--color-fg-subtle)]"
                      />
                    )}
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm">{item.name}</span>
                      <span className="block truncate font-mono text-2xs text-[var(--color-fg-subtle)]">
                        {item.path}
                      </span>
                    </span>
                    <span className="tnum font-mono text-2xs">
                      {formatBytes(item.allocated, locale)}
                    </span>
                    <IconButton
                      size="sm"
                      icon={<X aria-hidden />}
                      label={t('basket.remove', { name: item.name })}
                      disabled={recycling}
                      onClick={() => {
                        onRemove(item.path);
                      }}
                    />
                  </li>
                </ContextMenuTrigger>
                <ContextMenuContent>
                  <ContextMenuLabel>{item.name}</ContextMenuLabel>
                  <ContextMenuSeparator />
                  {onReveal !== undefined && (
                    <ContextMenuItem
                      onSelect={() => {
                        setRevealError(null);
                        onReveal(item.path).catch((cause: unknown) => {
                          setRevealError(errorMessage(cause));
                        });
                      }}
                    >
                      <FolderSearch className="size-4" aria-hidden="true" />
                      {t('explore.menu.reveal')}
                    </ContextMenuItem>
                  )}
                  <ContextMenuItem
                    onSelect={() => {
                      void globalThis.navigator?.clipboard?.writeText(item.path);
                    }}
                  >
                    <Copy className="size-4" aria-hidden="true" />
                    {t('explore.menu.copy')}
                  </ContextMenuItem>
                  <ContextMenuSeparator />
                  <ContextMenuItem
                    disabled={recycling}
                    onSelect={() => {
                      onRemove(item.path);
                    }}
                  >
                    <ListMinus className="size-4" aria-hidden="true" />
                    {t('basket.inBasketRemove')}
                  </ContextMenuItem>
                </ContextMenuContent>
              </ContextMenu>
            ))}
          </ul>
        )}
      </DialogContent>
    </DialogRoot>
  );
}

function recycledCount(report: RecycleReport): number {
  return report.items.filter((item) => item.outcome === 'recycled').length;
}

function ReportRow({ item, locale }: { readonly item: RecycleItem; readonly locale: string }) {
  const { t } = useTranslation(STORAGE_NS);
  const ok = item.outcome === 'recycled';
  return (
    <li className="rounded-md border border-[var(--color-border-subtle)] px-2 py-1.5">
      <div className="flex items-center gap-2">
        <Badge tone={ok ? 'ok' : item.outcome === 'missing' ? 'neutral' : 'warn'}>
          {t(`outcome.${item.outcome}`)}
        </Badge>
        <span className="min-w-0 flex-1 truncate font-mono text-2xs" title={item.path}>
          {item.path}
        </span>
        {item.freed !== null && (
          <span className="tnum font-mono text-2xs">{formatBytes(item.freed, locale)}</span>
        )}
      </div>
      {item.outcome === 'refused' && item.protection !== null && (
        <p className="mt-1 text-2xs text-[var(--color-fg-muted)]">
          {t(`protection.${item.protection}`)}
        </p>
      )}
      {item.outcome !== 'refused' && item.outcome !== 'recycled' && item.outcome !== 'locked' && (
        <p className="mt-1 text-2xs text-[var(--color-fg-muted)]">
          {t(`outcomeWhy.${item.outcome}`, { code: item.code ?? 0 })}
        </p>
      )}
      {item.outcome === 'locked' && <HolderList holders={item.holders} />}
    </li>
  );
}

/** "Why can't I delete this": the programs holding it, or why none are named. */
export function HolderList({ holders }: { readonly holders: readonly Holder[] | null }) {
  const { t } = useTranslation(STORAGE_NS);
  if (holders === null) {
    return <p className="mt-1 text-2xs text-[var(--color-fg-muted)]">{t('holders.unknown')}</p>;
  }
  if (holders.length === 0) {
    return <p className="mt-1 text-2xs text-[var(--color-fg-muted)]">{t('holders.none')}</p>;
  }
  return (
    <div className="mt-1 text-2xs">
      <p className="text-[var(--color-fg-muted)]">
        {t('holders.intro', { count: holders.length })}
      </p>
      <ul className="mt-0.5 flex flex-col gap-0.5">
        {holders.map((holder) => (
          <li key={holder.pid} className="flex items-center gap-1.5">
            <span className="font-medium">
              {holder.name === '' ? t('holders.unnamed') : holder.name}
            </span>
            <span className="text-[var(--color-fg-subtle)]">
              {t('holders.pid', { pid: holder.pid })}
              {holder.service !== null && ` · ${t('holders.service', { name: holder.service })}`}
            </span>
            {holder.kind === 'critical' && <Badge tone="danger">{t('holders.critical')}</Badge>}
          </li>
        ))}
      </ul>
      <p className="mt-0.5 text-[var(--color-fg-subtle)]">{t('holders.advice')}</p>
    </div>
  );
}
