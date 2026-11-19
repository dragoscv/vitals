/**
 * A feature this build cannot offer, and the reason why.
 *
 * The alternative — a button that is present and fails on every press — is the
 * defining flaw of this category of tool: the user concludes the whole app is
 * broken rather than that one action is unavailable on this system. So an
 * unsupported *action* is either hidden outright (when its absence is the
 * honest answer, as with a Windows-only menu item) or, when the surface around
 * it is still shown, replaced by this: an `EmptyState` that names the obstacle.
 *
 * The reason comes from the capability report, not from the caller's guess, so
 * "needs administrator rights" and "not built yet" cannot be confused with
 * each other or with "your hardware does not report this".
 */

import { useTranslation } from 'react-i18next';
import { Ban } from 'lucide-react';

import { EmptyState } from '@vitals/ui';

import type { Unavailable } from '@vitals/protocol';

import { REASON_KEYS } from '../lib/capabilityText';

export interface UnsupportedNoticeProps {
  /** The feature's own name, already translated by the caller. */
  readonly title: string;
  /** Why it cannot run here, straight from the capability report. */
  readonly reason: Unavailable;
  /** An extra line — usually a next step the user can take. */
  readonly detail?: string;
}

export function UnsupportedNotice({
  title,
  reason,
  detail,
}: UnsupportedNoticeProps): React.JSX.Element {
  const { t } = useTranslation();
  return (
    <EmptyState
      icon={<Ban />}
      title={title}
      description={[t(REASON_KEYS[reason]), detail].filter(Boolean).join(' ')}
    />
  );
}
