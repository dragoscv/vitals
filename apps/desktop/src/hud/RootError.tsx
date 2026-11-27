/**
 * What a crashed overlay looks like.
 *
 * The overlay sits on a transparent window over whatever the user is doing,
 * so the crash screen paints its own opaque panel — text on a transparent
 * background is invisible over a game, and an invisible error is the blank
 * overlay this boundary exists to prevent. Retry is a button for the same
 * reason as everywhere else: the props are live, so the next render often
 * succeeds, and an automatic retry would be a hot loop.
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
    <div className="flex h-full w-full flex-col items-center justify-center gap-2 rounded-xl bg-[rgb(20_20_24/0.92)] px-4 py-3 text-center text-[var(--color-fg-default)]">
      <span className="flex items-center gap-1.5 text-sm font-semibold">
        <TriangleAlert aria-hidden className="size-4 text-[var(--color-warn-text)]" />
        {t('common.error')}
      </span>
      {/* The message is shown, not hidden: this ships to people already
          debugging a computer and a copyable reason is the useful part. */}
      <span className="line-clamp-2 text-2xs text-[var(--color-fg-muted)]">{error.message}</span>
      <Button size="sm" onClick={onRetry}>
        <RotateCcw aria-hidden className="size-3.5" />
        {t('common.retry')}
      </Button>
    </div>
  );
}
