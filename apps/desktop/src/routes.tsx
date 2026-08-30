import { Skeleton } from '@vitals/ui';

import { DashboardScreen } from './features/dashboard';
import { ConnectionsScreen } from './features/connections';
import { AppsScreen } from './features/apps';
import { BenchmarksScreen } from './features/benchmarks';
import { DevicesScreen } from './features/devices';
import { AppHistoryScreen } from './features/history';
import { PerformanceScreen } from './features/performance';
import { ProcessesScreen } from './features/processes';
import { StartupScreen } from './features/startup';
import { StorageScreen } from './features/storage';
import { UsersScreen } from './features/users';
import { type RouteId } from './shell/navigation';

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
