/**
 * What a crashed screen looks like.
 *
 * Two decisions worth stating, because both are the opposite of the usual:
 *
 * **The error message is shown, not hidden.** Consumer apps say "something
 * went wrong" and bury the detail. This one is open source, ships to people
 * who are already debugging a computer, and has a GitHub issue tracker as its
 * only support channel — a message they can copy is the single most useful
 * thing on this screen. Rendered as text, never as markup.
 *
 * **Retry is a button, not automatic.** The same props will throw again, so an
 * automatic retry is a hot loop pinning a core inside a performance monitor.
 * It is worth offering manually because the data is live: the next render
 * genuinely has different props, so retrying after a bad frame often works.
 */

import { RotateCcw, TriangleAlert } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { Button, EmptyState } from '@vitals/ui';

import { SHELL_NS } from './strings';

export function RouteError({
  error,
  onRetry,
}: {
  readonly error: Error;
  readonly onRetry: () => void;
}): React.JSX.Element {
  const { t } = useTranslation(SHELL_NS);

  return (
    <EmptyState
      icon={<TriangleAlert />}
      title={t('routeError.title')}
      description={error.message}
      action={
        <Button onClick={onRetry}>
          <RotateCcw aria-hidden className="size-4" />
          {t('routeError.retry')}
        </Button>
      }
    />
  );
}
