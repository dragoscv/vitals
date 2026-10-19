/**
 * Thin wrapper over the shared `@vitals/ui` StatList.
 *
 * Performance panels keep the "absent readings are omitted" behaviour: most
 * of them carry several optional fields — SMART data, Wi-Fi signal, GPU power
 * draw — and a machine reporting none of them would otherwise show a column
 * of dashes that looks like the panel failed to load. Callers that genuinely
 * want to say "we cannot read this" pass the translated `unavailable` string
 * as the value, which is a statement rather than an absence.
 */

import { useTranslation } from 'react-i18next';

import { StatList as SharedStatList, type Stat, type StatListProps } from '@vitals/ui';

import { PERFORMANCE_NS } from './strings';

export type { Stat };

/*
 * A row that has shown a reading keeps its slot when the reading drops out
 * (the shared list's `omitNull` contract), so the dash it shows then needs a
 * name a screen reader can say.
 */
export function StatList(props: Omit<StatListProps, 'omitNull'>): React.JSX.Element | null {
  const { t } = useTranslation(PERFORMANCE_NS);
  return <SharedStatList unavailableLabel={t('unavailable')} {...props} omitNull />;
}
