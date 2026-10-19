/**
 * Runs one guided disk move: plan, choose, confirm, move, report.
 *
 * Every loading state can end: the plan resolves or rejects, the move
 * resolves to a report or rejects to `failed` with whether it was a refusal
 * (nothing stopped) or a fault. A move cannot be cancelled once it starts —
 * WSL copies the disk in one call — so there is no cancel here at all; the
 * dialog's Cancel exists only before it.
 */

import { useCallback, useEffect, useRef, useState } from 'react';

import { errorMessage, isCommandError } from '../../../../lib/commandError';
import type { DockerDisk, MovePlan, MoveProgress, MoveReport } from './model';

/** Emitted by `run_drive_move` as it moves through its stages. */
export const DRIVE_MOVE_PROGRESS_EVENT = 'vitals://storage/drive-move-progress';

export interface DriveMoveSource {
  readonly plan: (mount: string) => Promise<MovePlan>;
  readonly run: (
    planId: number,
    diskId: string,
    target: string,
    confirmed: boolean,
  ) => Promise<MoveReport>;
  readonly openDocker: () => Promise<void>;
  readonly checkDocker: () => Promise<DockerDisk>;
  readonly onProgress: (listener: (progress: MoveProgress) => void) => Promise<() => void>;
}

/** The real source. Dynamic imports so a browser never evaluates the IPC module. */
export const tauriMoveSource: DriveMoveSource = {
  plan: async (mount) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<MovePlan>('plan_drive_move', { mount });
  },
  run: async (planId, diskId, target, confirmed) => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<MoveReport>('run_drive_move', { planId, diskId, target, confirmed });
  },
  openDocker: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<void>('open_docker_desktop');
  },
  checkDocker: async () => {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<DockerDisk>('check_docker_disk');
  },
  onProgress: async (listener) => {
    const { listen } = await import('@tauri-apps/api/event');
    return listen<MoveProgress>(DRIVE_MOVE_PROGRESS_EVENT, (event) => {
      listener(event.payload);
    });
  },
};

export type DriveMovePhase =
  | { readonly step: 'loading' }
  | { readonly step: 'loadFailed'; readonly message: string }
  | { readonly step: 'plan'; readonly plan: MovePlan }
  | {
      readonly step: 'moving';
      readonly plan: MovePlan;
      readonly diskId: string;
      readonly target: string;
      readonly progress: MoveProgress | null;
    }
  | {
      readonly step: 'report';
      readonly plan: MovePlan;
      readonly diskId: string;
      readonly target: string;
      readonly report: MoveReport;
    }
  | {
      readonly step: 'failed';
      readonly plan: MovePlan;
      readonly diskId: string;
      readonly target: string;
      readonly message: string;
      /** The backend refused before anything stopped. */
      readonly refused: boolean;
    };

export interface DriveMoveState {
  readonly phase: DriveMovePhase;
  readonly move: (diskId: string, target: string) => Promise<void>;
}

export function useDriveMove(mount: string, source: DriveMoveSource): DriveMoveState {
  const sourceRef = useRef(source);
  sourceRef.current = source;
  const [phase, setPhase] = useState<DriveMovePhase>({ step: 'loading' });
  const mounted = useRef(true);
  const inFlight = useRef(false);

  useEffect(() => {
    mounted.current = true;
    sourceRef.current
      .plan(mount)
      .then((plan) => {
        if (mounted.current) setPhase({ step: 'plan', plan });
      })
      .catch((cause: unknown) => {
        if (mounted.current) setPhase({ step: 'loadFailed', message: errorMessage(cause) });
      });
    return () => {
      mounted.current = false;
    };
  }, [mount]);

  const move = useCallback(
    async (diskId: string, target: string) => {
      if (inFlight.current || phase.step !== 'plan') return;
      const plan = phase.plan;
      inFlight.current = true;
      setPhase({ step: 'moving', plan, diskId, target, progress: null });
      const unsubscribe = sourceRef.current
        .onProgress((progress) => {
          if (!mounted.current) return;
          setPhase((current) => (current.step === 'moving' ? { ...current, progress } : current));
        })
        .catch(() => () => undefined);
      try {
        const report = await sourceRef.current.run(plan.planId, diskId, target, true);
        if (mounted.current) setPhase({ step: 'report', plan, diskId, target, report });
      } catch (cause: unknown) {
        if (mounted.current) {
          setPhase({
            step: 'failed',
            plan,
            diskId,
            target,
            message: errorMessage(cause),
            refused: isCommandError(cause) && cause.kind === 'refused',
          });
        }
      } finally {
        inFlight.current = false;
        void unsubscribe.then((stop) => {
          stop();
        });
      }
    },
    [phase],
  );

  return { phase, move };
}
