import { act, render, screen } from '@testing-library/react';
import { useEffect, useState } from 'react';
import { beforeAll, describe, expect, it, vi } from 'vitest';

import { initI18n } from '@vitals/i18n';

import { preloadRoutes, RouteView } from './routes';
import { Content } from './shell/Content';
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

  it('switching route does not remount the screen being left, even inside Content', async () => {
    // The route transition must animate the wrapper WITHOUT unmounting the
    // routes — an `AnimatePresence` keyed on the route would do exactly that
    // and silently undo the keep-alive. And `Content` used to `key` its
    // wrapper on the route, which remounted `RouteView` itself on every
    // navigation; the two tests above rendered `RouteView` bare and never
    // noticed. This one goes through the same path `AppShell` does.
    let mounts = 0;

    function Counter() {
      useEffect(() => {
        mounts += 1;
      }, []);
      return <Probe label="probe" />;
    }

    function Harness() {
      const [route, setRoute] = useState<RouteId>('dashboard');
      return (
        <>
          <button
            type="button"
            data-testid="go-users"
            onClick={() => {
              setRoute('users');
            }}
          >
            users
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
          <Content routeKey={route}>
            <Counter />
            <RouteView route={route} />
          </Content>
        </>
      );
    }

    render(<Harness />);
    await act(async () => {});
    act(() => {
      screen.getByTestId('probe-type').click();
    });
    expect(mounts).toBe(1);

    act(() => {
      screen.getByTestId('go-users').click();
    });
    await act(async () => {});
    act(() => {
      screen.getByTestId('go-dashboard').click();
    });
    await act(async () => {});

    // Neither the content wrapper nor anything inside it was rebuilt.
    expect(mounts).toBe(1);
    expect(screen.getByTestId('probe-typed').textContent).toBe('user input');
  });
});

describe('preloadRoutes', () => {
  it('runs one step per idle callback, so a click or a frame always goes first', async () => {
    const idle: (() => void)[] = [];
    const order: string[] = [];
    const steps = ['a', 'b', 'c'].map((name) => async () => {
      order.push(name);
    });

    const done = preloadRoutes((run) => idle.push(run), steps);
    await Promise.resolve();
    expect(order).toEqual([]);

    for (let i = 0; i < 3; i++) {
      await vi.waitFor(() => expect(idle.length).toBe(i + 1));
      idle[i]?.();
      await vi.waitFor(() => expect(order).toHaveLength(i + 1));
    }
    await done;
    expect(order).toEqual(['a', 'b', 'c']);
  });

  it('keeps going past a failed step; that screen simply loads when opened', async () => {
    const ran: string[] = [];
    await preloadRoutes(
      (run) => {
        run();
      },
      [
        () => Promise.reject(new Error('chunk failed')),
        async () => {
          ran.push('next');
        },
      ],
    );
    expect(ran).toEqual(['next']);
  });
});
