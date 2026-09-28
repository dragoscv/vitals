/**
 * The row actions for Startup and Services, rendered two ways.
 *
 * Every row has a right-click menu (Radix `ContextMenu`, which also opens on
 * Shift+F10 and the Menu key) and a visible "⋯" button (`DropdownMenu`). The
 * button exists because a right-click menu alone is undiscoverable: nothing
 * on screen says it is there.
 *
 * Both menus are drawn from ONE descriptor list built here. Two hand-written
 * menus would drift — an item disabled in one and live in the other is how a
 * user ends up clicking through to a failure the other menu had prevented.
 */

import { useTranslation } from 'react-i18next';

import {
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuRadioGroup,
  ContextMenuRadioItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
} from '@vitals/ui';

import {
  isMachineWide,
  isRunOnce,
  labelFor,
  type DisableRiskKey,
  type ServiceEntry,
  type StartupEntry,
} from './model';
import type { ServiceStartType, StartupActions } from './startupActions';
import { STARTUP_NS } from './strings';

export type MenuNode =
  | {
      readonly kind: 'item';
      readonly id: string;
      readonly label: string;
      readonly onSelect: () => void;
      readonly disabled?: boolean;
      readonly title?: string;
    }
  | { readonly kind: 'separator'; readonly id: string }
  | {
      readonly kind: 'radio';
      readonly id: string;
      readonly label: string;
      readonly value: string;
      readonly options: readonly { readonly value: string; readonly label: string }[];
      readonly onValueChange: (value: string) => void;
      readonly disabled?: boolean;
      readonly title?: string;
    };

export interface RowMenuModel {
  readonly key: string;
  readonly label: string;
  readonly nodes: readonly MenuNode[];
}

/** Every change a row can request; also the key for its status line. */
export type Verb =
  'enable' | 'disable' | 'start' | 'stop' | 'restart' | 'automatic' | 'manual' | 'disabled';

export interface ActionRequest {
  readonly key: string;
  readonly name: string;
  readonly verb: Verb;
  /** True when the change makes less run — the only kind worth a warning. */
  readonly reduces: boolean;
  readonly risk: DisableRiskKey;
  readonly run: (confirmed: boolean) => Promise<void>;
}

export interface RowMenuContext {
  readonly actions: StartupActions;
  readonly busyKey: string | null;
  readonly request: (request: ActionRequest) => void;
  /** Runs a side action (open location, properties) and reports its failure. */
  readonly attempt: (work: () => Promise<void>) => void;
}

export const startupKey = (entry: StartupEntry): string => `${entry.source}:${entry.name}`;
export const serviceKey = (service: ServiceEntry): string => `service:${service.name}`;

/** `title` spread only when present: `exactOptionalPropertyTypes` rejects `undefined`. */
const titled = (title: string | undefined) => (title === undefined ? {} : { title });

/** Builds the descriptor lists. A hook because the labels need `t`. */
export function useRowMenus(context: RowMenuContext): {
  readonly startup: (entry: StartupEntry) => RowMenuModel;
  readonly service: (service: ServiceEntry) => RowMenuModel;
} {
  const { t } = useTranslation(STARTUP_NS);
  const { actions, busyKey, request, attempt } = context;

  const fileNodes = (path: string | null): MenuNode[] => {
    const noPath = path === null ? t('reason.noPath') : undefined;
    return [
      {
        kind: 'item',
        id: 'openLocation',
        label: t('action.openLocation'),
        disabled: path === null,
        ...titled(noPath),
        onSelect: () => {
          if (path !== null) attempt(() => actions.openFileLocation(path));
        },
      },
      {
        kind: 'item',
        id: 'properties',
        label: t('action.properties'),
        disabled: path === null,
        ...titled(noPath),
        onSelect: () => {
          if (path !== null) attempt(() => actions.showFileProperties(path));
        },
      },
    ];
  };

  const lookupNodes = (label: string, details: string): MenuNode[] => [
    {
      kind: 'item',
      id: 'searchOnline',
      label: t('action.searchOnline'),
      onSelect: () => {
        globalThis.open?.(
          `https://duckduckgo.com/?q=${encodeURIComponent(label)}`,
          '_blank',
          'noopener,noreferrer',
        );
      },
    },
    {
      kind: 'item',
      id: 'copyDetails',
      label: t('action.copyDetails'),
      onSelect: () => {
        void globalThis.navigator?.clipboard?.writeText(details);
      },
    },
  ];

  const startup = (entry: StartupEntry): RowMenuModel => {
    const key = startupKey(entry);
    const label = labelFor(entry);
    const busy = busyKey === key;
    // `unknown` offers Disable: the entry might be running, and the user who
    // opened this menu almost certainly wants it not to.
    const enabling = entry.state === 'disabled';
    const verb: Verb = enabling ? 'enable' : 'disable';

    const blocked =
      entry.risk === 'forbidden'
        ? t('reason.forbidden')
        : isRunOnce(entry)
          ? t('reason.runOnce')
          : undefined;
    const hint = blocked ?? (isMachineWide(entry) ? t('reason.elevation') : undefined);

    return {
      key,
      label,
      nodes: [
        {
          kind: 'item',
          id: 'toggle',
          label: t(`action.${verb}`),
          disabled: busy || blocked !== undefined,
          ...titled(hint),
          onSelect: () => {
            request({
              key,
              name: label,
              verb,
              reduces: !enabling,
              risk: entry.risk,
              run: (confirmed) => actions.setStartupEnabled(entry, enabling, confirmed),
            });
          },
        },
        { kind: 'separator', id: 'sep-file' },
        ...fileNodes(entry.imagePath).map((node) =>
          node.kind === 'item' && busy ? { ...node, disabled: true } : node,
        ),
        { kind: 'separator', id: 'sep-lookup' },
        ...lookupNodes(label, `${label}\t${entry.source}\t${entry.command ?? '—'}`),
      ],
    };
  };

  const service = (svc: ServiceEntry): RowMenuModel => {
    const key = serviceKey(svc);
    const label = labelFor(svc);
    const busy = busyKey === key;
    const forbidden = svc.risk === 'forbidden';
    // Every service change goes through the SCM, which needs administrator
    // rights, so the hint is on every control rather than only machine-wide
    // startup entries.
    const hint = forbidden ? t('reason.forbidden') : t('reason.elevation');
    const active = svc.state === 'running' || svc.state === 'paused';
    // Boot and System start even earlier than Automatic, so moving any of
    // them to Manual reduces what runs and gets the same warning.
    const startsItself =
      svc.startType === 'automatic' || svc.startType === 'boot' || svc.startType === 'system';

    const control = (verb: 'start' | 'stop' | 'restart', allowed: boolean): MenuNode => ({
      kind: 'item',
      id: verb,
      label: t(`action.${verb}`),
      disabled: busy || forbidden || !allowed,
      title: hint,
      onSelect: () => {
        request({
          key,
          name: label,
          verb,
          reduces: verb !== 'start',
          risk: svc.risk,
          run: (confirmed) => actions.controlService(svc, verb, confirmed),
        });
      },
    });

    const startTypes: readonly ServiceStartType[] = ['automatic', 'manual', 'disabled'];
    const current = (startTypes as readonly string[]).includes(svc.startType) ? svc.startType : '';

    return {
      key,
      label,
      nodes: [
        control('start', svc.state === 'stopped'),
        control('stop', active),
        control('restart', active),
        {
          kind: 'radio',
          id: 'startType',
          label: t('action.startType'),
          value: current,
          options: startTypes.map((value) => ({ value, label: t(`action.${value}`) })),
          disabled: busy || forbidden,
          title: hint,
          onValueChange: (value) => {
            const next = startTypes.find((candidate) => candidate === value);
            // Re-selecting the current type is a no-op, not a round trip
            // through UAC to write the value that is already there.
            if (next === undefined || next === svc.startType) return;
            request({
              key,
              name: label,
              verb: next,
              reduces: next === 'disabled' || (next === 'manual' && startsItself),
              risk: svc.risk,
              run: (confirmed) => actions.setServiceStartType(svc, next, confirmed),
            });
          },
        },
        { kind: 'separator', id: 'sep-file' },
        ...fileNodes(svc.imagePath).map((node) =>
          node.kind === 'item' && busy ? { ...node, disabled: true } : node,
        ),
        { kind: 'separator', id: 'sep-lookup' },
        ...lookupNodes(label, `${label}\t${svc.name}\t${svc.binaryPath ?? '—'}`),
      ],
    };
  };

  return { startup, service };
}

export function RowContextMenu({ menu }: { readonly menu: RowMenuModel }): React.JSX.Element {
  return (
    <ContextMenuContent className="min-w-52">
      <ContextMenuLabel>{menu.label}</ContextMenuLabel>
      <ContextMenuSeparator />
      {menu.nodes.map((node) => {
        if (node.kind === 'separator') return <ContextMenuSeparator key={node.id} />;
        if (node.kind === 'item') {
          return (
            <ContextMenuItem
              key={node.id}
              disabled={node.disabled === true}
              {...titled(node.title)}
              onSelect={node.onSelect}
            >
              {node.label}
            </ContextMenuItem>
          );
        }
        return (
          <ContextMenuSub key={node.id}>
            <ContextMenuSubTrigger disabled={node.disabled === true} {...titled(node.title)}>
              {node.label}
            </ContextMenuSubTrigger>
            <ContextMenuSubContent>
              <ContextMenuRadioGroup value={node.value} onValueChange={node.onValueChange}>
                {node.options.map((option) => (
                  <ContextMenuRadioItem key={option.value} value={option.value}>
                    {option.label}
                  </ContextMenuRadioItem>
                ))}
              </ContextMenuRadioGroup>
            </ContextMenuSubContent>
          </ContextMenuSub>
        );
      })}
    </ContextMenuContent>
  );
}

export function RowDropdownMenu({ menu }: { readonly menu: RowMenuModel }): React.JSX.Element {
  return (
    <DropdownMenuContent align="end" className="min-w-52">
      <DropdownMenuLabel>{menu.label}</DropdownMenuLabel>
      <DropdownMenuSeparator />
      {menu.nodes.map((node) => {
        if (node.kind === 'separator') return <DropdownMenuSeparator key={node.id} />;
        if (node.kind === 'item') {
          return (
            <DropdownMenuItem
              key={node.id}
              disabled={node.disabled === true}
              {...titled(node.title)}
              onSelect={node.onSelect}
            >
              {node.label}
            </DropdownMenuItem>
          );
        }
        return (
          <DropdownMenuSub key={node.id}>
            <DropdownMenuSubTrigger disabled={node.disabled === true} {...titled(node.title)}>
              {node.label}
            </DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuRadioGroup value={node.value} onValueChange={node.onValueChange}>
                {node.options.map((option) => (
                  <DropdownMenuRadioItem key={option.value} value={option.value}>
                    {option.label}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
        );
      })}
    </DropdownMenuContent>
  );
}
