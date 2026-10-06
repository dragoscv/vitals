/**
 * Users screen: logon sessions and their resource consumption.
 *
 * Mirrors Task Manager's Users tab: every session on this machine, its state,
 * and what it is consuming. Session 0 and listeners are included and
 * classified rather than hidden, so their presence is legible.
 *
 * # No value is ever a plausible stand-in
 *
 * Where a figure could not be obtained (logon time, process count without a
 * rollup), the screen prints "Not available" or an explicit placeholder. Not 0,
 * not a dash alone — each of those is a fabrication the user would act on.
 */

import { Copy, RefreshCw, ShieldAlert, User } from 'lucide-react';
import { forwardRef, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import {
  Badge,
  Button,
  Card,
  CardBody,
  CardHeader,
  CardTitle,
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
  ContextMenuTrigger,
  EmptyState,
  Input,
  Skeleton,
  formatBytes,
  formatPercent,
} from '@vitals/ui';

import { useRowMenu } from '../../lib/useRowMenu';
import {
  displayName,
  filterSessions,
  isInteractive,
  rollupFor,
  sortSessions,
  type LogonSession,
} from './model';
import { USERS_NS } from './strings';
import { NO_HOST, useUsers, type UsersReader } from './useUsers';

export interface UsersScreenProps {
  /** Injectable so tests and the sampler-less preview need no Tauri host. */
  readonly reader?: UsersReader;
}

export function UsersScreen({ reader }: UsersScreenProps): React.JSX.Element {
  const { t } = useTranslation(USERS_NS);
  const state = useUsers(reader);
  const [searchQuery, setSearchQuery] = useState('');
  const menu = useRowMenu();

  const filteredSessions = useMemo(() => {
    if (state.snapshot === null) return [];
    const filtered = filterSessions(state.snapshot.sessions, searchQuery);
    return sortSessions(filtered);
  }, [state.snapshot, searchQuery]);

  const interactiveCount = useMemo(() => {
    return filteredSessions.filter(isInteractive).length;
  }, [filteredSessions]);

  if (state.pending && state.snapshot === null) return <UsersSkeleton />;

  if (state.error === NO_HOST) {
    return (
      <EmptyState icon={<ShieldAlert />} title={t('noHost.title')} description={t('noHost.body')} />
    );
  }

  // If there's an error and no snapshot, show an error state instead of empty.
  if (state.error !== null && state.snapshot === null) {
    return <EmptyState icon={<ShieldAlert />} title={t('error.title')} description={state.error} />;
  }

  if (state.snapshot === null || state.snapshot.sessions.length === 0) {
    return <EmptyState icon={<User />} title={t('empty.title')} description={t('empty.body')} />;
  }

  return (
    <div className="screen">
      <header className="flex flex-wrap items-start justify-between gap-2">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold">{t('title')}</h2>
          <p className="text-2xs text-[var(--color-fg-muted)]">{t('subtitle')}</p>
        </div>
        <Button variant="ghost" size="sm" onClick={state.refresh}>
          <RefreshCw aria-hidden className="size-4" />
          {t('refresh')}
        </Button>
      </header>

      {state.error !== null && state.error !== NO_HOST && (
        <p role="alert" className="text-2xs text-[var(--color-status-danger)]">
          {state.error}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Input
          type="search"
          placeholder={t('search')}
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          className="max-w-xs"
        />
        {searchQuery && (
          <Button variant="ghost" size="sm" onClick={() => setSearchQuery('')}>
            {t('clear')}
          </Button>
        )}
      </div>

      <p className="text-2xs text-[var(--color-fg-subtle)]">
        {t('counts.sessions', { count: filteredSessions.length })}
        {' · '}
        {t('counts.interactive', { count: interactiveCount })}
      </p>

      {/* Session cards at their own height; the body scrolls (S16-17). */}
      <div className="screen-body" onKeyDown={menu.onKeyDown}>
        {filteredSessions.map((session) => (
          <ContextMenu key={session.sessionId} {...menu.rootProps(String(session.sessionId))}>
            <ContextMenuTrigger asChild>
              <SessionCard
                session={session}
                rollup={rollupFor(session, state.snapshot?.rollups ?? [])}
                onContextMenu={menu.onContextMenu}
              />
            </ContextMenuTrigger>
            <SessionMenu session={session} onRefresh={state.refresh} />
          </ContextMenu>
        ))}
      </div>
    </div>
  );
}

function SessionMenu({
  session,
  onRefresh,
}: {
  readonly session: LogonSession;
  readonly onRefresh: () => void;
}) {
  const { t } = useTranslation(USERS_NS);
  const name = displayName(session);
  const copy = (text: string): void => {
    void globalThis.navigator?.clipboard?.writeText(text);
  };
  return (
    <ContextMenuContent>
      <ContextMenuLabel>{name}</ContextMenuLabel>
      <ContextMenuItem
        onSelect={() => {
          copy(name);
        }}
      >
        <Copy className="size-4" aria-hidden="true" />
        {t('menu.copyName')}
      </ContextMenuItem>
      <ContextMenuItem
        onSelect={() => {
          copy(
            [
              name,
              `${t('column.sessionId')} ${String(session.sessionId)}`,
              t(`state.${session.state}`),
              session.clientName ?? '—',
            ].join('\t'),
          );
        }}
      >
        <Copy className="size-4" aria-hidden="true" />
        {t('menu.copyDetails')}
      </ContextMenuItem>
      <ContextMenuSeparator />
      <ContextMenuItem onSelect={onRefresh}>
        <RefreshCw className="size-4" aria-hidden="true" />
        {t('refresh')}
      </ContextMenuItem>
    </ContextMenuContent>
  );
}

/**
 * Forwards ref and props: the card is the `ContextMenuTrigger asChild`
 * target, so Radix's handlers and ref have to reach the `Card` element.
 */
const SessionCard = forwardRef<
  HTMLDivElement,
  {
    readonly session: LogonSession;
    readonly rollup: ReturnType<typeof rollupFor>;
  } & Omit<React.ComponentPropsWithoutRef<'div'>, 'children' | 'className' | 'title'>
>(function SessionCard({ session, rollup, ...rest }, ref) {
  const { t, i18n } = useTranslation(USERS_NS);

  const logonTimeLabel = useMemo(() => {
    if (session.logonTime === null) return null;
    const date = new Date(session.logonTime * 1000);
    return new Intl.DateTimeFormat(i18n.language, {
      dateStyle: 'short',
      timeStyle: 'short',
    }).format(date);
  }, [session.logonTime, i18n.language]);

  const stateColor = useMemo(() => {
    switch (session.state) {
      case 'active':
      case 'connected':
        return 'success';
      case 'disconnected':
        return 'warning';
      case 'idle':
      case 'listen':
        return 'default';
      default:
        return 'danger';
    }
  }, [session.state]);

  return (
    <Card ref={ref} regionLabel={`${displayName(session)} session`} {...rest}>
      <CardHeader>
        <CardTitle level={3}>
          <span className="inline-flex items-center gap-1.5">
            <User aria-hidden className="size-4" />
            {displayName(session)}
          </span>
        </CardTitle>
        <div className="flex flex-wrap items-center gap-2">
          <Badge color={stateColor}>{t(`state.${session.state}`)}</Badge>
          {session.clientName && (
            <Badge color="info" title={t('remote')}>
              {session.clientName}
            </Badge>
          )}
          {session.isServices && <Badge color="default">{t('counts.services')}</Badge>}
        </div>
      </CardHeader>
      <CardBody>
        <dl className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <Field label={t('column.sessionId')} value={String(session.sessionId)} />
          <Field
            label={t('column.logonTime')}
            value={logonTimeLabel}
            unavailableHint={t('logonTimeUnavailable')}
          />
          <Field
            label={t('column.processes')}
            value={rollup ? String(rollup.processCount) : null}
          />
          <Field
            label={t('column.cpu')}
            value={rollup ? formatPercent(rollup.cpuPercent / 100, i18n.language, 1) : null}
          />
          <Field
            label={t('column.memory')}
            value={rollup ? formatBytes(rollup.memoryBytes, i18n.language) : null}
          />
        </dl>
      </CardBody>
    </Card>
  );
});

/**
 * A label and its value. A missing value is the em dash used everywhere in
 * Vitals, with the words for it visually hidden: `aria-label` on a plain
 * `span` is not reliably announced, so the dash alone used to read as
 * nothing at all to a screen reader.
 */
function Field({
  label,
  value,
  unavailableHint,
}: {
  readonly label: string;
  readonly value: string | null;
  readonly unavailableHint?: string;
}) {
  const { t } = useTranslation(USERS_NS);

  return (
    <div className="min-w-0">
      <dt className="text-2xs text-[var(--color-fg-muted)]">{label}</dt>
      <dd className="truncate text-sm">
        {value ?? (
          <span
            className="text-[var(--color-fg-subtle)]"
            {...(unavailableHint ? { title: unavailableHint } : {})}
          >
            <span aria-hidden="true">—</span>
            <span className="sr-only">{unavailableHint ?? t('unavailable')}</span>
          </span>
        )}
      </dd>
    </div>
  );
}

function UsersSkeleton() {
  // `common.*` lives in the default namespace, not in USERS_NS; there is no
  // fallbackNS, so the wrong `t` renders the key path as the label.
  const { t } = useTranslation();
  return (
    <div className="flex flex-col gap-4" aria-busy="true">
      <div className="flex justify-between">
        <div className="flex flex-col gap-1">
          <Skeleton className="h-6 w-24" />
          <Skeleton className="h-4 w-64" />
        </div>
        <Skeleton className="h-9 w-24" />
      </div>
      <Skeleton className="h-10 w-full max-w-xs" />
      <Skeleton className="h-4 w-48" />
      <div className="flex flex-col gap-3">
        {[1, 2, 3].map((i) => (
          <Card key={i} regionLabel={t('common.loading')}>
            <CardHeader>
              <Skeleton className="h-6 w-32" />
              <div className="flex gap-2">
                <Skeleton className="h-6 w-16" />
                <Skeleton className="h-6 w-20" />
              </div>
            </CardHeader>
            <CardBody>
              <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                {[1, 2, 3, 4, 5].map((j) => (
                  <div key={j} className="flex flex-col gap-1">
                    <Skeleton className="h-3 w-20" />
                    <Skeleton className="h-4 w-24" />
                  </div>
                ))}
              </div>
            </CardBody>
          </Card>
        ))}
      </div>
    </div>
  );
}
