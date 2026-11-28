/**
 * Contains a render error inside a widget instead of leaving an empty,
 * always-on-top window on the user's screen with no way to dismiss it.
 *
 * # Why a copy of the HUD's boundary
 *
 * Identical shape and identical reasoning (see `hud/ErrorBoundary.tsx`): the
 * shell's boundary drags the router, the charts and the store into a window
 * that draws six numbers, and this tree is fenced away from the shell for that
 * reason. It may still reach `lib/logToFile`, the one module outside
 * `src/widgets` the fence permits, because a window with no devtools needs its
 * errors written somewhere.
 *
 * The fallback is a class-level boundary for the same reason: React has no
 * hook equivalent. It does not retry automatically — a render that threw once
 * with the same props throws again, and the loop would pin a core on a
 * performance monitor.
 */

import { Component, type ErrorInfo, type ReactNode } from 'react';

import { reportToLog } from '../lib/logToFile';

export interface ErrorBoundaryProps {
  readonly children: ReactNode;
  /** Changing this remounts the boundary and clears the error (config change). */
  readonly resetKey?: string;
  /** Rendered in place of the children. Receives a retry callback. */
  readonly fallback: (error: Error, retry: () => void) => ReactNode;
  /** Reporting hook. Kept injectable so a test can assert it is called. */
  readonly onError?: (error: Error, info: ErrorInfo) => void;
}

interface ErrorBoundaryState {
  readonly error: Error | null;
  readonly key: string | undefined;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { error: null, key: undefined };

  static getDerivedStateFromError(error: unknown): Partial<ErrorBoundaryState> {
    // Anything can be thrown — a string, `undefined`, an object whose
    // `toString` itself throws — so it is normalised here and the fallback can
    // rely on `.message` existing.
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  static getDerivedStateFromProps(
    props: ErrorBoundaryProps,
    state: ErrorBoundaryState,
  ): Partial<ErrorBoundaryState> | null {
    // Reset during render, not in an effect: an effect runs after paint, so
    // the user would see one frame of the previous config's crash screen.
    if (state.error !== null && state.key !== props.resetKey) {
      return { error: null, key: props.resetKey };
    }
    if (state.error === null && state.key !== props.resetKey) {
      return { key: props.resetKey };
    }
    return null;
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('widget render error contained by boundary', error, info.componentStack);
    reportToLog('error', error, 'widgets:boundary', info.componentStack ?? undefined);
    this.props.onError?.(error, info);
  }

  private readonly retry = (): void => {
    this.setState({ error: null });
  };

  override render(): ReactNode {
    const { error } = this.state;
    if (error !== null) return this.props.fallback(error, this.retry);
    return this.props.children;
  }
}
