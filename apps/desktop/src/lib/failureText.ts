/**
 * The one way a screen turns a rejected command into something to show.
 *
 * `errorMessage` reads the backend's `{ kind, message }` errors, but Tauri's
 * own rejection for a command that is not in `generate_handler!` is a bare
 * string — `Command <name> not found` — which is not something to put in front
 * of a user. That case means the feature is not built into this platform, so
 * it gets the same sentence every capability gap uses.
 *
 * The translation lives here rather than in `lib/commandError.ts` so that
 * module can stay free of user-facing text, and it is a hook rather than a
 * plain function so a screen does not have to remember to pass the sentence at
 * each of its call sites.
 */

import { useCallback } from 'react';
import { useTranslation } from 'react-i18next';

import { errorMessage } from './commandError';

export function useFailureText(): (error: unknown) => string {
  const { t } = useTranslation();
  return useCallback(
    (error: unknown) => errorMessage(error, { unsupported: t('capability.notSupported') }),
    [t],
  );
}
