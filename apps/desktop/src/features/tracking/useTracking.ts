/**
 * The Tracking page's data: the cross-repo tracker index, read once per visit.
 *
 * # Read once, not polled
 *
 * The index only changes when someone runs `node tools/tracker-lint.mjs
 * --scan`, which is a deliberate act, not a steady stream. So unlike the AI
 * page's two-second push or the Projects tab's thirty-second poll, this reads
 * when the page becomes visible and then holds still, with a manual Reload for
 * the person who just re-scanned. A timer here would burn reads on a file that
 * cannot have moved.
 *
 * # Subscribed only while visible
 *
 * The same rule as `useAgents` and `useProjects`: a launch that mounts this
 * page in a window hidden in the tray must not read for a screen nobody can
 * see. The last index stays on screen for when the window comes back.
 *
 * # Four absences, four states
 *
 * `noHost` (a dev server with no Tauri backend), `empty` (the host answered but
 * there is no scan yet), `ready`, and `error` (the read itself failed). The
 * first two are normal states the page explains; only `error` is a fault.
 */

import { useCallback, useEffect, useRef, useState, useSyncExternalStore } from 'react';

import { hasTauriHost } from '../../shell/host';
import { parseIndex, parseResumePack, type ResumePack, type TrackerIndex } from './model';

/** Which of the two allowed verbs a claim asks for (`claim_tracker_task`). */
export type ClaimTaskAction = 'claim' | 'complete';

/**
 * The host call behind the panel's one write. `by` is the claiming session
 * (the resume pack's `sessionId`); the command resolves the coordination root
 * itself, so nothing here names a path.
 */
export interface ClaimTaskRequest {
  readonly id: string;
  readonly action: ClaimTaskAction;
  readonly by: string;
  readonly evidence?: string;
}

export type TrackingState =
  | { readonly status: 'loading' }
  /** A browser/dev-server context with no Tauri host to read the file. */
  | { readonly status: 'noHost' }
  /** No scan has run yet (or the index file was empty). */
  | { readonly status: 'empty' }
  | { readonly status: 'ready'; readonly index: TrackerIndex }
  | { readonly status: 'error'; readonly message: string };

/**
 * The one seam the screen depends on. Injectable so a test supplies an index
 * without a Tauri host, exactly as `ProjectsSource` and `HealthApi` are.
 */
export interface TrackingSource {
  /** The index, or `null` when no scan has run. Rejects on a real read failure. */
  readIndex(): Promise<TrackerIndex | null>;
  /** The session's plan and the task store. Rejects on a real read failure. */
  readResume(): Promise<ResumePack | null>;
  /** Whether a Codwen account is stored — the locality indicator. */
  signedIn(): Promise<boolean>;
  /** Claims or completes one task. Rejects with the host's reason. */
  claimTask(request: ClaimTaskRequest): Promise<void>;
}

/** The Tauri-backed source. Dynamic imports keep the IPC module out of a browser. */
export const tauriTrackingSource: TrackingSource = {
  async readIndex() {
    const { invoke } = await import('@tauri-apps/api/core');
    // The command returns the file verbatim (or `null` when absent); the shape
    // is validated here rather than trusted.
    return parseIndex(await invoke<unknown>('get_tracker_index'));
  },
  async readResume() {
    const { invoke } = await import('@tauri-apps/api/core');
    return parseResumePack(await invoke<unknown>('get_resume_pack'));
  },
  async signedIn() {
    const { invoke } = await import('@tauri-apps/api/core');
    const status = await invoke<{ signedIn: boolean }>('get_cloud_status');
    return status.signedIn;
  },
  async claimTask(request) {
    const { invoke } = await import('@tauri-apps/api/core');
    // The command takes one named argument; Tauri reads it as `request`.
    await invoke('claim_tracker_task', { request });
  },
};

export interface TrackingView {
  readonly state: TrackingState;
  /**
   * Whether a Codwen account is stored. `false` before the answer arrives, so
   * the common local-only case is the one drawn first and never flashes an
   * "account connected" label it may have to take back.
   */
  readonly codwen: boolean;
  /** The session's plan and the task store, or `null` until the first read. */
  readonly resume: ResumePack | null;
  /** A failed resume read, so the panel can say so rather than look empty. */
  readonly resumeError: string | null;
  /** Claims or completes a task, then re-reads the pack from the store. */
  readonly claim: (request: ClaimTaskRequest) => Promise<void>;
  /** The id of the task a claim is in flight for, so its button can spin. */
  readonly pendingClaim: string | null;
  /** The reason the last claim was refused, or `null`. */
  readonly claimError: string | null;
  /**
   * Re-reads the index — the Reload button, after a fresh scan.
   *
   * Declared as a property rather than a method so it can be handed straight
   * to a click handler: `unbound-method` refuses a method reference taken off
   * an interface, and `useCallback` returns a plain function anyway.
   */
  readonly reload: () => void;
}

/** Whether `doc` is visible, re-rendering on `visibilitychange`. */
function useVisible(doc: Document): boolean {
  const subscribe = useCallback(
    (onChange: () => void) => {
      doc.addEventListener('visibilitychange', onChange);
      return () => doc.removeEventListener('visibilitychange', onChange);
    },
    [doc],
  );
  return useSyncExternalStore(subscribe, () => doc.visibilityState === 'visible');
}

export function useTracking(
  source: TrackingSource = tauriTrackingSource,
  doc: Document = document,
): TrackingView {
  const visible = useVisible(doc);
  const [state, setState] = useState<TrackingState>({ status: 'loading' });
  const [codwen, setCodwen] = useState(false);
  const [resume, setResume] = useState<ResumePack | null>(null);
  const [resumeError, setResumeError] = useState<string | null>(null);
  const [pendingClaim, setPendingClaim] = useState<string | null>(null);
  const [claimError, setClaimError] = useState<string | null>(null);
  const [nonce, setNonce] = useState(0);
  const srcRef = useRef(source);
  srcRef.current = source;

  useEffect(() => {
    if (!hasTauriHost()) {
      setState({ status: 'noHost' });
      return undefined;
    }
    // Hidden: hold no read, so a tray launch costs nothing.
    if (!visible) return undefined;
    let alive = true;
    const load = async (): Promise<void> => {
      try {
        const index = await srcRef.current.readIndex();
        if (!alive) return;
        setState(
          index === null || index.repos.length === 0
            ? { status: 'empty' }
            : { status: 'ready', index },
        );
      } catch (cause) {
        if (!alive) return;
        setState({
          status: 'error',
          message: cause instanceof Error ? cause.message : String(cause),
        });
      }
    };
    const loadResume = async (): Promise<void> => {
      try {
        const pack = await srcRef.current.readResume();
        if (!alive) return;
        setResume(pack);
        setResumeError(null);
      } catch (cause) {
        // Keep whatever pack is on screen; record why it may be stale.
        if (!alive) return;
        setResumeError(cause instanceof Error ? cause.message : String(cause));
      }
    };
    void load();
    void loadResume();
    // The account fact is a separate, cheaper read; a failure is "local only".
    void srcRef.current
      .signedIn()
      .then((signedIn) => {
        if (alive) setCodwen(signedIn);
      })
      .catch(() => {
        if (alive) setCodwen(false);
      });
    return () => {
      alive = false;
    };
  }, [visible, nonce]);

  const reload = useCallback(() => {
    setState({ status: 'loading' });
    setNonce((n) => n + 1);
  }, []);

  const claim = useCallback(async (request: ClaimTaskRequest): Promise<void> => {
    setPendingClaim(request.id);
    setClaimError(null);
    try {
      await srcRef.current.claimTask(request);
      // Re-read so the button reflects the store, not an assumption about it.
      setResume(await srcRef.current.readResume());
      setResumeError(null);
    } catch (cause) {
      setClaimError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setPendingClaim(null);
    }
  }, []);

  return { state, codwen, resume, resumeError, claim, pendingClaim, claimError, reload };
}
