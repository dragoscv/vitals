/**
 * Contains a render error inside the overlay instead of leaving an empty
 * window on top of the user's screen.
 *
 * # Why a copy of the shell's boundary
 *
 * `shell/ErrorBoundary` is the same idea, but the overlay must not import it:
 * `hud/boundary.test.ts` fences `src/hud/**` off from the shell, because one
 * shell import drags the whole desktop bundle — router, charts, virtualised
 * tables, the store — into a 220×96 window that shows three numbers, and the
 * cost lands on every launch since the overlay is created at startup. The prop
 * shape is kept identical so the two read side by side; unlike the phone copy,
 * this one may still reach `lib/logToFile`, which is the single module outside
 * `src/hud` the fence permits and which imports nothing but
 * `@tauri-apps/api/core`.
 *
 * # Why a class, and why no automatic retry
 *
 * React offers no hook equivalent (`getDerivedStateFromError` and
 * `componentDidCatch` are class-only). It does not retry on its own: a render
 * that threw once with the same props throws again, and a loop pins a core —
 * on a performance monitor, of all things. The button is useful precisely
 * because the overlay's data is live, so the next render has different props.
 */

import { Component, type ErrorInfo, type ReactNode } from 'react';

import { reportToLog } from '../lib/logToFile';

export interface ErrorBoundaryProps {
  readonly children: ReactNode;
  /** Changing this remounts the boundary and clears the error (navigation). */
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
    // Anything can be thrown — strings, `undefined`, an object whose
    // `toString` itself throws — so it is normalised here and the fallback can
    // rely on `.message` existing.
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  static getDerivedStateFromProps(
    props: ErrorBoundaryProps,
    state: ErrorBoundaryState,
  ): Partial<ErrorBoundaryState> | null {
    // Done here rather than in an effect: an effect runs after paint, so the
    // user would see one frame of the previous render's error.
    if (state.error !== null && state.key !== props.resetKey) {
      return { error: null, key: props.resetKey };
    }
    if (state.error === null && state.key !== props.resetKey) {
      return { key: props.resetKey };
    }
    return null;
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    // Logged unconditionally, not only in development: the overlay has no
    // devtools at all, so a silent boundary is a crash nobody ever hears about.
    console.error('overlay render error contained by boundary', error, info.componentStack);
    reportToLog('error', error, 'hud:boundary', info.componentStack ?? undefined);
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
