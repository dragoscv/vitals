import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { fetchHistory } from '../../lib/api';
import { celsius, percent } from '../../lib/format';
import { bucketHistory, peak, type HistorySeries } from '../../lib/history';
import type { Pairing } from '../../lib/pairing';
import { Choice, Hint, Panel, Spark } from '../../ui/components';
import { Palette } from '../../ui/theme';

const SPANS = [
  { key: '1h', secs: 3_600 },
  { key: '24h', secs: 86_400 },
  { key: '7d', secs: 604_800 },
] as const;

type Load =
  | { kind: 'loading' }
  | { kind: 'failed'; span: string }
  | { kind: 'ok'; span: string; series: HistorySeries; empty: boolean };

export function HistoryTab({ pairing }: { pairing: Pairing }) {
  const { t } = useTranslation();
  const [span, setSpan] = useState<(typeof SPANS)[number]>(SPANS[0]);
  const [loaded, setLoaded] = useState<Load>({ kind: 'loading' });
  // An answer for another span is stale: show "loading" until this span's arrives.
  const load: Load =
    loaded.kind !== 'loading' && loaded.span !== span.key ? { kind: 'loading' } : loaded;

  useEffect(() => {
    let alive = true;
    fetchHistory(pairing, span.secs, fetch.bind(globalThis)).then(
      (samples) => {
        if (!alive) return;
        const series = bucketHistory(samples, Date.now() / 1000, span.secs);
        setLoaded({ kind: 'ok', span: span.key, series, empty: samples.length === 0 });
      },
      () => alive && setLoaded({ kind: 'failed', span: span.key }),
    );
    return () => {
      alive = false;
    };
  }, [pairing, span]);

  const charts =
    load.kind === 'ok' && !load.empty
      ? [
          {
            title: t('metric.cpu'),
            values: load.series.cpu,
            colour: Palette.cpu,
            max: 100,
            fmt: percent,
          },
          {
            title: t('metric.memory'),
            values: load.series.memory,
            colour: Palette.memory,
            max: 100,
            fmt: percent,
          },
          {
            title: t('metric.gpu'),
            values: load.series.gpu,
            colour: Palette.gpu,
            max: 100,
            fmt: percent,
          },
          {
            title: t('metric.temp'),
            values: load.series.temperature,
            colour: Palette.thermal,
            max: 110,
            fmt: celsius,
          },
        ]
      : [];

  return (
    <div className="stack">
      <div className="choice-row">
        {SPANS.map((s) => (
          <Choice
            key={s.key}
            label={t(`history.${s.key}`)}
            selected={span.key === s.key}
            onPress={() => setSpan(s)}
          />
        ))}
      </div>
      {load.kind === 'loading' && <Hint>{t('history.loading')}</Hint>}
      {load.kind === 'failed' && <Hint>{t('history.failed')}</Hint>}
      {load.kind === 'ok' && load.empty && <Hint>{t('history.empty')}</Hint>}
      <div className="grid-2">
        {charts.map((c) => (
          <Panel key={c.title} title={c.title} accent={c.colour}>
            <button type="button" className="panel-stop stack" aria-label={c.title}>
              <Spark
                values={c.values}
                colour={c.colour}
                max={c.max}
                height="large"
                label={c.title}
              />
              <Hint>{t('history.peak', { value: c.fmt(peak(c.values)) })}</Hint>
            </button>
          </Panel>
        ))}
      </div>
    </div>
  );
}
