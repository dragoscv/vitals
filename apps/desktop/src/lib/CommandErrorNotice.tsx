/**
 * The sentence a rejected command should show, with the backend's raw text
 * kept reachable behind a disclosure.
 *
 * A `CommandError` carries a stable `kind` as well as its `message`. Showing
 * the message alone puts a Rust-authored line in front of the user and hides
 * the one part that is actionable — `access-denied` is the case a person can
 * fix, by running Vitals as an administrator, and no wording of the message
 * can promise that across locales. So the kind becomes an EN/RO sentence and
 * the raw text stays reachable for a bug report.
 *
 * There is no sentence to show when the rejection carries no kind, and no point
 * in a disclosure that repeats the sentence — in both cases the message is
 * shown on its own. A disclosure that opens onto nothing would be worse than
 * none, so it is never rendered empty.
 *
 * The output is inline — a `<span>` for the sentence and a `<details>` for the
 * disclosure — so a caller can drop it inside an existing `<p role="alert">`
 * without nesting a `<p>` in a `<p>`.
 */

import { useTranslation } from 'react-i18next';

import { errorKind, errorMessage, type CommandErrorShape } from './commandError';

/**
 * The locale key for each kind. `satisfies` makes a new variant of
 * `CommandErrorShape['kind']` a compile error here rather than a key path that
 * renders untranslated on someone's screen.
 */
const KIND_KEYS = {
  'access-denied': 'error.kind.accessDenied',
  'not-found': 'error.kind.notFound',
  unsupported: 'error.kind.unsupported',
  refused: 'error.kind.refused',
  internal: 'error.kind.internal',
} as const satisfies Record<CommandErrorShape['kind'], string>;

export interface CommandErrorNoticeProps {
  /** Anything a command or promise rejected with, exactly as it arrived. */
  readonly error: unknown;
}

export function CommandErrorNotice({ error }: CommandErrorNoticeProps): React.JSX.Element {
  const { t } = useTranslation();
  const kind = errorKind(error);
  const message = errorMessage(error);
  const sentence = kind === null ? null : t(KIND_KEYS[kind]);
  // No kind, or a handler that already wrote the sentence itself: the message
  // is the whole notice, and there is nothing a disclosure could add.
  if (sentence === null || sentence === message) {
    return <span>{message}</span>;
  }
  return (
    <>
      <span>{sentence}</span>{' '}
      <details className="inline-block align-top">
        <summary className="cursor-pointer">{t('error.details')}</summary>
        <span className="font-mono break-all">{message}</span>
      </details>
    </>
  );
}
