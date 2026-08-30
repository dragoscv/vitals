import { Suspense, lazy } from 'react';

import { Skeleton } from '@vitals/ui';

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
  return (
    // Keyed on the route so switching sections shows the skeleton again
    // rather than holding the previous screen visible while the next chunk
    // loads. Without the key React would keep the old boundary mounted and
    // the app would appear frozen for the duration of the fetch.
    <Suspense key={route} fallback={<RouteSkeleton />}>
      <RouteContent route={route} {...(onNavigate && { onNavigate })} />
    </Suspense>
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
