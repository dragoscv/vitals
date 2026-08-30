/**
 * The row context menu.
 *
 * Built on the UI package's Radix-backed `ContextMenu` specifically because
 * Radix opens it on **Shift+F10 and the Menu key** as well as right-click. A
 * hand-rolled `onContextMenu` handler would silently exclude keyboard-only
 * users from every action on this screen, and on this screen the actions are
 * the point.
 */

import { useTranslation } from 'react-i18next';

import {
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from '@vitals/ui';

import type { Process } from '@vitals/protocol';

import {
  UNIMPLEMENTED_ACTIONS,
  priorities,
  type ProcessPriority,
  type UnimplementedAction,
} from './actions';
import { fallback } from './strings';

/**
 * Highest first, which is the reverse of the wire order.
 *
 * A menu is read top-down and "make this faster" is the common intent, so the
 * option people want is under the cursor rather than at the bottom.
 */
const MENU_ORDER: readonly ProcessPriority[] = [...priorities].reverse();

/**
 * Translation key per wire value.
 *
 * Written out rather than derived by string manipulation so `fallback` can
 * typecheck it. A derived `process.action.priority.${string}` is opaque to
 * the compiler, which is exactly how a missing translation reaches the UI as
 * a raw key path.
 */
const PRIORITY_KEYS = {
  idle: 'process.action.priority.idle',
  'below-normal': 'process.action.priority.belowNormal',
  normal: 'process.action.priority.normal',
  'above-normal': 'process.action.priority.aboveNormal',
  high: 'process.action.priority.high',
  realtime: 'process.action.priority.realtime',
} as const satisfies Record<ProcessPriority, string>;

export interface ProcessMenuProps {
  readonly process: Process;
  readonly descendantCount: number;
  readonly onTerminate: () => void;
  readonly onTerminateTree: () => void;
  readonly onSuspend: () => void;
  readonly onResume: () => void;
  readonly onSetPriority: (priority: ProcessPriority) => void;
  readonly onSearchOnline: () => void;
  readonly onCopyDetails: () => void;
}

export function ProcessMenu(props: ProcessMenuProps): React.JSX.Element {
  const { t } = useTranslation();
  const { process, descendantCount } = props;

  const suspended = process.state === 'suspended';

  return (
    <ContextMenuContent className="min-w-56">
      <ContextMenuLabel>{process.name}</ContextMenuLabel>
      <ContextMenuSeparator />

      <ContextMenuItem destructive onSelect={props.onTerminate}>
        {t('process.action.endTask')}
      </ContextMenuItem>
      <ContextMenuItem
        destructive
        disabled={descendantCount === 0}
        onSelect={props.onTerminateTree}
      >
        {descendantCount === 0
          ? t('process.action.endTree')
          : t('process.action.endTreeCount', fallback('process.action.endTreeCount'), {
              count: descendantCount,
            })}
      </ContextMenuItem>

      <ContextMenuSeparator />

      {suspended ? (
        <ContextMenuItem onSelect={props.onResume}>{t('process.action.resume')}</ContextMenuItem>
      ) : (
        <ContextMenuItem onSelect={props.onSuspend}>{t('process.action.suspend')}</ContextMenuItem>
      )}

      <ContextMenuSub>
        <ContextMenuSubTrigger>{t('process.action.priority')}</ContextMenuSubTrigger>
        <ContextMenuSubContent>
          {MENU_ORDER.map((priority) => (
            <ContextMenuItem
              key={priority}
              onSelect={() => {
                props.onSetPriority(priority);
              }}
            >
              {t(PRIORITY_KEYS[priority], fallback(PRIORITY_KEYS[priority]))}
            </ContextMenuItem>
          ))}
        </ContextMenuSubContent>
      </ContextMenuSub>

      {/* File location and properties still have no backend command. Shown
          disabled with the reason rather than hidden: a task manager missing
          them reads as unfinished, and one that fakes them would be worse
          than either. */}
      {UNIMPLEMENTED_ACTIONS.map((action) => (
        <ContextMenuItem key={action} disabled title={unavailable(t)}>
          {t(`process.action.${action satisfies UnimplementedAction}`)}
        </ContextMenuItem>
      ))}

      <ContextMenuSeparator />

      <ContextMenuItem onSelect={props.onSearchOnline}>
        {t('process.action.searchOnline', fallback('process.action.searchOnline'))}
      </ContextMenuItem>
      <ContextMenuItem onSelect={props.onCopyDetails}>
        {t('process.action.copyDetails', fallback('process.action.copyDetails'))}
      </ContextMenuItem>
    </ContextMenuContent>
  );
}

function unavailable(t: (key: string, defaultValue: string) => string): string {
  return t('process.action.notImplemented', fallback('process.action.notImplemented'));
}
