/**
 * Shows a failed fire-and-forget action to the user.
 *
 * Buttons like Quit, Open Task Manager, Clear history and Export recording
 * call a Tauri command and `void` the promise. When such a command rejected
 * — helper missing, dialog plugin absent, permission refused — the rejection
 * went to the console and the button simply appeared not to work, which is
 * the one outcome worse than an error. Every such call site routes here.
 *
 * sonner is loaded on first use, for the same reason as `useSamplerToasts`:
 * a toast is a rare event and does not belong in the entry chunk.
 */

import type { toast as Toast } from '@vitals/ui/toast';

import { errorMessage } from './commandError';

let toastModule: Promise<{ toast: typeof Toast }> | null = null;

/** Attaches a failure toast to `action`; resolves either way, so callers can `void` it. */
export function reportFailure(action: Promise<unknown>, title: string): Promise<void> {
  return action.then(
    () => undefined,
    (error: unknown) => {
      const description = errorMessage(error);
      toastModule ??= import('@vitals/ui/toast');
      return toastModule.then(({ toast }) => {
        // Stable id: a button clicked twice shows one toast, updated, not two.
        toast.error(title, { description, id: `action:${title}` });
      });
    },
  );
}
