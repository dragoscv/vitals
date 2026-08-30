import { Construction } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { EmptyState, Skeleton } from '@vitals/ui';

import { DashboardScreen } from './features/dashboard';
import { ProcessesScreen } from './features/processes';
import { navItems, type RouteId } from './shell/navigation';
import { SHELL_NS } from './shell/strings';

/**
 * Placeholder for a section whose feature module does not exist yet.
 *
 * It says what is missing and why, rather than rendering nothing. A blank
 * panel in a system monitor is indistinguishable from "your computer reports
 * nothing here", which is a far more alarming message than "not built yet".
 */
function NotBuiltYet({ route }: { readonly route: RouteId }) {
  const { t } = useTranslation();
  const { t: ts } = useTranslation(SHELL_NS);
  const item = navItems.find((candidate) => candidate.id === route);
  const section = item ? t(item.labelKey) : '';

  return (
    <EmptyState
      icon={<Construction />}
      title={ts('placeholder.title', { section })}
      description={ts('placeholder.body')}
    />
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
  switch (route) {
    case 'dashboard':
      return <DashboardScreen onNavigate={onNavigate} />;
    case 'processes':
      return <ProcessesScreen />;
    case 'performance':
    case 'startup':
    case 'services':
    case 'appHistory':
    case 'users':
    case 'network':
    case 'storage':
    case 'installedApps':
    case 'devices':
    case 'benchmarks':
      return <NotBuiltYet route={route} />;
    default: {
      const exhaustive: never = route;
      return exhaustive;
    }
  }
}
