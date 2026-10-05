/**
 * The chrome every widget sits in.
 *
 * Layout controls live here rather than in each widget so that adding a widget
 * is writing a body and nothing else — twelve copies of a move/resize/remove
 * menu is twelve chances for one of them to behave differently.
 *
 * # Controls are only mounted in edit mode
 *
 * Not hidden with CSS: mounted or not. A hidden button is still in the tab
 * order unless every widget remembers `tabIndex={-1}`, and a dashboard with
 * twelve widgets would put thirty-six invisible buttons between the user and
 * the content they were tabbing towards.
 *
 * # The right-click menu is always there
 *
 * The same layout actions, plus Add and Reset, without entering edit mode: a
 * menu adds nothing to the tab order (it opens on a right click or
 * Shift+F10 only, through `useRowMenu`), so it does not carry the cost the
 * buttons above would.
 */

import { ChevronDown, ChevronUp, Maximize2, Minimize2, Plus, RotateCcw, X } from 'lucide-react';
import { type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import {
  AnimatedValue,
  Card,
  CardBody,
  CardHeader,
  CardTitle,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
  IconButton,
  cn,
} from '@vitals/ui';

import type { RowMenu } from '../../lib/useRowMenu';
import { DASHBOARD_NS } from './strings';
import type { Headline } from './widgets/headline';
import type { WidgetDefinition, WidgetId, WidgetSize } from './widgets';

export interface WidgetFrameProps {
  readonly definition: WidgetDefinition;
  /** Position in the grid, for the staggered entrance delay. */
  readonly index?: number;
  readonly size: WidgetSize;
  readonly editing: boolean;
  /** False for the first and last widget respectively, to disable the arrows. */
  readonly canMoveUp: boolean;
  readonly canMoveDown: boolean;
  readonly onMove: (direction: 'up' | 'down') => void;
  readonly onResize: (size: WidgetSize) => void;
  readonly onRemove: () => void;
  /**
   * The dashboard's shared menu gate. Absent: no right-click menu, which is
   * what a frame rendered outside the grid (a preview, a test) wants.
   */
  readonly menu?: RowMenu;
  /** Widgets that can still be added, for the menu's Add submenu. */
  readonly addable?: readonly Pick<WidgetDefinition, 'id' | 'titleKey'>[];
  readonly onAdd?: (id: WidgetId) => void;
  readonly onReset?: () => void;
  /** Trailing content shown when not editing — a live badge, a link. */
  readonly actions?: ReactNode;
  /** The one number the widget leads with, shown large beside the title. */
  readonly headline?: Headline | null;
  readonly children: ReactNode;
}

const HEADLINE_TONE = {
  default: 'text-[var(--color-fg-default)]',
  warn: 'text-[var(--color-status-warn)]',
  danger: 'text-[var(--color-status-danger)]',
} as const;

export function WidgetFrame({
  definition,
  index = 0,
  size,
  editing,
  canMoveUp,
  canMoveDown,
  onMove,
  onResize,
  onRemove,
  menu,
  addable = [],
  onAdd,
  onReset,
  actions,
  headline,
  children,
}: WidgetFrameProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const title = t(definition.titleKey);

  const card = (
    <Card
      // The region label is the widget's own title rather than a generic
      // "widget": a screen reader user landing on a wall of identical regions
      // has no way to tell CPU from GPU without reading the contents of each.
      regionLabel={title}
      data-widget={definition.id}
      style={{ '--i': index } as React.CSSProperties}
      {...(menu !== undefined && { onContextMenu: menu.onContextMenu })}
      className={cn(
        // `min-h-0` lets the card shrink to its grid row: the dashboard fits
        // the window and never scrolls, so a card whose content is taller
        // than its row scrolls inside its own body instead (S12-25).
        '@container/widget flex min-h-0 min-w-0 flex-col overflow-hidden',
        // Grid placement is a class, not inline style, so the same layout works
        // at every breakpoint without JavaScript measuring the window. Full
        // spans every track, whatever the grid's column count is at this width.
        size === 'full' ? 'col-span-full' : '',
        editing && 'ring-1 ring-[var(--color-border-strong)]',
      )}
    >
      <CardHeader
        className="min-h-9 px-3.5 pt-2.5 pb-0.5"
        actions={
          editing ? (
            <>
              <IconButton
                label={t('layout.moveUp')}
                icon={<ChevronUp />}
                disabled={!canMoveUp}
                onClick={() => {
                  onMove('up');
                }}
              />
              <IconButton
                label={t('layout.moveDown')}
                icon={<ChevronDown />}
                disabled={!canMoveDown}
                onClick={() => {
                  onMove('down');
                }}
              />
              <IconButton
                label={size === 'full' ? t('layout.narrow') : t('layout.wide')}
                icon={size === 'full' ? <Minimize2 /> : <Maximize2 />}
                onClick={() => {
                  onResize(size === 'full' ? 'half' : 'full');
                }}
              />
              <IconButton
                label={definition.essential ? t('layout.essential') : t('layout.remove')}
                icon={<X />}
                // Essential widgets render the button disabled rather than
                // omitting it, so the row of controls does not reflow between
                // widgets and the tooltip can explain why it cannot be removed.
                disabled={definition.essential}
                onClick={onRemove}
              />
            </>
          ) : headline ? (
            // The number a glance is for. In the header, not as the first of
            // six equal stats, so the card can be a quarter of the window and
            // still answer "how busy is it" without reading the body.
            <p className="flex items-baseline gap-2">
              {headline.caption !== undefined && (
                <span className="tnum hidden truncate text-2xs text-[var(--color-fg-muted)] @[20rem]/widget:inline">
                  {headline.caption}
                </span>
              )}
              <span
                className={cn(
                  'tnum text-lg leading-none font-semibold tracking-tight',
                  HEADLINE_TONE[headline.tone],
                )}
              >
                <AnimatedValue value={headline.value} />
              </span>
            </p>
          ) : (
            actions
          )
        }
      >
        <CardTitle level={3}>{title}</CardTitle>
      </CardHeader>
      <CardBody className="widget-body flex min-h-0 flex-1 flex-col gap-2 overflow-x-hidden overflow-y-auto px-3.5 pt-1.5 pb-3">
        {children}
      </CardBody>
    </Card>
  );

  if (menu === undefined) return card;

  return (
    <ContextMenu {...menu.rootProps(definition.id)}>
      <ContextMenuTrigger asChild>{card}</ContextMenuTrigger>
      <ContextMenuContent className="min-w-52">
        <ContextMenuLabel>{title}</ContextMenuLabel>
        <ContextMenuSeparator />
        <ContextMenuItem
          disabled={!canMoveUp}
          onSelect={() => {
            onMove('up');
          }}
        >
          <ChevronUp className="size-4" aria-hidden="true" />
          {t('layout.moveUp')}
        </ContextMenuItem>
        <ContextMenuItem
          disabled={!canMoveDown}
          onSelect={() => {
            onMove('down');
          }}
        >
          <ChevronDown className="size-4" aria-hidden="true" />
          {t('layout.moveDown')}
        </ContextMenuItem>
        <ContextMenuItem
          onSelect={() => {
            onResize(size === 'full' ? 'half' : 'full');
          }}
        >
          {size === 'full' ? (
            <Minimize2 className="size-4" aria-hidden="true" />
          ) : (
            <Maximize2 className="size-4" aria-hidden="true" />
          )}
          {size === 'full' ? t('layout.narrow') : t('layout.wide')}
        </ContextMenuItem>
        <ContextMenuItem
          destructive
          // Disabled rather than omitted, as in edit mode, so the title can
          // say why this one stays.
          disabled={definition.essential}
          {...(definition.essential && { title: t('layout.essential') })}
          onSelect={onRemove}
        >
          <X className="size-4" aria-hidden="true" />
          {t('layout.removeWidget')}
        </ContextMenuItem>
        {(onAdd !== undefined || onReset !== undefined) && <ContextMenuSeparator />}
        {onAdd !== undefined && (
          <ContextMenuSub>
            <ContextMenuSubTrigger
              disabled={addable.length === 0}
              {...(addable.length === 0 && { title: t('layout.addEmpty') })}
            >
              <Plus className="size-4" aria-hidden="true" />
              {t('layout.addMenu')}
            </ContextMenuSubTrigger>
            <ContextMenuSubContent>
              {addable.map((widget) => (
                <ContextMenuItem
                  key={widget.id}
                  onSelect={() => {
                    onAdd(widget.id);
                  }}
                >
                  {t(widget.titleKey)}
                </ContextMenuItem>
              ))}
            </ContextMenuSubContent>
          </ContextMenuSub>
        )}
        {onReset !== undefined && (
          <ContextMenuItem onSelect={onReset}>
            <RotateCcw className="size-4" aria-hidden="true" />
            {t('layout.reset')}
          </ContextMenuItem>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}
