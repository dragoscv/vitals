/**
 * Contains a render error to one region instead of the whole window.
 *
 * # Why this exists
 *
 * There were no error boundaries anywhere in this app. That means a single
 * throw in any widget — a null the protocol says cannot be null, an
 * unexpected shape from a new sampler field — unmounted the entire React tree
 * and left a white window with no text, no navigation, and no way to reach
 * Settings. In a tool people open *because* something is already wrong with
 * their computer, blanking the screen is the worst possible failure: it looks
 * like Vitals broke the machine.
 *
 * The problem was visible in the console the whole time ("Consider adding an
 * error boundary to your tree") and nobody had acted on it.
 *
 * # Why a class component
 *
 * React provides no hook equivalent. `getDerivedStateFromError` and
 * `componentDidCatch` are only available on a class, and every third-party
 * boundary library is a thin wrapper around exactly this. Adding a dependency
 * for thirty lines would be worse than writing them.
 *
 * # What it deliberately does not do
 *
 * It does not retry automatically. A render that threw once with the same
 * props will throw again, and an automatic retry loop turns one error into a
 * hot loop that pins a core — on a performance monitor, of all things. The
 * user gets a button instead, which is useful precisely because live data
 * means the next render has different props.
 */

import { Component, type ErrorInfo, type ReactNode } from 'react';

import { reportToLog } from '../lib/logToFile';

export interface ErrorBoundaryProps {
  readonly children: ReactNode;
  /**
   * Changing this remounts the boundary and clears the error.
   *
   * Navigation passes the route here: having Processes crash should not leave
   * Settings unreachable, and without a reset key the boundary would keep
   * showing the old error over every subsequent screen.
   */
  readonly resetKey?: string;
  /** Rendered in place of the children. Receives a retry callback. */
  readonly fallback: (error: Error, retry: () => void) => ReactNode;
  /** Reporting hook. Kept injectable so a test can assert it is called. */
  readonly onError?: (error: Error, info: ErrorInfo) => void;
}

interface ErrorBoundaryState {
  readonly error: Error | null;
  /** The `resetKey` the current error belongs to. */
  readonly key: string | undefined;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  override state: ErrorBoundaryState = { error: null, key: undefined };

  static getDerivedStateFromError(error: unknown): Partial<ErrorBoundaryState> {
    // Anything can be thrown in JavaScript, including strings and undefined.
    // Normalising here means the fallback can rely on `.message` existing
    // rather than each one re-deriving it — and a fallback that itself throws
    // while formatting the error escapes this boundary entirely.
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  /**
   * Clears a stale error when the route changes.
   *
   * Done in `getDerivedStateFromProps` rather than an effect: an effect runs
   * after paint, so the user would see one frame of the previous screen's
   * error before the new screen replaced it.
   */
  static getDerivedStateFromProps(
    props: ErrorBoundaryProps,
    state: ErrorBoundaryState,
  ): Partial<ErrorBoundaryState> | null {
    if (state.error !== null && state.key !== props.resetKey) {
      return { error: null, key: props.resetKey };
    }
    if (state.error === null && state.key !== props.resetKey) {
      return { key: props.resetKey };
    }
    return null;
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    // Logged unconditionally, not only in development. A release build with a
    // silent boundary is how a recurring crash goes unreported for months.
    console.error('render error contained by boundary', error, info.componentStack);
    // And to the log file: the console above is invisible in a release build.
    reportToLog('error', error, 'boundary', info.componentStack ?? undefined);
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
