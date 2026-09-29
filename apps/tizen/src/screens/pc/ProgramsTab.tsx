/**
 * The process list on the left, the chosen one on the right with the same
 * actions the phone offers. No search box: typing on a TV keyboard to find a
 * program is slower than sorting and scrolling, and CPU order puts the one a
 * person is looking for at the top.
 */

import type { ControlPriority, ControlRequest, VitalsClient } from '@vitals/client';
import { processKeyId, type Process } from '@vitals/protocol';
import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { controlFailure } from '../../lib/api';
import { bytes, duration, percent, rate } from '../../lib/format';
import type { LiveState } from '../../lib/live';
import type { Pairing } from '../../lib/pairing';
import {
  SHOWN,
  inEfficiencyMode,
  isSafelyTerminable,
  sortProcesses,
  type SortKey,
} from '../../lib/processes';
import { useApp } from '../../ui/app-context';
import { Action, Choice, Hint, InfoRow, Panel } from '../../ui/components';
import { Palette, Thresholds } from '../../ui/theme';

const SORTS: readonly SortKey[] = ['cpu', 'memory', 'name'];
// Real-time is refused by the desktop from a remote, so it is not offered.
const PRIORITIES: readonly ControlPriority[] = [
  'idle',
  'below-normal',
  'normal',
  'above-normal',
  'high',
];
/** The confirming press must follow the first within this, or the arming lapses. */
const CONFIRM_MS = 4_000;

export function ProgramsTab({
  pairing,
  state,
  client,
}: {
  pairing: Pairing;
  state: LiveState;
  client: VitalsClient;
}) {
  const { t, i18n } = useTranslation();
  const [sort, setSort] = useState<SortKey>('cpu');
  const [selected, setSelected] = useState<string | null>(null);
  const rows = useMemo(
    () => (state.processes === null ? null : sortProcesses(state.processes.values(), sort)),
    [state.processes, sort],
  );
  const chosen = selected === null ? undefined : state.processes?.get(selected);

  return (
    <div className="programs">
      <div className="programs-list">
        <div className="choice-row">
          <span className="choice-label">{t('sort.label')}</span>
          {SORTS.map((s) => (
            <Choice
              key={s}
              label={t(`sort.${s}`)}
              selected={sort === s}
              onPress={() => setSort(s)}
            />
          ))}
        </div>
        {rows === null ? (
          <Hint>
            {t(state.status === 'unreachable' ? 'state.unreachable' : 'processes.waiting')}
          </Hint>
        ) : rows.length === 0 ? (
          <Hint>{t('processes.none')}</Hint>
        ) : (
          <>
            <Hint>
              {t('processes.shown', { shown: Math.min(SHOWN, rows.length), total: rows.length })}
            </Hint>
            <div className="rows">
              {rows.slice(0, SHOWN).map((p) => {
                const id = processKeyId(p.key);
                return (
                  <button
                    type="button"
                    key={id}
                    data-key={id}
                    className={`row ${id === selected ? 'is-selected' : ''}`}
                    aria-pressed={id === selected}
                    onClick={() => setSelected(id)}
                  >
                    <span className="row-name">
                      {p.name}
                      {p.state === 'suspended' && (
                        <em className="tone-warn"> · {t('processes.suspended')}</em>
                      )}
                      {p.state === 'notResponding' && (
                        <em className="tone-warn"> · {t('processes.notResponding')}</em>
                      )}
                    </span>
                    <span className="row-num">{percent(p.cpu)}</span>
                    <span className="row-num wide">{bytes(p.memoryPrivate, i18n.language)}</span>
                  </button>
                );
              })}
            </div>
          </>
        )}
      </div>
      <div className="programs-panel">
        {chosen !== undefined ? (
          <ProcessPanel key={selected} pairing={pairing} process={chosen} client={client} />
        ) : (
          <Hint>{t('processes.pick')}</Hint>
        )}
      </div>
    </div>
  );
}

function ProcessPanel({
  pairing,
  process,
  client,
}: {
  pairing: Pairing;
  process: Process;
  client: VitalsClient;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const { pairings } = useApp();
  const [busy, setBusy] = useState(false);
  const [armed, setArmed] = useState(false);
  const [outcome, setOutcome] = useState<{ ok: boolean; text: string } | null>(null);

  // A panel left open must not be one stray OK away from ending a program.
  useEffect(() => {
    if (!armed) return;
    const timer = setTimeout(() => setArmed(false), CONFIRM_MS);
    return () => clearTimeout(timer);
  }, [armed]);

  const run = (request: ControlRequest) => {
    if (busy) return;
    setBusy(true);
    setArmed(false);
    client.control(request).then(
      () => {
        setBusy(false);
        setOutcome({ ok: true, text: t('control.done') });
        if (pairing.scope === 'unknown') pairings.setScope(pairing.id, 'control');
      },
      (error: unknown) => {
        setBusy(false);
        const failure = controlFailure(error);
        // The server's 403 `forbidden` is how a pairing learns it is read-only;
        // the actions are hidden from then on.
        if (failure.readOnly) pairings.setScope(pairing.id, 'read');
        setOutcome({ ok: false, text: t(failure.key, failure.values ?? {}) });
      },
    );
  };

  const suspended = process.state === 'suspended';
  const efficient = inEfficiencyMode(process);
  return (
    <Panel title={process.name} accent={Palette.cpu}>
      <InfoRow
        label={t('sort.cpu')}
        value={percent(process.cpu)}
        tone={process.cpu >= 85 ? Thresholds.cpu(process.cpu) : 'plain'}
      />
      <InfoRow label={t('metric.memory')} value={bytes(process.memoryPrivate, lang)} />
      <InfoRow
        label={t('processes.disk')}
        value={t('disk.readWrite', {
          read: rate(process.diskRead, lang),
          write: rate(process.diskWrite, lang),
        })}
      />
      <InfoRow label={t('metric.gpu')} value={percent(process.gpu)} />
      <InfoRow label={t('processes.threads')} value={String(process.threadCount)} />
      <InfoRow label={t('processes.uptime')} value={duration(process.uptimeSecs)} />
      <InfoRow label={t('processes.user')} value={process.user} />
      <InfoRow label={t('processes.pid')} value={String(process.key.pid)} />
      {pairing.scope === 'read' ? (
        <Hint>{t('processes.readOnly')}</Hint>
      ) : !isSafelyTerminable(process) ? (
        <Hint>{t('processes.protected')}</Hint>
      ) : (
        <div className="stack">
          <div className="choice-row">
            <Action
              label={t(armed ? 'action.endConfirm' : 'action.end')}
              danger
              disabled={busy}
              autoFocus
              onPress={() =>
                armed ? run({ action: 'terminate', key: process.key }) : setArmed(true)
              }
            />
            <Action
              label={t(suspended ? 'action.resume' : 'action.suspend')}
              disabled={busy}
              onPress={() => run({ action: suspended ? 'resume' : 'suspend', key: process.key })}
            />
          </div>
          <h4>{t('action.priority')}</h4>
          <div className="choice-row wrap">
            {PRIORITIES.map((priority) => (
              // The wire does not report the current priority, so none is shown as selected.
              <Choice
                key={priority}
                label={t(`priority.${priority}`)}
                selected={false}
                onPress={() => run({ action: 'set-priority', key: process.key, priority })}
              />
            ))}
          </div>
          <h4>{t('action.efficiency')}</h4>
          <Hint>{t('action.efficiencyBody')}</Hint>
          <Action
            label={t(efficient ? 'action.efficiencyOff' : 'action.efficiencyOn')}
            disabled={busy}
            onPress={() =>
              run({ action: 'set-efficiency-mode', key: process.key, enabled: !efficient })
            }
          />
        </div>
      )}
      {outcome !== null && (
        <p className={`outcome tone-${outcome.ok ? 'ok' : 'danger'}`} role="status">
          {outcome.text}
        </p>
      )}
    </Panel>
  );
}
