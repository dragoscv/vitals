/**
 * The devcloud panel on the Projects tab: the local half of the picture — how
 * many runs went out, and which workspace agentq has marked down — beside the
 * live reading of every workspace, read from agentq's own read-only status
 * document (ADR 0050).
 *
 * # Where the live figures come from
 *
 * `agentq-cloud.ps1 -Status -Json` prints one document listing every workspace
 * in agentq's registry (it ignores `-Ws`): its VM state, heartbeat age, idle
 * seconds, queue depth, and the workspaces it could not measure. A Rust command
 * runs it through the same guards the agentq actions use and returns the parsed
 * document; this panel only draws it. The webview spawns nothing.
 *
 * # Nothing is invented
 *
 * A figure the document did not carry is `null` and renders as an em dash,
 * never `0` — a queue depth of "0" and "nobody measured it" are different
 * facts (AGENTS.md). The document does not carry a cost at all, so that column
 * is an em dash too. When the document cannot be read the panel says why
 * instead of drawing zeroes, because that is what a machine without agentq
 * sees — the default this panel is written around.
 */

import { CloudOff, Cloudy, RefreshCw } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import type { AgentsSnapshot } from '@vitals/protocol';
import { Button } from '@vitals/ui';

import { useCloudStatus, type CloudStatusSource, type DevcloudWorkspace } from './useCloudStatus';

/** `null`/`undefined` (unmeasured) → an em dash; a number → its own text. */
function measured(value: number | null | undefined): string {
  return typeof value === 'number' ? String(value) : '—';
}

/** The i18n key naming a workspace state, defaulting to "unknown". */
function stateKey(state: string): string {
  switch (state) {
    case 'idle':
      return 'projects.devcloud.stateIdle';
    case 'busy':
      return 'projects.devcloud.stateBusy';
    case 'suspended':
      return 'projects.devcloud.stateSuspended';
    case 'stopped':
      return 'projects.devcloud.stateStopped';
    case 'suspending':
      return 'projects.devcloud.stateSuspending';
    default:
      return 'projects.devcloud.stateUnknown';
  }
}

/** The i18n key explaining why the document could not be read. */
function failureKey(kind: string): string {
  switch (kind) {
    case 'noAgentq':
      return 'projects.devcloud.failureNoAgentq';
    case 'blocked':
      return 'projects.devcloud.failureBlocked';
    case 'unparsable':
      return 'projects.devcloud.failureUnparsable';
    default:
      return 'projects.devcloud.failureFailed';
  }
}

export function DevcloudPanel({
  snapshot,
  source,
}: {
  readonly snapshot: AgentsSnapshot;
  readonly source?: CloudStatusSource;
}): React.JSX.Element {
  // The Projects strings live in the shared locale files, so the default
  // namespace is the one to translate through.
  const { t, i18n } = useTranslation();
  const { state, refresh } = useCloudStatus(source);
  const { windowHours, cloudRuns } = snapshot.stats;
  const time = (ms: number | null): string =>
    ms === null ? '—' : new Intl.DateTimeFormat(i18n.language, { timeStyle: 'short' }).format(ms);
  const ago = (value: number | null | undefined): string =>
    typeof value === 'number' ? t('projects.devcloud.agoSeconds', { count: value }) : '—';
  const bare = (value: number | null | undefined): string =>
    typeof value === 'number' ? t('projects.devcloud.seconds', { count: value }) : '—';

  const row = (ws: DevcloudWorkspace): React.JSX.Element => (
    <li
      key={ws.ws}
      className="flex flex-wrap items-center gap-x-3 gap-y-0.5 rounded-lg border border-[var(--color-border-subtle)] bg-[var(--color-bg-raised)] px-3 py-2 text-2xs"
    >
      <span className="font-medium">{ws.ws}</span>
      <span className="text-[var(--color-fg-muted)]">{t(stateKey(ws.state))}</span>
      <span className="text-[var(--color-fg-muted)]">
        {t('projects.devcloud.heartbeat', { value: ago(ws.heartbeatAgeSec) })}
      </span>
      <span className="text-[var(--color-fg-muted)]">
        {t('projects.devcloud.idleFor', { value: bare(ws.idleSec) })}
      </span>
      <span className="text-[var(--color-fg-muted)]">
        {t('projects.devcloud.queue', { value: measured(ws.queueDepth) })}
      </span>
      <span className="text-[var(--color-fg-muted)]">
        {t('projects.devcloud.cost', { value: measured(ws.costMilliEur) })}
      </span>
    </li>
  );

  return (
    <section
      aria-label={t('projects.devcloud.title')}
      className="flex flex-col gap-2 rounded-xl border border-[var(--color-border-subtle)] bg-[var(--surface-card)] p-3"
    >
      <h3 className="flex items-center gap-2 text-sm font-semibold">
        <Cloudy aria-hidden className="size-4 text-[var(--color-fg-muted)]" />
        {t('projects.devcloud.title')}
      </h3>

      <p className="text-2xs text-[var(--color-fg-muted)]">
        {t('projects.devcloud.runs', { count: cloudRuns, hours: windowHours })}
      </p>

      {snapshot.cloudDown.length === 0 ? (
        <p className="inline-flex items-center gap-2 text-2xs text-[var(--color-ok-text)]">
          <Cloudy aria-hidden className="size-3.5" />
          {t('projects.devcloud.healthy')}
        </p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {snapshot.cloudDown.map((down) => (
            <li
              key={down.workspace}
              role="status"
              className="flex flex-col gap-0.5 rounded-lg border border-[var(--color-status-warn)]/60 bg-[var(--color-bg-raised)] px-3 py-2 text-2xs"
            >
              <span className="inline-flex items-center gap-2 font-medium text-[var(--color-warn-text)]">
                <CloudOff aria-hidden className="size-3.5" />
                {t('projects.devcloud.down', {
                  ws: down.workspace,
                  until: time(down.untilMs),
                })}
              </span>
              {down.reason !== null && (
                <span className="text-[var(--color-fg-muted)]">
                  {t('projects.devcloud.downReason', { reason: down.reason })}
                </span>
              )}
            </li>
          ))}
        </ul>
      )}

      <div className="flex flex-col gap-1.5 border-t border-[var(--color-border-subtle)] pt-2">
        <div className="flex items-center justify-between gap-2">
          <h4 className="text-2xs font-medium tracking-wide text-[var(--color-fg-subtle)] uppercase">
            {t('projects.devcloud.liveTitle')}
          </h4>
          <Button variant="ghost" size="sm" onClick={refresh} disabled={state.status === 'loading'}>
            <RefreshCw aria-hidden className="size-3.5" />
            {t('projects.devcloud.refresh')}
          </Button>
        </div>

        {state.status === 'loading' && (
          <p role="status" className="text-2xs text-[var(--color-fg-subtle)]">
            {t('projects.devcloud.reading')}
          </p>
        )}

        {state.status === 'error' && (
          <p role="status" className="text-2xs text-[var(--color-warn-text)]">
            {t(
              state.error === 'no-host'
                ? 'projects.devcloud.noHost'
                : 'projects.devcloud.unreachable',
            )}
          </p>
        )}

        {state.status === 'ok' &&
          (state.data.available ? (
            <>
              <ul className="flex flex-col gap-1.5">
                {(state.data.workspaces ?? []).map((ws) => row(ws))}
              </ul>
              {(state.data.unreadable ?? []).map((u) => (
                <p key={u.ws} className="text-2xs text-[var(--color-fg-subtle)]">
                  {t('projects.devcloud.unreadable', { ws: u.ws, why: u.why })}
                </p>
              ))}
              <p className="text-2xs text-[var(--color-fg-subtle)]">
                {t('projects.devcloud.dashNote')}
              </p>
            </>
          ) : (
            <>
              <p role="status" className="text-2xs text-[var(--color-fg-muted)]">
                {t(failureKey(state.data.failure?.kind ?? 'failed'))}
              </p>
              {state.data.failure?.detail ? (
                <p className="text-2xs text-[var(--color-fg-subtle)]">
                  {state.data.failure.detail}
                </p>
              ) : null}
            </>
          ))}
      </div>
    </section>
  );
}
