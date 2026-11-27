/**
 * Contains a render error on the TV instead of leaving a black screen.
 *
 * # Why not the desktop's boundary
 *
 * The TV app is a separate program with its own bundle: it does not import
 * the desktop shell at all, so the shell's `ErrorBoundary` is not reachable
 * from here. The prop shape is kept identical so the two read the same way.
 *
 * # Why a class, and why no automatic retry
 *
 * React has no hook equivalent. It does not retry on its own because the same
 * props would throw again; the button is offered because the readings are
 * live and the next render often succeeds. A viewer three metres from the
 * screen needs a focus stop they can reach with the remote, which `autoFocus`
 * provides.
 */

import { Component, type ErrorInfo, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { Action, Hint } from './components';

export interface ErrorBoundaryProps {
  readonly children: ReactNode;
  /** Changing this remounts the boundary and clears the error. */
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
    // Anything can be thrown, so it is normalised here and the fallback can
    // rely on `.message` existing.
    return { error: error instanceof Error ? error : new Error(String(error)) };
  }

  static getDerivedStateFromProps(
    props: ErrorBoundaryProps,
    state: ErrorBoundaryState,
  ): Partial<ErrorBoundaryState> | null {
    // In `getDerivedStateFromProps`, not an effect: an effect runs after
    // paint, so the previous screen's error would flash for one frame.
    if (state.error !== null && state.key !== props.resetKey) {
      return { error: null, key: props.resetKey };
    }
    if (state.error === null && state.key !== props.resetKey) {
      return { key: props.resetKey };
    }
    return null;
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    // A TV has no convenient devtools; the remote console is the only surface.
    console.error('tizen render error contained by boundary', error, info.componentStack);
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

/** What a crashed screen looks like: the reason, in words, with a way back. */
export function RootError({
  error,
  onRetry,
}: {
  readonly error: Error;
  readonly onRetry: () => void;
}): React.JSX.Element {
  const { t } = useTranslation();
  return (
    <div className="screen">
      <h2>{t('common.error')}</h2>
      <Hint>{error.message}</Hint>
      <Action label={t('common.retry')} onPress={onRetry} autoFocus />
    </div>
  );
}
