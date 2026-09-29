/**
 * The home screen: this TV and every paired PC, each a focusable card with
 * its headline rings. What a person glances at from the sofa; Enter on a
 * card opens the whole machine.
 */

import { gpuPercent, headlineTemperature, memoryPercent } from '@vitals/protocol';
import { useTranslation } from 'react-i18next';

import { celsiusCompact, percentCompact } from '../lib/format';
import type { Pairing } from '../lib/pairing';
import { useApp, useLive, usePairings } from '../ui/app-context';
import { Hint, Ring, ScreenTitle, Spark } from '../ui/components';
import { PcStatus } from '../ui/pc-status';
import { Palette, Thresholds } from '../ui/theme';
import { useDeviceLive } from './device-live';

function TvCard({ onOpen, autoFocus }: { onOpen: () => void; autoFocus: boolean }) {
  const { t } = useTranslation();
  const live = useDeviceLive(true);
  const s = live.snapshot;
  const memory =
    s.memoryTotal !== null && s.memoryAvailable !== null && s.memoryTotal > 0
      ? ((s.memoryTotal - s.memoryAvailable) / s.memoryTotal) * 100
      : null;
  return (
    <button
      type="button"
      className="card card-wide"
      onClick={onOpen}
      {...(autoFocus && { 'data-autofocus': '' })}
    >
      <span className="card-title">{t('overview.tv')}</span>
      {!live.available ? (
        <Hint>{t('device.unavailable')}</Hint>
      ) : (
        <span className="card-row">
          <Ring
            fraction={s.cpuLoad === null ? null : s.cpuLoad / 100}
            colour={Palette.cpu}
            value={percentCompact(s.cpuLoad)}
            label={t('metric.cpu')}
            level={Thresholds.cpu(s.cpuLoad)}
          />
          <Ring
            fraction={memory === null ? null : memory / 100}
            colour={Palette.memory}
            value={percentCompact(memory)}
            label={t('metric.memoryShort')}
            level={Thresholds.memory(memory)}
          />
          <span className="card-sparks">
            <Spark values={live.cpu} colour={Palette.cpu} label={t('metric.cpu')} />
            <Spark values={live.memory} colour={Palette.memory} label={t('metric.memoryShort')} />
            <span className="card-line">
              {t('device.connection')}:{' '}
              {s.networkType !== null
                ? t(`device.netType.${s.networkType}`, { defaultValue: s.networkType })
                : '—'}
              {s.ipAddress !== null ? ` · ${s.ipAddress}` : ''}
            </span>
          </span>
        </span>
      )}
    </button>
  );
}

export function PcCard({
  pairing,
  onOpen,
  autoFocus,
}: {
  pairing: Pairing;
  onOpen: () => void;
  autoFocus?: boolean;
}) {
  const { t } = useTranslation();
  const { state } = useLive(pairing);
  const system = state.status === 'live' ? state.system : null;
  const memory = system !== null ? memoryPercent(system) : null;
  const gpu = system !== null ? gpuPercent(system) : null;
  const temp = system !== null ? headlineTemperature(system) : null;
  const cpu = system?.cpu.total ?? null;
  return (
    <button
      type="button"
      className="card card-pc"
      onClick={onOpen}
      data-pc={pairing.id}
      {...(autoFocus === true && { 'data-autofocus': '' })}
    >
      <span className="card-title">{pairing.label}</span>
      <PcStatus state={state} />
      {system !== null && (
        <>
          <span className="card-row card-rings">
            <Ring
              size="small"
              fraction={cpu === null ? null : cpu / 100}
              colour={Palette.cpu}
              value={percentCompact(cpu)}
              label={t('metric.cpu')}
              level={Thresholds.cpu(cpu)}
            />
            <Ring
              size="small"
              fraction={memory === null ? null : memory / 100}
              colour={Palette.memory}
              value={percentCompact(memory)}
              label={t('metric.memoryShort')}
              level={Thresholds.memory(memory)}
            />
            <Ring
              size="small"
              fraction={gpu === null ? null : gpu / 100}
              colour={Palette.gpu}
              value={percentCompact(gpu)}
              label={t('metric.gpu')}
            />
            <Ring
              size="small"
              fraction={temp === null ? null : temp / 110}
              colour={Palette.thermal}
              value={celsiusCompact(temp)}
              label={t('metric.temp')}
              level={Thresholds.cpuTemp(temp)}
            />
          </span>
          <Spark values={state.cpu} colour={Palette.cpu} label={t('metric.cpu')} />
        </>
      )}
    </button>
  );
}

export function OverviewScreen() {
  const { t } = useTranslation();
  const { nav } = useApp();
  const pairings = usePairings();
  return (
    <div className="screen">
      <ScreenTitle>{t('overview.title')}</ScreenTitle>
      <h2>{t('overview.tv')}</h2>
      <TvCard onOpen={() => nav.top('tv')} autoFocus={pairings.length === 0} />
      <h2>{t('overview.pcs')}</h2>
      {pairings.length === 0 ? (
        <Hint>{t('overview.noPcs')}</Hint>
      ) : (
        <div className="card-strip">
          {pairings.map((p, i) => (
            <PcCard
              key={p.id}
              pairing={p}
              autoFocus={i === 0}
              onOpen={() => nav.push({ name: 'pc', id: p.id })}
            />
          ))}
        </div>
      )}
    </div>
  );
}
