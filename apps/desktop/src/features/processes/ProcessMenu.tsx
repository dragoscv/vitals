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

import { UNIMPLEMENTED_ACTIONS, type UnimplementedAction } from './actions';
import { fallback } from './strings';

const PRIORITIES = ['realtime', 'high', 'aboveNormal', 'normal', 'belowNormal', 'idle'] as const;

export interface ProcessMenuProps {
  readonly process: Process;
  readonly descendantCount: number;
  readonly onTerminate: () => void;
  readonly onTerminateTree: () => void;
  readonly onSuspend: () => void;
  readonly onResume: () => void;
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

      {/* Priority, affinity, file location and properties have no backend
          command yet. They are shown disabled with the reason rather than
          hidden: a task manager with no priority menu reads as unfinished,
          and one that fakes the action would be worse than either. */}
      <ContextMenuSub>
        <ContextMenuSubTrigger disabled title={unavailable(t)}>
          {t('process.action.priority')}
        </ContextMenuSubTrigger>
        <ContextMenuSubContent>
          {PRIORITIES.map((priority) => (
            <ContextMenuItem key={priority} disabled>
              {priority}
            </ContextMenuItem>
          ))}
        </ContextMenuSubContent>
      </ContextMenuSub>

      {UNIMPLEMENTED_ACTIONS.filter((action) => action !== 'priority').map((action) => (
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
