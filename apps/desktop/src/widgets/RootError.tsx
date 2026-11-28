/**
 * What a crashed widget looks like.
 *
 * The crash screen paints its own translucent panel for the same reason the
 * widget does: text on a transparent background is invisible over a game, and
 * an invisible error is exactly the blank window this boundary exists to
 * prevent. The fill is the overlay token rather than a hard-coded colour: the
 * panel floats over an arbitrary desktop, so its contrast is a property of the
 * stylesheet (see the measurement in `styles.css`), not of this component. The
 * message is shown rather than hidden — this ships to people already debugging
 * a computer, and a copyable reason is the useful part.
 */

import { RotateCcw, TriangleAlert } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button } from '@vitals/ui';

export function RootError({
  error,
  onRetry,
}: {
  readonly error: Error;
  readonly onRetry: () => void;
}): React.JSX.Element {
  const { t } = useTranslation();

  return (
    <div className="flex h-full w-full flex-col items-center justify-center gap-2 rounded-xl bg-[var(--color-bg-overlay)] px-4 py-3 text-center text-[var(--color-fg-default)]">
      <span className="flex items-center gap-1.5 text-sm font-semibold">
        <TriangleAlert aria-hidden className="size-4 text-[var(--color-warn-text)]" />
        {t('common.error')}
      </span>
      <span className="line-clamp-3 text-2xs text-[var(--color-fg-muted)]">{error.message}</span>
      <Button size="sm" onClick={onRetry}>
        <RotateCcw aria-hidden className="size-3.5" />
        {t('common.retry')}
      </Button>
    </div>
  );
}
