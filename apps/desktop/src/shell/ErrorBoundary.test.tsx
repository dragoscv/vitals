import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ErrorBoundary } from './ErrorBoundary';

/** Throws on demand, so a test can flip it between renders. */
function Bomb({ throws, label = 'ok' }: { readonly throws: boolean; readonly label?: string }) {
  if (throws) throw new Error('widget exploded');
  return <p>{label}</p>;
}

const fallback = (error: Error, retry: () => void) => (
  <div>
    <p>caught: {error.message}</p>
    <button onClick={retry}>retry</button>
  </div>
);

beforeEach(() => {
  // React logs the caught error itself, and the boundary logs it again on
  // purpose. Both are expected here and would otherwise bury the assertions.
  vi.spyOn(console, 'error').mockImplementation(() => undefined);
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe('ErrorBoundary', () => {
  it('renders children when nothing throws', () => {
    render(
      <ErrorBoundary fallback={fallback}>
        <Bomb throws={false} />
      </ErrorBoundary>,
    );

    expect(screen.getByText('ok')).toBeTruthy();
  });

  it('renders the fallback instead of unmounting the tree', () => {
    // Without a boundary this throw blanks the entire window — no navigation,
    // no Settings, no text. In a tool people open because something is
    // already wrong, that looks like Vitals broke the machine.
    render(
      <ErrorBoundary fallback={fallback}>
        <Bomb throws />
      </ErrorBoundary>,
    );

    expect(screen.getByText('caught: widget exploded')).toBeTruthy();
  });

  it('normalises a non-Error throw', () => {
    // JavaScript permits throwing anything. A fallback reading `.message` off
    // a string would throw while rendering the error, escaping the boundary.
    function ThrowString(): never {
      throw 'just a string';
    }

    render(
      <ErrorBoundary fallback={fallback}>
        <ThrowString />
      </ErrorBoundary>,
    );

    expect(screen.getByText('caught: just a string')).toBeTruthy();
  });

  it('reports the error so a crash is not silent', () => {
    const onError = vi.fn();

    render(
      <ErrorBoundary fallback={fallback} onError={onError}>
        <Bomb throws />
      </ErrorBoundary>,
    );

    expect(onError).toHaveBeenCalledOnce();
    expect(onError.mock.calls[0]?.[0]).toBeInstanceOf(Error);
  });

  it('recovers when retry is pressed and the cause has passed', () => {
    // Worth offering manually because the data is live: the next render
    // genuinely has different props, so retrying after a bad frame works.
    function Flaky() {
      return <Bomb throws={shouldThrow} label="recovered" />;
    }
    let shouldThrow = true;

    render(
      <ErrorBoundary fallback={fallback}>
        <Flaky />
      </ErrorBoundary>,
    );
    expect(screen.getByText('caught: widget exploded')).toBeTruthy();

    shouldThrow = false;
    fireEvent.click(screen.getByRole('button', { name: 'retry' }));

    expect(screen.getByText('recovered')).toBeTruthy();
  });

  it('does not retry on its own', () => {
    // An automatic retry loop turns one error into a hot loop pinning a core,
    // inside a performance monitor of all things.
    let renders = 0;
    function Counting(): never {
      renders += 1;
      throw new Error('always');
    }

    render(
      <ErrorBoundary fallback={fallback}>
        <Counting />
      </ErrorBoundary>,
    );

    // The invariant is that it SETTLES, not a specific count: React re-invokes
    // a throwing component in development to recover a component stack, so the
    // number is an implementation detail and pinning it makes the test fail on
    // a React upgrade for no reason.
    const afterMount = renders;
    expect(afterMount).toBeGreaterThan(0);

    // Nothing further happens without user action. A boundary that retried on
    // its own would keep incrementing this forever.
    expect(renders).toBe(afterMount);
    expect(screen.getByText('caught: always')).toBeTruthy();
  });

  it('clears the error when the route changes', () => {
    // Otherwise a crash in Processes shows its error over every screen the
    // user visits afterwards, and Settings becomes unreachable.
    const { rerender } = render(
      <ErrorBoundary resetKey="processes" fallback={fallback}>
        <Bomb throws />
      </ErrorBoundary>,
    );
    expect(screen.getByText('caught: widget exploded')).toBeTruthy();

    rerender(
      <ErrorBoundary resetKey="settings" fallback={fallback}>
        <Bomb throws={false} label="settings screen" />
      </ErrorBoundary>,
    );

    expect(screen.getByText('settings screen')).toBeTruthy();
  });

  it('keeps showing the error while the route is unchanged', () => {
    const { rerender } = render(
      <ErrorBoundary resetKey="processes" fallback={fallback}>
        <Bomb throws />
      </ErrorBoundary>,
    );

    rerender(
      <ErrorBoundary resetKey="processes" fallback={fallback}>
        <Bomb throws={false} />
      </ErrorBoundary>,
    );

    expect(screen.getByText('caught: widget exploded')).toBeTruthy();
  });
});
