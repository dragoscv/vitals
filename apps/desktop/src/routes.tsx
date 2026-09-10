import { Activity, Suspense, lazy, useEffect, useRef, useState, type ReactNode } from 'react';

import { Skeleton, useReducedMotion } from '@vitals/ui';

import { DashboardScreen } from './features/dashboard';
import { type RouteId } from './shell/navigation';

// Dashboard is eager: it is what the window opens on, so deferring it would
// only add a flash of skeleton to the one screen whose load time is the
// app's perceived startup time.
//
// Every other section is lazy. Only one is ever on screen, and most sessions
// touch two or three, so eagerly parsing all eleven made the first paint pay
// for Benchmarks, Storage and Users that the user may never open. Each of
// these is a chunk the browser fetches from local disk on first navigation.
//
// Each chunk registers its own translations as it loads. That has to happen
// before the component renders, which is exactly what awaiting it inside the
// `lazy` factory guarantees — registering from `main.tsx` instead would
// import every barrel eagerly and collapse the split back into one bundle.
const ConnectionsScreen = lazy(async () => {
  const m = await import('./features/connections');
  m.registerConnectionStrings();
  return { default: m.ConnectionsScreen };
});
const AppsScreen = lazy(async () => {
  const m = await import('./features/apps');
  m.registerAppsStrings();
  return { default: m.AppsScreen };
});
const BenchmarksScreen = lazy(async () => {
  const m = await import('./features/benchmarks');
  m.registerBenchmarksStrings();
  return { default: m.BenchmarksScreen };
});
const DevicesScreen = lazy(async () => {
  const m = await import('./features/devices');
  m.registerDevicesStrings();
  return { default: m.DevicesScreen };
});
const AppHistoryScreen = lazy(async () => {
  const m = await import('./features/history');
  m.registerHistoryStrings();
  return { default: m.AppHistoryScreen };
});
const PerformanceScreen = lazy(async () => {
  const m = await import('./features/performance');
  m.registerPerformanceStrings();
  return { default: m.PerformanceScreen };
});
const ProcessesScreen = lazy(async () => {
  const m = await import('./features/processes');
  m.registerProcessesStrings();
  return { default: m.ProcessesScreen };
});
const StartupScreen = lazy(async () => {
  const m = await import('./features/startup');
  m.registerStartupStrings();
  return { default: m.StartupScreen };
});
const StorageScreen = lazy(async () => {
  const m = await import('./features/storage');
  m.registerStorageStrings();
  return { default: m.StorageScreen };
});
const UsersScreen = lazy(async () => {
  const m = await import('./features/users');
  m.registerUsersStrings();
  return { default: m.UsersScreen };
});

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

    const animation = target.animate(
      [
        { opacity: 0, transform: 'translateY(4px)' },
        { opacity: 1, transform: 'none' },
      ],
      { duration: 140, easing: 'ease-out' },
    );

    return () => {
      // A route change mid-fade: jump to the end rather than letting the old
      // fade run on top of the new one. `finish`, not `cancel`: cancelling
      // would be fine visually (no fill), but happy-dom rejects `finished`
      // on cancel and every test would log an unhandled error.
      animation.finish();
    };
  }, [route, reduced]);

  return <div ref={element}>{children}</div>;
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
  // Only VISITED routes are rendered. Rendering all eleven up front would
  // resolve every `lazy` import immediately and undo the code splitting that
  // keeps the first paint cheap.
  // State rather than a ref: a ref mutated during render is invisible to
  // React's concurrent scheduler, which may render a component twice or
  // discard the result. The lint rule that forbids it is correct — the
  // symptom would be a route occasionally not appearing in the set at all.
  //
  // The updater returns the SAME set when the route is already known, so a
  // revisit is not a state change and does not re-render.
  const [visited, setVisited] = useState<ReadonlySet<RouteId>>(() => new Set([route]));

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
          <Suspense fallback={<RouteSkeleton />}>
            <RouteContent route={id} {...(onNavigate && { onNavigate })} />
          </Suspense>
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
