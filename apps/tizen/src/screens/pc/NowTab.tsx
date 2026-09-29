import { memoryPercent } from '@vitals/protocol';
import { useTranslation } from 'react-i18next';

import { bytes, celsius, duration, ghz, percent, rate, rpm, watts } from '../../lib/format';
import type { LiveState } from '../../lib/live';
import { Bar, Hint, InfoRow, Panel, Ring, Spark } from '../../ui/components';
import { Palette, Thresholds } from '../../ui/theme';

/**
 * Everything the PC measured this second. Each panel is a focus stop so the
 * page scrolls under the remote; a panel for hardware the PC does not have
 * (no battery, no fan headers) is left out rather than shown full of dashes.
 */
export function NowTab({ state }: { state: LiveState }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const system = state.system;
  if (system === null)
    return (
      <Hint>{t(state.status === 'unreachable' ? 'state.unreachable' : 'processes.waiting')}</Hint>
    );
  const { cpu, memory } = system;
  const memPct = memoryPercent(system);

  return (
    <div className="grid-2">
      <Panel title={t('metric.cpu')} accent={Palette.cpu} className="focus-panel">
        <button type="button" className="panel-stop" aria-label={t('metric.cpu')}>
          <span className="panel-row">
            <Ring
              fraction={cpu.total / 100}
              colour={Palette.cpu}
              value={percent(cpu.total)}
              label={t('metric.cpu')}
              level={Thresholds.cpu(cpu.total)}
            />
            <Spark values={state.cpu} colour={Palette.cpu} label={t('metric.cpu')} height="large" />
          </span>
        </button>
        <InfoRow label={t('cpu.clock')} value={ghz(cpu.effectiveClock, lang)} />
        <InfoRow
          label={t('cpu.temperature')}
          value={celsius(cpu.temperature)}
          tone={Thresholds.cpuTemp(cpu.temperature)}
        />
        <InfoRow label={t('cpu.power')} value={watts(cpu.power, lang)} />
        <InfoRow
          label={t('cpu.state')}
          value={
            cpu.throttled === null
              ? t('cpu.notThrottled')
              : t('cpu.throttled', { reason: t(`throttle.${cpu.throttled}`) })
          }
          tone={cpu.throttled === null ? 'ok' : 'warn'}
        />
        <InfoRow label={t('cpu.processes')} value={String(cpu.processCount)} />
        <InfoRow label={t('cpu.uptime')} value={duration(cpu.uptimeSecs)} />
        <h4>{t('cpu.cores')}</h4>
        <div className="cores">
          {cpu.perCore.map((c, i) => (
            <div key={i} className="core" title={`${i}: ${percent(c)}`}>
              <div className="core-fill" style={{ height: `${Math.min(100, Math.max(0, c))}%` }} />
            </div>
          ))}
        </div>
      </Panel>

      <Panel title={t('metric.memory')} accent={Palette.memory}>
        <button type="button" className="panel-stop" aria-label={t('metric.memory')}>
          <span className="panel-row">
            <Ring
              fraction={memPct === null ? null : memPct / 100}
              colour={Palette.memory}
              value={percent(memPct)}
              label={t('metric.memoryShort')}
              level={Thresholds.memory(memPct)}
            />
            <Spark
              values={state.memory}
              colour={Palette.memory}
              label={t('metric.memory')}
              height="large"
            />
          </span>
        </button>
        <InfoRow
          label={t('metric.memory')}
          value={t('memory.usedOf', {
            used: bytes(memory.used, lang),
            total: bytes(memory.total, lang),
          })}
        />
        <InfoRow label={t('memory.available')} value={bytes(memory.available, lang)} />
        <InfoRow label={t('memory.cached')} value={bytes(memory.cached, lang)} />
        <InfoRow
          label={t('memory.committed')}
          value={t('memory.usedOf', {
            used: bytes(memory.committed, lang),
            total: bytes(memory.commitLimit, lang),
          })}
        />
      </Panel>

      {system.gpus.map((g) => (
        <Panel key={g.id} title={g.name} accent={Palette.gpu}>
          <button type="button" className="panel-stop" aria-label={g.name}>
            <InfoRow label={t('metric.gpu')} value={percent(g.utilization)} />
          </button>
          <Bar
            fraction={g.utilization === null ? null : g.utilization / 100}
            colour={Palette.gpu}
          />
          <InfoRow
            label={t('gpu.memory')}
            value={
              g.memoryUsed === null
                ? null
                : t('memory.usedOf', {
                    used: bytes(g.memoryUsed, lang),
                    total: bytes(g.memoryTotal, lang),
                  })
            }
          />
          <InfoRow
            label={t('gpu.temperature')}
            value={celsius(g.temperature)}
            tone={Thresholds.gpuTemp(g.temperature)}
          />
          <InfoRow label={t('gpu.power')} value={watts(g.power, lang)} />
          <InfoRow
            label={t('gpu.fan')}
            value={g.fanRpm !== null ? rpm(g.fanRpm) : percent(g.fanPercent)}
          />
        </Panel>
      ))}

      <Panel title={t('metric.disk')} accent={Palette.disk}>
        {system.disks.map((d) => (
          <button type="button" key={d.id} className="panel-stop stack" aria-label={d.name}>
            <InfoRow
              label={d.mount ?? d.name}
              value={t('disk.free', { free: bytes(d.free, lang), total: bytes(d.total, lang) })}
            />
            <Bar
              fraction={d.total > 0 ? (d.total - d.free) / d.total : null}
              colour={Palette.disk}
            />
            <InfoRow label={t('disk.activity')} value={percent(d.activeTime)} />
            <InfoRow
              label=""
              value={t('disk.readWrite', { read: rate(d.read, lang), write: rate(d.write, lang) })}
            />
            {d.health !== null && d.health.lifeRemaining !== null && (
              <InfoRow label={t('disk.life')} value={percent(d.health.lifeRemaining)} />
            )}
            {d.health?.failing === true && <Hint tone="danger">{t('disk.failing')}</Hint>}
          </button>
        ))}
      </Panel>

      <Panel title={t('metric.network')} accent={Palette.network}>
        {system.networks
          .filter((n) => n.kind !== 'loopback')
          .map((n) => (
            <button type="button" key={n.id} className="panel-stop stack" aria-label={n.name}>
              <InfoRow
                label={n.name}
                value={
                  n.connected
                    ? t('net.downUp', { down: rate(n.rx, lang), up: rate(n.tx, lang) })
                    : t('net.disconnected')
                }
              />
              {n.ipv4 !== null && <InfoRow label="IPv4" value={n.ipv4} />}
            </button>
          ))}
      </Panel>

      {system.fans.length > 0 && (
        <Panel title={t('metric.fans')} accent={Palette.thermal}>
          <button type="button" className="panel-stop" aria-label={t('metric.fans')}>
            {system.fans.map((f) => (
              <InfoRow key={f.name} label={f.name} value={rpm(f.rpm)} />
            ))}
          </button>
        </Panel>
      )}

      {system.battery !== null && (
        <Panel title={t('metric.battery')} accent={Palette.power}>
          <button type="button" className="panel-stop" aria-label={t('metric.battery')}>
            <InfoRow
              label={t(system.battery.charging ? 'battery.charging' : 'battery.discharging')}
              value={percent(system.battery.charge)}
            />
            <InfoRow
              label={t('battery.timeLeft')}
              value={duration(system.battery.timeRemainingSecs)}
            />
            <InfoRow label={t('battery.health')} value={percent(system.battery.health)} />
          </button>
        </Panel>
      )}

      {system.powerDraw !== null && (
        <Panel title={t('metric.power')} accent={Palette.power}>
          <button type="button" className="panel-stop" aria-label={t('metric.power')}>
            <InfoRow label={t('metric.power')} value={watts(system.powerDraw, lang)} />
          </button>
        </Panel>
      )}
    </div>
  );
}
