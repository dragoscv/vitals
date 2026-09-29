import type { Health, VitalsClient } from '@vitals/client';
import type { HostInfo } from '@vitals/protocol';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';

import { bytes } from '../../lib/format';
import { Hint, InfoRow, Panel } from '../../ui/components';

/**
 * Static facts about the PC: `GET /api/v1/host` and the version from `/health`.
 *
 * A 204 ("not probed yet") is a different fact from an unreachable PC, and
 * saying "Can't reach this PC" for it was wrong: the first run on the TV,
 * against `serve_dev`, which never probes the host, showed exactly that
 * while every other tab was live.
 */
export function AboutTab({ client }: { client: VitalsClient }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const [host, setHost] = useState<HostInfo | 'loading' | 'none' | 'failed'>('loading');
  const [health, setHealth] = useState<Health | null>(null);

  useEffect(() => {
    let alive = true;
    client.host().then(
      (h) => alive && setHost(h ?? 'none'),
      () => alive && setHost('failed'),
    );
    client.health().then(
      (h) => alive && setHealth(h),
      () => undefined,
    );
    return () => {
      alive = false;
    };
  }, [client]);

  if (host === 'loading') return <Hint>{t('device.loading')}</Hint>;
  if (host === 'failed') return <Hint>{t('state.unreachable')}</Hint>;
  if (host === 'none') {
    return (
      <Panel title={t('tab.about')}>
        <button type="button" className="panel-stop stack">
          <Hint>{t('host.notProbed')}</Hint>
          <InfoRow label={t('host.version')} value={health?.version ?? null} />
        </button>
      </Panel>
    );
  }
  const started = new Intl.DateTimeFormat(lang, { dateStyle: 'medium', timeStyle: 'short' }).format(
    host.bootTimeMs,
  );
  return (
    <Panel title={host.hostname}>
      <button type="button" className="panel-stop stack" aria-label={host.hostname}>
        <InfoRow label={t('host.name')} value={host.hostname} />
        <InfoRow label={t('host.os')} value={`${host.osName} ${host.osVersion}`} />
        <InfoRow label={t('host.cpu')} value={host.cpuModel} />
        <InfoRow
          label=""
          value={t('host.cores', { physical: host.physicalCores, logical: host.logicalCores })}
        />
        <InfoRow label={t('host.memory')} value={bytes(host.totalMemory, lang)} />
        <InfoRow label={t('host.board')} value={host.motherboard} />
        <InfoRow label={t('host.bios')} value={host.biosVersion} />
        <InfoRow label={t('host.vm')} value={t(host.isVirtualMachine ? 'host.yes' : 'host.no')} />
        <InfoRow label={t('host.started')} value={started} />
        <InfoRow label={t('host.version')} value={health?.version ?? null} />
      </button>
    </Panel>
  );
}
