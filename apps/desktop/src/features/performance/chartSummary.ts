/**
 * The accessible name of a history chart, computed from its data.
 *
 * Shared by the Performance panels and the dashboard widgets so the same
 * chart is described the same way wherever it appears. The formatter is the
 * caller's because only the caller knows the unit — a percentage, bytes per
 * second — and formatting through the same `@vitals/ui` helpers as the
 * readout keeps the spoken figure identical to the visible one.
 */

import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';

import type { ChartSummary } from '@vitals/charts';

export function useChartDescriber(
  metric: string,
  format: (value: number) => string,
): (summary: ChartSummary) => string {
  // The default namespace, not the caller's: the key is shared by every
  // screen that draws a chart.
  const { t } = useTranslation();
  return useCallback(
    (summary: ChartSummary) =>
      t('a11y.chartSummary', {
        metric,
        average: format(summary.average),
        peak: format(summary.peak),
      }),
    [t, metric, format],
  );
}
