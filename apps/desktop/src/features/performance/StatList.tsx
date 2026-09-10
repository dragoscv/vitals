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

import { StatList as SharedStatList, type Stat, type StatListProps } from '@vitals/ui';

export type { Stat };

export function StatList(props: Omit<StatListProps, 'omitNull'>): React.JSX.Element | null {
  return <SharedStatList {...props} omitNull />;
}
