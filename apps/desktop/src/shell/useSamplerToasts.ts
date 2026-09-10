/**
 * Turns sampler failures into toasts.
 *
 * The dashboard already keeps its last good readings when the sampler errors
 * (see `useSystemSnapshot`), which is right for the data — but silently. A
 * user whose GPU counters just stopped answering deserves one sentence saying
 * so, once, rather than a chart that quietly flatlines.
 *
 * Deduplicated on the message: the sampler retries every tick, and Rust
 * already caps reporting at three identical errors, but three identical
 * toasts is still two too many.
 */

import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';

import type { toast as Toast } from '@vitals/ui/toast';

import { hasTauriHost } from './host';
import { SHELL_NS } from './strings';

/** Matches `ERROR_EVENT` in `apps/desktop/src-tauri/src/sampling.rs`. */
export const SAMPLER_ERROR_EVENT = 'vitals://sampler-error';

export function useSamplerToasts(): void {
  const { t } = useTranslation(SHELL_NS);

  useEffect(() => {
    if (!hasTauriHost()) return;
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    const seen = new Set<string>();
    // Loaded on the first error, not at startup: sonner stays out of the
    // entry chunk for the sessions in which nothing ever fails. Resolved
    // once and reused; every error after the first is a plain call.
    let toastModule: Promise<{ toast: typeof Toast }> | null = null;
    const toastError = (title: string, options: { description: string; id: string }): void => {
      toastModule ??= import('@vitals/ui/toast');
      void toastModule.then(({ toast }) => {
        toast.error(title, options);
      });
    };

    void import('@tauri-apps/api/event').then(async ({ listen }) => {
      const stop = await listen<string>(SAMPLER_ERROR_EVENT, (event) => {
        const detail = event.payload;
        if (seen.has(detail)) return;
        seen.add(detail);
        toastError(t('sampler.errorTitle'), {
          description: t('sampler.errorBody', { detail }),
          // Stable id so a repeat of the same message updates in place
          // rather than stacking, even if it slips past the set above.
          id: `sampler:${detail}`,
        });
      });
      if (cancelled) stop();
      else unlisten = stop;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [t]);
}
