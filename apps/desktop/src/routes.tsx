import {
  Activity,
  Suspense,
  lazy,
  startTransition,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from 'react';

import { Skeleton, useReducedMotion } from '@vitals/ui';

import { DashboardScreen } from './features/dashboard';
import { hasTauriHost } from './shell/host';
import { routeIds, type RouteId } from './shell/navigation';

// Dashboard is eager: it is what the window opens on, so deferring it would
// only add a flash of skeleton to the one screen whose load time is the
// app's perceived startup time.
//
// Every other section is lazy, so the first paint does not pay for eleven
// screens. They are then preloaded in the background — chunk AND first
// data — by `preloadRoutes` below, so a first visit is as fast as a return
// visit. Lazy-then-preload keeps both properties: a fast launch and no
// skeleton on any tab.
//
// Each chunk registers its own translations as it loads. That has to happen
// before the component renders, which is exactly what awaiting it inside the
// `lazy` factory guarantees — registering from `main.tsx` instead would
// import every barrel eagerly and collapse the split back into one bundle.
//
// Each loader is memoised: `lazy` and the preloader share one promise, so the
// chunk is fetched and the strings registered exactly once, whichever asks
// first.
function once<T>(load: () => Promise<T>): () => Promise<T> {
  let promise: Promise<T> | null = null;
  return () => {
    promise ??= load().catch((error: unknown) => {
      // A failed chunk (a dev-server restart, a disk hiccup) is retried by
      // the next caller rather than cached as a permanent failure.
      promise = null;
      throw error;
    });
    return promise;
  };
}

const loadConnections = once(async () => {
  const m = await import('./features/connections');
  m.registerConnectionStrings();
  return m;
});
const loadApps = once(async () => {
  const m = await import('./features/apps');
  m.registerAppsStrings();
  return m;
});
const loadBenchmarks = once(async () => {
  const m = await import('./features/benchmarks');
  m.registerBenchmarksStrings();
  return m;
});
const loadDevices = once(async () => {
  const m = await import('./features/devices');
  m.registerDevicesStrings();
  return m;
});
const loadHistory = once(async () => {
  const m = await import('./features/history');
  m.registerHistoryStrings();
  return m;
});
const loadPerformance = once(async () => {
  const m = await import('./features/performance');
  m.registerPerformanceStrings();
  return m;
});
const loadProcesses = once(async () => {
  const m = await import('./features/processes');
  m.registerProcessesStrings();
  return m;
});
const loadStartup = once(async () => {
  const m = await import('./features/startup');
  m.registerStartupStrings();
  return m;
});
const loadStorage = once(async () => {
  const m = await import('./features/storage');
  m.registerStorageStrings();
  return m;
});
const loadUsers = once(async () => {
  const m = await import('./features/users');
  m.registerUsersStrings();
  return m;
});

const ConnectionsScreen = lazy(async () => ({
  default: (await loadConnections()).ConnectionsScreen,
}));
const AppsScreen = lazy(async () => ({ default: (await loadApps()).AppsScreen }));
const BenchmarksScreen = lazy(async () => ({ default: (await loadBenchmarks()).BenchmarksScreen }));
const DevicesScreen = lazy(async () => ({ default: (await loadDevices()).DevicesScreen }));
const AppHistoryScreen = lazy(async () => ({ default: (await loadHistory()).AppHistoryScreen }));
const PerformanceScreen = lazy(async () => ({
  default: (await loadPerformance()).PerformanceScreen,
}));
const ProcessesScreen = lazy(async () => ({ default: (await loadProcesses()).ProcessesScreen }));
const StartupScreen = lazy(async () => ({ default: (await loadStartup()).StartupScreen }));
const StorageScreen = lazy(async () => ({ default: (await loadStorage()).StorageScreen }));
const UsersScreen = lazy(async () => ({ default: (await loadUsers()).UsersScreen }));

/**
 * What each section needs before its first paint: its chunk, then a
 * background read of its data (lib/prefetch). In the order a user is most
 * likely to open them, so the probable next click is ready first.
 *
 * Only reads — nothing here scans a disk or runs a benchmark. Screens fed by
 * the metrics stream (Performance, Processes) need no read: the stream is
 * always listening (lib/metrics), so they render from the current frame.
 */
const preloaders: readonly (() => Promise<unknown>)[] = [
  loadProcesses,
  loadPerformance,
  async () => (await loadStartup()).prefetchStartup(false),
  async () => (await loadStartup()).prefetchStartup(true),
  async () => (await loadApps()).prefetchApps(),
  async () => (await loadConnections()).prefetchConnections(),
  async () => (await loadUsers()).prefetchUsers(),
  async () => (await loadHistory()).prefetchAppHistory(),
  async () => (await loadStorage()).prefetchStorage(),
  async () => (await loadDevices()).prefetchSensors(),
  async () => (await loadBenchmarks()).prefetchBenchmarks(),
];

/**
 * Loads every section in the background, one at a time, while the window is
 * idle.
 *
 * One at a time rather than all at once: eleven chunks parsing and eleven
 * commands answering in the same second would make the dashboard stutter
 * for exactly the second after launch the user is looking at it. Each step
 * waits for an idle callback, so a click or a frame always goes first.
 *
 * Failures are ignored here. A section whose preload failed simply loads
 * when opened, as it did before, and reports its own error there.
 */
export async function preloadRoutes(
  idle: (run: () => void) => void = whenIdle,
  steps: readonly (() => Promise<unknown>)[] = preloaders,
): Promise<void> {
  for (const step of steps) {
    await new Promise<void>((resolve) => {
      idle(resolve);
    });
    await step().catch(() => undefined);
  }
}

function whenIdle(run: () => void): void {
  // `requestIdleCallback` is in every WebView2 build; the timeout keeps a
  // constantly-busy dashboard from postponing the preload forever.
  if (typeof requestIdleCallback === 'function') requestIdleCallback(run, { timeout: 500 });
  else setTimeout(run, 50);
}

/**
 * Fades and lifts the content area when the section changes.
 *
 * Short and small on purpose — 140 ms and four pixels. This is a window
 * someone opens because their computer is misbehaving, and a section that
 * slides in from across the panel puts animation between them and the number
 * they came to read.
 *
 * Two constraints shaped this, and both rule out the obvious
 * `AnimatePresence` keyed on the route:
 *
 * 1. Every visited screen stays mounted inside an `<Activity>` so its scroll
 *    position, sort order and search text survive navigation.
 *    `AnimatePresence` animates components in and out of the *tree*, so it
 *    would unmount the screen being left and undo exactly the property this
 *    file exists to provide. Here the wrapper is one plain `<div>` that never
 *    changes identity; only its style moves.
 * 2. The size budget. Measured on this build, gzip, in the entry chunk:
 *    Motion's `m` component 7.7 KB, `useAnimationControls` a further 15 KB,
 *    and even `motion/mini`'s `animate` loaded lazily still pulled 10 KB of
 *    shared internals into the entry because the feature bundle chunk needs
 *    the same modules — against 4.3 KB of headroom. The Web Animations API
 *    does a one-property fade natively for nothing, and WebView2 has shipped
 *    it for years. Motion stays for the things it is good at; this is not
 *    one of them.
 *
 * Reduced motion collapses the duration to zero rather than shortening it:
 * the point is that the content is simply there, not that it arrives faster.
 * The hook reads the same `data-reduce-motion` override `MotionConfig` is fed
 * from, so this and every `m.*` component agree.
 */
function RouteTransition({
  route,
  children,
}: {
  readonly route: RouteId;
  readonly children: ReactNode;
}) {
  const element = useRef<HTMLDivElement | null>(null);
  // The route this wrapper last animated to. The startup paint is not a
  // transition — fading in the dashboard on launch would only delay the one
  // screen whose appearance is the perceived start time.
  const shown = useRef<RouteId | null>(null);
  const reduced = useReducedMotion();

  useEffect(() => {
    const target = element.current;
    if (shown.current === route || target === null) return;
    const first = shown.current === null;
    shown.current = route;
    if (first || reduced) return;
    // The shell navigates inside a View Transition where the platform has
    // one; the browser already cross-fades the whole view, and a second fade
    // on top would make the new screen arrive twice. This path remains for
    // a WebView without the API and for tests.
    if ('startViewTransition' in document) return;

    const animation = target.animate(
      [
        { opacity: 0, transform: 'translateY(6px)', filter: 'blur(3px)' },
        { opacity: 1, transform: 'none', filter: 'none' },
      ],
      { duration: 220, easing: 'cubic-bezier(0.25, 1, 0.5, 1)' },
    );

    return () => {
      // A route change mid-fade: jump to the end rather than letting the old
      // fade run on top of the new one. `finish`, not `cancel`: cancelling
      // would be fine visually (no fill), but happy-dom rejects `finished`
      // on cancel and every test would log an unhandled error.
      animation.finish();
    };
  }, [route, reduced]);

  // A flex column, like every wrapper between `<main>` and a screen: see
  // Content.tsx for why the height has to reach the screen root.
  return (
    <div ref={element} className="flex min-h-0 flex-1 flex-col">
      {children}
    </div>
  );
}

/**
 * Shown while a section's data is still arriving.
 *
 * Skeletons rather than a spinner: the shapes reserve the space the real
 * content will occupy, so nothing jumps when it lands. In a UI that updates
 * every second, layout shift is the difference between readable and nauseating.
 */
export function RouteSkeleton() {
  return (
    <div aria-busy="true" className="space-y-3">
      <Skeleton className="h-7 w-48" />
      <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {[0, 1, 2, 3, 4, 5].map((index) => (
          <Skeleton key={index} className="h-28" />
        ))}
      </div>
    </div>
  );
}

/**
 * Resolves a route to its view.
 *
 * A plain switch rather than a registry object so that adding a `RouteId`
 * without a view is a compile error at the `never` case, not a blank panel
 * discovered by a user.
 */
export function RouteView({
  route,
  onNavigate,
}: {
  readonly route: RouteId;
  /** Lets a screen send the user elsewhere — dashboard alerts do this. */
  readonly onNavigate?: (route: RouteId) => void;
}) {
  // Every route the user has opened stays mounted, hidden, for the rest of
  // the session.
  //
  // Previously exactly one screen existed at a time, so leaving a section
  // destroyed it and returning re-ran its whole load: scroll position, sort
  // order, expanded rows and search text all reset, and the Startup tab paid
  // another registry-and-SCM walk to redraw a list the user had just been
  // reading.
  //
  // `<Activity mode="hidden">` keeps the state and the DOM but tears down
  // effects, so a hidden screen holds no subscription and no timer. That is
  // the property that makes this affordable: eleven mounted screens cost
  // eleven React trees in memory, not eleven pollers.
  //
  // Only VISITED routes are rendered at first. Rendering all eleven up front
  // would resolve every `lazy` import immediately and undo the code
  // splitting that keeps the first paint cheap. Once the background preload
  // has loaded every chunk and read every screen's data, the rest are
  // mounted hidden too (below): mounting a screen — building a 800-row
  // table's tree — was most of the ~400 ms a first visit still took after its
  // data was ready, and a hidden mount in idle time costs the user nothing.
  // State rather than a ref: a ref mutated during render is invisible to
  // React's concurrent scheduler, which may render a component twice or
  // discard the result. The lint rule that forbids it is correct — the
  // symptom would be a route occasionally not appearing in the set at all.
  //
  // The updater returns the SAME set when the route is already known, so a
  // revisit is not a state change and does not re-render.
  const [visited, setVisited] = useState<ReadonlySet<RouteId>>(() => new Set([route]));

  // After the first paint, never before it: the window's first frame is the
  // app's perceived start time. Host-only — without one every read answers
  // "no host" at once and there is nothing to warm. `preloadRoutes` is
  // idempotent in effect (memoised chunks, prefetch keeps fresh entries), so
  // StrictMode's second mount costs nothing.
  useEffect(() => {
    if (!hasTauriHost()) return;
    let live = true;
    void preloadRoutes().then(() => {
      if (!live) return;
      // A transition, so React may split the work and a click interrupts it.
      startTransition(() => {
        setVisited((current) =>
          current.size === routeIds.length ? current : new Set([...current, ...routeIds]),
        );
      });
    });
    return () => {
      live = false;
    };
  }, []);

  if (!visited.has(route)) {
    // Setting state during render is the supported way to derive state from
    // props. React discards the in-progress render and immediately retries
    // with the new value, before anything reaches the DOM.
    setVisited((current) => (current.has(route) ? current : new Set(current).add(route)));
  }

  return (
    <RouteTransition route={route}>
      {[...visited].map((id) => (
        // Keyed per route, and each gets its own boundary: a section still
        // loading its chunk must not blank a sibling that is already up.
        <Activity key={id} mode={id === route ? 'visible' : 'hidden'}>
          {/*
           * `data-route-visible` marks the one route on screen; styles.css
           * gives only its title a `view-transition-name`. Hidden routes are
           * still in the DOM, and two elements sharing a name abort the
           * whole transition — the CSS cannot tell them apart, React can.
           */}
          <div
            className="flex min-h-0 flex-1 flex-col"
            {...(id === route && { 'data-route-visible': '' })}
          >
            <Suspense fallback={<RouteSkeleton />}>
              <RouteContent route={id} {...(onNavigate && { onNavigate })} />
            </Suspense>
          </div>
        </Activity>
      ))}
    </RouteTransition>
  );
}

function RouteContent({
  route,
  onNavigate,
}: {
  readonly route: RouteId;
  readonly onNavigate?: (route: RouteId) => void;
}) {
  switch (route) {
    case 'dashboard':
      return <DashboardScreen onNavigate={onNavigate} />;
    case 'performance':
      return <PerformanceScreen />;
    case 'processes':
      return <ProcessesScreen />;
    case 'network':
      return <ConnectionsScreen />;
    case 'startup':
      return <StartupScreen mode="startup" />;
    case 'services':
      // Same component, different mode: the two share one backend call, and
      // `services` is what asks for start types — an SCM round trip each.
      return <StartupScreen mode="services" />;
    case 'installedApps':
      return <AppsScreen />;
    case 'storage':
      return <StorageScreen />;
    case 'devices':
      return <DevicesScreen />;
    case 'benchmarks':
      return <BenchmarksScreen />;
    case 'appHistory':
      return <AppHistoryScreen />;
    case 'users':
      return <UsersScreen />;
    default: {
      const exhaustive: never = route;
      return exhaustive;
    }
  }
}
