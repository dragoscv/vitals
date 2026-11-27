/**
 * Contains a render error on the phone instead of leaving a blank page.
 *
 * # Why a copy of the shell's boundary
 *
 * `shell/ErrorBoundary` is the same idea, but the phone must not import it:
 * the mobile bundle is `src/mobile/**` and nothing else (`boundary.test.ts`),
 * because the shell's boundary reaches `lib/logToFile`, which pulls in
 * `@tauri-apps/*` — code with no host on a phone and bytes a LAN page should
 * not download. The prop shape is kept identical so the two can be read side
 * by side; the only difference is that there is no log file to write to here,
 * so a report goes to the console and to `onError`.
 *
 * # Why a class, and why no automatic retry
 *
 * React offers no hook equivalent (`getDerivedStateFromError` and
 * `componentDidCatch` are class-only). It deliberately does not retry on its
 * own: a render that threw once with the same props will throw again, and a
 * loop is worse than a button. The user gets the button, which is worth it
 * precisely because the data is live — the next render has different props.
 */

import { RotateCcw, TriangleAlert } from 'lucide-react';
import { Component, type ErrorInfo, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { Button, EmptyState } from '@vitals/ui';

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
    // user would see one frame of the previous screen's error.
    if (state.error !== null && state.key !== props.resetKey) {
      return { error: null, key: props.resetKey };
    }
    if (state.error === null && state.key !== props.resetKey) {
      return { key: props.resetKey };
    }
    return null;
  }

  override componentDidCatch(error: Error, info: ErrorInfo): void {
    // A phone has no log file and no devtools; the console is where the
    // browser's own error surface reads from.
    console.error('mobile render error contained by boundary', error, info.componentStack);
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

/** What a crashed phone screen looks like: the reason, in words, with a way back. */
export function RootError({
  error,
  onRetry,
}: {
  readonly error: Error;
  readonly onRetry: () => void;
}): React.JSX.Element {
  const { t } = useTranslation();
  return (
    <EmptyState
      className="flex-1"
      icon={<TriangleAlert />}
      title={t('common.error')}
      description={error.message}
      action={
        <Button onClick={onRetry}>
          <RotateCcw aria-hidden className="size-4" />
          {t('common.retry')}
        </Button>
      }
    />
  );
}
