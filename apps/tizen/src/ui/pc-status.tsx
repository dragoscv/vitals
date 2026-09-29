import { useTranslation } from 'react-i18next';

import type { LiveState } from '../lib/live';
import { Hint } from './components';

function relative(ms: number, locale: string): string {
  const secs = Math.round((ms - Date.now()) / 1000);
  const fmt = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  if (Math.abs(secs) < 60) return fmt.format(secs, 'second');
  if (Math.abs(secs) < 3600) return fmt.format(Math.round(secs / 60), 'minute');
  return fmt.format(Math.round(secs / 3600), 'hour');
}

/** One line under a PC's name: why it is not showing numbers, or how many warnings it has. */
export function PcStatus({ state }: { state: LiveState }) {
  const { t, i18n } = useTranslation();
  const serious = state.alerts.filter((a) => a.severity !== 'info');
  switch (state.status) {
    case 'unauthorised':
      return <Hint tone="warn">{t('state.unauthorised')}</Hint>;
    case 'incompatible':
      return <Hint tone="warn">{t('state.incompatible')}</Hint>;
    case 'unreachable':
      return (
        <Hint>
          {state.lastSeenMs !== null
            ? t('state.lastSeen', { when: relative(state.lastSeenMs, i18n.language) })
            : `${t('state.unreachable')}. ${t('state.unreachableHint')}`}
        </Hint>
      );
    case 'connecting':
      return <Hint>{t('state.connecting')}</Hint>;
    case 'live':
      if (serious.length === 0) return null;
      return (
        <Hint tone={serious.some((a) => a.severity === 'critical') ? 'danger' : 'warn'}>
          {t('state.alertCount', { count: serious.length })}
        </Hint>
      );
  }
}
