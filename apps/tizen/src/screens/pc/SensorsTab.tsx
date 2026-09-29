/**
 * Every sensor the PC can read, grouped by kind. Polled every five seconds
 * while the tab is open — the host caches for five seconds, so faster would
 * only re-read the same answer — and not at all once it closes.
 */

import type { SensorLine } from '@vitals/protocol';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { fetchSensors } from '../../lib/api';
import { sensor } from '../../lib/format';
import type { Pairing } from '../../lib/pairing';
import { Hint, InfoRow, Panel } from '../../ui/components';
import { Palette } from '../../ui/theme';

const GROUPS = ['temperature', 'fanSpeed', 'power', 'voltage', 'percent'] as const;
const COLOURS: Record<string, string> = {
  temperature: Palette.thermal,
  fanSpeed: Palette.network,
  power: Palette.power,
  voltage: Palette.disk,
  percent: Palette.memory,
  other: Palette.accent,
};
const EVERY_MS = 5_000;

type Load = { kind: 'loading' } | { kind: 'failed' } | { kind: 'ok'; lines: SensorLine[] };

export function groupOf(line: SensorLine): string {
  if (line.unit === 'charge') return 'percent';
  return (GROUPS as readonly string[]).includes(line.unit) ? line.unit : 'other';
}

export function SensorsTab({ pairing }: { pairing: Pairing }) {
  const { t, i18n } = useTranslation();
  const [load, setLoad] = useState<Load>({ kind: 'loading' });

  useEffect(() => {
    let alive = true;
    const tick = () => {
      fetchSensors(pairing, fetch.bind(globalThis)).then(
        (lines) => alive && setLoad({ kind: 'ok', lines }),
        () => alive && setLoad((prev) => (prev.kind === 'ok' ? prev : { kind: 'failed' })),
      );
    };
    tick();
    const timer = setInterval(tick, EVERY_MS);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [pairing]);

  if (load.kind === 'loading') return <Hint>{t('sensors.loading')}</Hint>;
  if (load.kind === 'failed') return <Hint>{t('sensors.unavailable')}</Hint>;
  if (load.lines.length === 0) return <Hint>{t('sensors.empty')}</Hint>;

  const groups = new Map<string, SensorLine[]>();
  for (const line of load.lines) {
    const g = groupOf(line);
    groups.set(g, [...(groups.get(g) ?? []), line]);
  }
  const order = [...GROUPS, 'other'].filter((g) => groups.has(g));
  return (
    <div className="grid-2">
      {order.map((g) => (
        <Panel key={g} title={t(`sensors.group.${g}`)} accent={COLOURS[g] ?? Palette.accent}>
          {(groups.get(g) ?? []).map((line) => (
            <button type="button" key={line.key} className="panel-stop">
              <InfoRow label={line.label} value={sensor(line.unit, line.value, i18n.language)} />
            </button>
          ))}
        </Panel>
      ))}
    </div>
  );
}
