/**
 * One PC, in five tabs. The tabs are the first row of focus stops, so Up from
 * anywhere near the top reaches them and Left/Right switches between them.
 */

import { useState } from 'react';
import { useTranslation } from 'react-i18next';

import { useApp, useLive, usePairings } from '../../ui/app-context';
import { Action, Choice, Hint, ScreenTitle } from '../../ui/components';
import { PcStatus } from '../../ui/pc-status';
import { AboutTab } from './AboutTab';
import { HistoryTab } from './HistoryTab';
import { NowTab } from './NowTab';
import { ProgramsTab } from './ProgramsTab';
import { SensorsTab } from './SensorsTab';

const TABS = ['now', 'programs', 'sensors', 'history', 'about'] as const;
type Tab = (typeof TABS)[number];

export function PcScreen({ id }: { id: string }) {
  const { t } = useTranslation();
  const { nav } = useApp();
  const pairing = usePairings().find((p) => p.id === id);
  const { state, connection } = useLive(pairing);
  const [tab, setTab] = useState<Tab>('now');

  if (pairing === undefined) {
    return (
      <div className="screen">
        <Action label={t('nav.back')} onPress={() => nav.pop()} autoFocus />
      </div>
    );
  }

  const offline = state.status !== 'live';
  return (
    <div className="screen">
      <div className="title-row">
        <ScreenTitle>{pairing.label}</ScreenTitle>
        <PcStatus state={state} />
        {(state.status === 'unreachable' || state.status === 'unauthorised') && (
          <Action label={t('state.connecting')} onPress={() => connection?.restart()} />
        )}
      </div>
      <div className="tabs" role="tablist">
        {TABS.map((name) => (
          <Choice
            key={name}
            label={t(`tab.${name}`)}
            selected={tab === name}
            onPress={() => setTab(name)}
            autoFocus={tab === name}
          />
        ))}
      </div>
      {offline && state.status === 'unreachable' && <Hint>{t('wake.notPossible')}</Hint>}
      <div className="tab-body" role="tabpanel">
        {tab === 'now' && <NowTab state={state} />}
        {tab === 'programs' && connection !== null && (
          <ProgramsTab pairing={pairing} state={state} client={connection.client} />
        )}
        {tab === 'sensors' && <SensorsTab pairing={pairing} />}
        {tab === 'history' && <HistoryTab pairing={pairing} />}
        {tab === 'about' && connection !== null && <AboutTab client={connection.client} />}
      </div>
    </div>
  );
}
