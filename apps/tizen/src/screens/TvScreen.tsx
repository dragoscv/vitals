/**
 * This TV, as far as `tizen.systeminfo` lets a web app see it. The limits
 * are stated on screen rather than hidden behind empty rows.
 */

import { useTranslation } from 'react-i18next';

import { bytes, percent } from '../lib/format';
import { Bar, Hint, InfoRow, Panel, Ring, ScreenTitle, Spark } from '../ui/components';
import { Palette, Thresholds } from '../ui/theme';
import { useDeviceLive } from './device-live';

export function TvScreen() {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const live = useDeviceLive(true);
  const s = live.snapshot;
  const used =
    s.memoryTotal !== null && s.memoryAvailable !== null ? s.memoryTotal - s.memoryAvailable : null;
  const memoryPct =
    used !== null && s.memoryTotal !== null && s.memoryTotal > 0
      ? (used / s.memoryTotal) * 100
      : null;

  return (
    <div className="screen">
      <ScreenTitle>{t('nav.tv')}</ScreenTitle>
      {!live.available && <Hint>{t('device.unavailable')}</Hint>}
      <div className="grid-3">
        <Panel title={t('device.cpu')} accent={Palette.cpu}>
          <div className="panel-row">
            <Ring
              fraction={s.cpuLoad === null ? null : s.cpuLoad / 100}
              colour={Palette.cpu}
              value={percent(s.cpuLoad)}
              label={t('device.load')}
              level={Thresholds.cpu(s.cpuLoad)}
            />
            <Spark values={live.cpu} colour={Palette.cpu} label={t('device.load')} height="large" />
          </div>
        </Panel>
        <Panel title={t('device.memory')} accent={Palette.memory}>
          <div className="panel-row">
            <Ring
              fraction={memoryPct === null ? null : memoryPct / 100}
              colour={Palette.memory}
              value={percent(memoryPct)}
              label={t('metric.memoryShort')}
              level={Thresholds.memory(memoryPct)}
            />
            <Spark
              values={live.memory}
              colour={Palette.memory}
              label={t('device.memory')}
              height="large"
            />
          </div>
          <InfoRow
            label={t('metric.memory')}
            value={t('memory.usedOf', {
              used: bytes(used, lang),
              total: bytes(s.memoryTotal, lang),
            })}
          />
          <InfoRow label={t('device.memoryFree')} value={bytes(s.memoryAvailable, lang)} />
        </Panel>
        <Panel title={t('device.storage')} accent={Palette.disk}>
          {s.storage === null || s.storage.length === 0 ? (
            <InfoRow label={t('device.storage')} value={null} />
          ) : (
            s.storage.map((u, i) => (
              <div key={`${u.type}-${i}`} className="stack">
                <InfoRow
                  label={t(u.removable ? 'device.storageRemovable' : 'device.storageInternal')}
                  value={t('device.storageFree', {
                    free: bytes(u.available, lang),
                    total: bytes(u.capacity, lang),
                  })}
                />
                <Bar
                  fraction={
                    u.capacity !== null && u.available !== null && u.capacity > 0
                      ? (u.capacity - u.available) / u.capacity
                      : null
                  }
                  colour={Palette.disk}
                />
              </div>
            ))
          )}
        </Panel>
        <Panel title={t('device.network')} accent={Palette.network}>
          <InfoRow
            label={t('device.connection')}
            value={
              s.networkType === null
                ? null
                : t(`device.netType.${s.networkType}`, { defaultValue: s.networkType })
            }
          />
          <InfoRow label={t('device.ipAddress')} value={s.ipAddress} />
          {s.networkType === 'WIFI' && (
            <>
              <InfoRow label={t('device.ssid')} value={s.ssid} />
              <InfoRow label={t('device.signal')} value={percent(s.wifiSignal)} />
            </>
          )}
        </Panel>
        <Panel title={t('device.display')} accent={Palette.power}>
          <InfoRow
            label={t('device.resolution')}
            value={s.resolution === null ? null : `${s.resolution.width} × ${s.resolution.height}`}
          />
        </Panel>
        <Panel title={t('nav.tv')} accent={Palette.accent}>
          <InfoRow label={t('device.model')} value={s.model} />
          <InfoRow label={t('device.manufacturer')} value={s.manufacturer} />
          <InfoRow label={t('device.platform')} value={s.platformVersion} />
          <InfoRow label={t('device.firmware')} value={s.firmware} />
        </Panel>
      </div>
      {/* A focus stop, so the page can be scrolled to its end with the remote. */}
      <button type="button" className="text-stop" data-autofocus="">
        {t('device.limits')}
      </button>
    </div>
  );
}
