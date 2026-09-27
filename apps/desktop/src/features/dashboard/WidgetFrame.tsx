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
 */

import { ChevronDown, ChevronUp, Maximize2, Minimize2, X } from 'lucide-react';
import { type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { Card, CardBody, CardHeader, CardTitle, IconButton, cn } from '@vitals/ui';

import { DASHBOARD_NS } from './strings';
import type { WidgetDefinition, WidgetSize } from './widgets';

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
  /** Trailing content shown when not editing — a live badge, a link. */
  readonly actions?: ReactNode;
  readonly children: ReactNode;
}

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
  actions,
  children,
}: WidgetFrameProps): React.JSX.Element {
  const { t } = useTranslation(DASHBOARD_NS);
  const title = t(definition.titleKey);

  return (
    <Card
      // The region label is the widget's own title rather than a generic
      // "widget": a screen reader user landing on a wall of identical regions
      // has no way to tell CPU from GPU without reading the contents of each.
      regionLabel={title}
      data-widget={definition.id}
      style={{ '--i': index } as React.CSSProperties}
      className={cn(
        'flex min-w-0 flex-col',
        // Grid placement is a class, not inline style, so the same layout works
        // at every breakpoint without JavaScript measuring the window. Full
        // spans every track, whatever the grid's column count is at this width.
        size === 'full' ? 'col-span-full' : '',
        editing && 'ring-1 ring-[var(--color-border-strong)]',
      )}
    >
      <CardHeader
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
          ) : (
            actions
          )
        }
      >
        <CardTitle level={3}>{title}</CardTitle>
      </CardHeader>
      <CardBody className="flex min-h-0 flex-1 flex-col gap-3">{children}</CardBody>
    </Card>
  );
}
