/**
 * The devcloud panel on the Projects tab: what this machine's agentq knows
 * about the cloud workspaces — how many runs went out in the window, and which
 * workspace is marked down.
 *
 * # Only the local half is shown
 *
 * A workspace's live status (heartbeat age, idle/busy, queue depth) and its
 * estimated cost do not reach Vitals today: heartbeat and queue live in GCS,
 * cost in the target repo's run journal, and Vitals reads only
 * `%USERPROFILE%\.codai\coord` (docs/codwen-projects-page.md §6.2). The brief's
 * recommendation is to ship the local half now — run counts and the
 * unavailable list, which agentq's journal and `cloud-down` records already
 * give the AI page — and to wait for a read-only agentq JSON verb before the
 * rest (§6.3).
 *
 * The fields that are not available are named as such, never shown as `0`: a
 * queue depth of "0" and "nobody measured it" are different facts (AGENTS.md).
 */

import { CloudOff, Cloudy } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import type { AgentsSnapshot } from '@vitals/protocol';

export function DevcloudPanel({
  snapshot,
}: {
  readonly snapshot: AgentsSnapshot;
}): React.JSX.Element {
  // The Projects strings live in the shared locale files, so the default
  // namespace is the one to translate through.
  const { t, i18n } = useTranslation();
  const { windowHours, cloudRuns } = snapshot.stats;
  const time = (ms: number | null): string =>
    ms === null ? '—' : new Intl.DateTimeFormat(i18n.language, { timeStyle: 'short' }).format(ms);

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

      <p className="text-2xs text-[var(--color-fg-subtle)]">{t('projects.devcloud.awaiting')}</p>
    </section>
  );
}
