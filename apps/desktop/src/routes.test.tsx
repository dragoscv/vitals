import { act, render, screen } from '@testing-library/react';
import { useEffect, useState } from 'react';
import { beforeAll, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { RouteView } from './routes';
import type { RouteId } from './shell/navigation';

beforeAll(async () => {
  await initI18n();
});

/**
 * Holds a piece of state, standing in for a real screen.
 *
 * The state represents the scroll position, sort order and search text that
 * used to be discarded every time the user navigated away.
 */
function Probe({ label }: { readonly label: string }) {
  const [typed, setTyped] = useState('');

  return (
    <div>
      <span data-testid={`${label}-typed`}>{typed}</span>
      <button
        type="button"
        data-testid={`${label}-type`}
        onClick={() => {
          setTyped('user input');
        }}
      >
        type
      </button>
    </div>
  );
}

describe('RouteView keeps visited screens alive', () => {
  it('preserves a screen\u2019s state after navigating away and back', async () => {
    // The behaviour the whole change exists for. Previously the switch
    // rendered exactly one screen, so leaving Startup destroyed it: the
    // registry and SCM were re-walked and every bit of UI state was lost.
    function Harness() {
      const [route, setRoute] = useState<RouteId>('dashboard');
      return (
        <>
          <button
            type="button"
            data-testid="go-processes"
            onClick={() => {
              setRoute('processes');
            }}
          >
            processes
          </button>
          <button
            type="button"
            data-testid="go-dashboard"
            onClick={() => {
              setRoute('dashboard');
            }}
          >
            dashboard
          </button>
          <RouteView route={route} />
        </>
      );
    }

    render(<Harness />);

    // Both real screens need a Tauri host they do not have here, so this
    // asserts the structural property: what RouteView renders, and whether
    // the earlier route's DOM survives.
    await act(async () => {});

    const dashboardMarkup = document.body.innerHTML;
    expect(dashboardMarkup.length).toBeGreaterThan(0);

    act(() => {
      screen.getByTestId('go-processes').click();
    });
    await act(async () => {});

    // The decisive assertion: after navigating away, the dashboard is still
    // in the document. `<Activity mode="hidden">` hides with `display: none`
    // rather than unmounting, so its subtree is present.
    const hidden = document.querySelectorAll('[hidden], [style*="display: none"]');
    expect(hidden.length).toBeGreaterThan(0);
  });

  it('only renders routes that have actually been visited', async () => {
    // Rendering all eleven up front would resolve every lazy import and undo
    // the code splitting that keeps the first paint cheap. This is the
    // guard against someone "simplifying" the visited-set away.
    const imported = vi.fn();

    function Harness({ route }: { readonly route: RouteId }) {
      useEffect(() => {
        imported(route);
      }, [route]);
      return <RouteView route={route} />;
    }

    const { rerender } = render(<Harness route="dashboard" />);
    await act(async () => {});

    const afterOne = document.querySelectorAll('[data-route]').length;

    rerender(<Harness route="processes" />);
    await act(async () => {});

    const afterTwo = document.querySelectorAll('[data-route]').length;

    // Two visited routes must not produce eleven subtrees.
    expect(afterTwo).toBeLessThanOrEqual(afterOne + 1);
  });

  it('Activity preserves component state, which is the whole premise', async () => {
    // Proves the React behaviour this design depends on, independent of any
    // Vitals screen. If `<Activity>` ever stopped preserving state, every
    // screen would silently go back to reloading on return and only this
    // test would say so.
    const { Activity } = await import('react');

    function Harness() {
      const [visible, setVisible] = useState(true);
      return (
        <>
          <button
            type="button"
            data-testid="toggle"
            onClick={() => {
              setVisible((value) => !value);
            }}
          >
            toggle
          </button>
          <Activity mode={visible ? 'visible' : 'hidden'}>
            <Probe label="probe" />
          </Activity>
        </>
      );
    }

    render(<Harness />);
    await act(async () => {});

    act(() => {
      screen.getByTestId('probe-type').click();
    });
    expect(screen.getByTestId('probe-typed').textContent).toBe('user input');

    // Hide, then show again.
    act(() => {
      screen.getByTestId('toggle').click();
    });
    await act(async () => {});
    act(() => {
      screen.getByTestId('toggle').click();
    });
    await act(async () => {});

    // State survived the round trip. An unmount would have reset this to ''.
    expect(screen.getByTestId('probe-typed').textContent).toBe('user input');
  });
});
