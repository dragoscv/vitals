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

import { displayName, type Process } from '@vitals/protocol';

import { priorities, type ProcessPriority } from './actions';
import { PROCESSES_NS, fallback } from './strings';

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
  /**
   * The executable path, when known. Both shell actions are disabled without
   * one — with the reason in the tooltip — because a protected process never
   * yields its path and a menu item that fails every time reads as broken.
   */
  readonly executablePath: string | null;
  readonly onOpenFileLocation: () => void;
  readonly onShowProperties: () => void;
  readonly onSearchOnline: () => void;
  readonly onCopyDetails: () => void;
}

export function ProcessMenu(props: ProcessMenuProps): React.JSX.Element {
  const { t } = useTranslation();
  const { t: tp } = useTranslation(PROCESSES_NS);
  const { process, descendantCount } = props;

  const suspended = process.state === 'suspended';
  const hasPath = props.executablePath !== null;
  const noPath = hasPath ? undefined : tp('detail.file.unknownPath');
  const appName = displayName(process);

  return (
    <ContextMenuContent className="min-w-56">
      {/* The file name first, as the user asked: it is the one thing the
          table no longer shows, and what a terminal, a firewall rule or a
          search engine needs. The app name below it says which row this is. */}
      <ContextMenuLabel className="flex flex-col gap-0.5">
        <span className="font-mono text-[var(--color-fg-default)]">{process.name}</span>
        {appName !== process.name && (
          <span className="truncate font-normal text-[var(--color-fg-muted)]">{appName}</span>
        )}
      </ContextMenuLabel>
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

      {/* Affinity is not here: it lives in the detail panel as a set of
          presets computed from the core topology, which a flat menu cannot
          show. Selecting the row is what reveals them. */}
      <ContextMenuItem
        disabled={!hasPath}
        {...(noPath !== undefined && { title: noPath })}
        onSelect={props.onOpenFileLocation}
      >
        {t('process.action.openLocation')}
      </ContextMenuItem>
      <ContextMenuItem
        disabled={!hasPath}
        {...(noPath !== undefined && { title: noPath })}
        onSelect={props.onShowProperties}
      >
        {t('process.action.properties')}
      </ContextMenuItem>

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
