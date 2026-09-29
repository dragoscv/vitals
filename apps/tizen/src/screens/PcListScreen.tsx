import { useTranslation } from 'react-i18next';

import { useApp, usePairings } from '../ui/app-context';
import { Action, Hint, ScreenTitle } from '../ui/components';
import { PcCard } from './OverviewScreen';

export function PcListScreen() {
  const { t } = useTranslation();
  const { nav } = useApp();
  const pairings = usePairings();
  return (
    <div className="screen">
      <ScreenTitle>{t('nav.pcs')}</ScreenTitle>
      {pairings.length === 0 ? (
        <>
          <Hint>{t('overview.noPcs')}</Hint>
          <Action label={t('nav.add')} onPress={() => nav.top('add')} autoFocus />
        </>
      ) : (
        <div className="card-grid">
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
