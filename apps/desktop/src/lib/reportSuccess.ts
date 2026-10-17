/**
 * Confirms a finished mutation to the user — priority changed, Recycle Bin
 * emptied, space freed.
 *
 * The sibling of `reportFailure`. Without it these actions succeeded in
 * silence: the menu closed and nothing on screen changed, which reads exactly
 * like a click that did not register, so people clicked again.
 *
 * sonner is loaded on first use for the same reason as there: a toast is a
 * rare event and does not belong in the entry chunk.
 */

import type { toast as Toast } from '@vitals/ui/toast';

let toastModule: Promise<{ toast: typeof Toast }> | null = null;

/** Shows a success toast. Fire-and-forget; never rejects. */
export function reportSuccess(title: string, description?: string): Promise<void> {
  toastModule ??= import('@vitals/ui/toast');
  return toastModule.then(
    ({ toast }) => {
      // Stable id: the same action repeated updates one toast, not a stack.
      toast.success(title, {
        id: `done:${title}`,
        ...(description !== undefined && { description }),
      });
    },
    () => undefined,
  );
}
