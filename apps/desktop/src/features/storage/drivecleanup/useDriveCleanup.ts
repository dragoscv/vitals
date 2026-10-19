/**
 * Runs one drive's clean-up wizard: analyse, choose, confirm, run, report.
 *
 * # Every loading state must be able to end
 *
 * The analysis resolves to a plan or rejects to a message; Cancel asks the
 * backend to stop and the call then settles through the same path. A run
 * that rejects ends in `failed` and says whether it was a refusal (nothing
 * changed) or a fault.
 *
 * # Only ids cross the boundary
 *
 * `run_drive_cleanup` takes the plan's `scanId`, the ids the user ticked and
 * `confirmed: true`. The webview never names a path.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { errorMessage, isCommandError } from '../../../lib/commandError';
import type {
  DriveAnalyseProgress,
  DriveCleanProgress,
  DriveCleanupPlan,
  DriveCleanupReport,
} from './model';

/** Emitted by `analyse_drive_cleanup` while it measures. */
export const DRIVE_ANALYSE_PROGRESS_EVENT = 'vitals://storage/drive-analyse-progress';

/** Emitted by `run_drive_cleanup` as it moves through its stages. */
export const DRIVE_CLEAN_PROGRESS_EVENT = 'vitals://storage/drive-clean-progress';

export interface DriveCleanupSource {
  readonly analyse: (mount: string) => Promise<DriveCleanupPlan>;
  readonly cancelAnalysis: () => Promise<void>;
  readonly run: (
    scanId: number,
    ids: readonly string[],
    confirmed: boolean,
  ) => Promise<DriveCleanupReport>;
  readonly onAnalyseProgress: (
    listener: (progress: DriveAnalyseProgress) => void,
  ) => Promise<() => void>;
  readonly onCleanProgress: (
    listener: (progress: DriveCleanProgress) => void,
  ) => Promise<() => void>;
}

/** The real source. Dynamic imports so a browser never evaluates the IPC module. */
export const tauriDriveSource: DriveCleanupSource = {
  analyse: async (mount) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<DriveCleanupPlan>('analyse_drive_cleanup', { mount });
  },
  cancelAnalysis: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<void>('cancel_drive_cleanup_analysis');
  },
  run: async (scanId, ids, confirmed) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<DriveCleanupReport>('run_drive_cleanup', { scanId, ids, confirmed });
  },
  onAnalyseProgress: async (listener) => {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<DriveAnalyseProgress>(DRIVE_ANALYSE_PROGRESS_EVENT, (event) => {
      listener(event.payload);
    });
  },
  onCleanProgress: async (listener) => {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<DriveCleanProgress>(DRIVE_CLEAN_PROGRESS_EVENT, (event) => {
      listener(event.payload);
    });
  },
};

export type DriveCleanupPhase =
  | { readonly step: 'analysing'; readonly progress: DriveAnalyseProgress | null }
  | { readonly step: 'analyseFailed'; readonly message: string }
  | { readonly step: 'plan'; readonly plan: DriveCleanupPlan }
  | {
      readonly step: 'running';
      readonly plan: DriveCleanupPlan;
      readonly ids: readonly string[];
      readonly progress: DriveCleanProgress | null;
    }
  | {
      readonly step: 'report';
      readonly plan: DriveCleanupPlan;
      readonly ids: readonly string[];
      readonly report: DriveCleanupReport;
    }
  | {
      readonly step: 'runFailed';
      readonly plan: DriveCleanupPlan;
      readonly ids: readonly string[];
      readonly message: string;
      /** The backend or the UAC prompt refused: nothing was changed. */
      readonly refused: boolean;
    };

export interface DriveCleanupState {
  readonly phase: DriveCleanupPhase;
  readonly cancelAnalysis: () => void;
  /** Runs the clean-up of the given ids; resolves when its report is in. */
  readonly run: (ids: readonly string[]) => Promise<void>;
}

/** Analyses `mount` as soon as it mounts; the dialog owns one instance per opening. */
export function useDriveCleanup(mount: string, source?: DriveCleanupSource): DriveCleanupState {
  const sourceRef = useRef<DriveCleanupSource>(source ?? tauriDriveSource);
  sourceRef.current = source ?? tauriDriveSource;
  const [phase, setPhase] = useState<DriveCleanupPhase>({ step: 'analysing', progress: null });
  const mounted = useRef(true);
  const runInFlight = useRef(false);

  useEffect(() => {
    mounted.current = true;
    // Subscribed before the analysis starts, so the first report is not missed.
    const unsubscribe = sourceRef.current
      .onAnalyseProgress((progress) => {
        if (!mounted.current) return;
        setPhase((current) =>
          current.step === 'analysing' ? { step: 'analysing', progress } : current,
        );
      })
      .catch(() => () => undefined);

    sourceRef.current
      .analyse(mount)
      .then((plan) => {
        if (mounted.current) setPhase({ step: 'plan', plan });
      })
      .catch((cause: unknown) => {
        if (mounted.current) setPhase({ step: 'analyseFailed', message: errorMessage(cause) });
      })
      .finally(() => {
        void unsubscribe.then((stop) => {
          stop();
        });
      });

    return () => {
      mounted.current = false;
    };
  }, [mount]);

  const cancelAnalysis = useCallback(() => {
    // The analysis settles through the normal path once the backend stops.
    void sourceRef.current.cancelAnalysis().catch((cause: unknown) => {
      if (mounted.current) setPhase({ step: 'analyseFailed', message: errorMessage(cause) });
    });
  }, []);

  const run = useCallback(
    async (ids: readonly string[]) => {
      if (runInFlight.current || phase.step !== 'plan' || ids.length === 0) return;
      const plan = phase.plan;
      runInFlight.current = true;
      setPhase({ step: 'running', plan, ids, progress: null });

      const unsubscribe = sourceRef.current
        .onCleanProgress((progress) => {
          if (!mounted.current) return;
          setPhase((current) => (current.step === 'running' ? { ...current, progress } : current));
        })
        .catch(() => () => undefined);

      try {
        const report = await sourceRef.current.run(plan.scanId, ids, true);
        if (mounted.current) setPhase({ step: 'report', plan, ids, report });
      } catch (cause: unknown) {
        if (mounted.current) {
          setPhase({
            step: 'runFailed',
            plan,
            ids,
            message: errorMessage(cause),
            refused: isCommandError(cause) && cause.kind === 'refused',
          });
        }
      } finally {
        runInFlight.current = false;
        void unsubscribe.then((stop) => {
          stop();
        });
      }
    },
    [phase],
  );

  return { phase, cancelAnalysis, run };
}
