import {
  Activity,
  Boxes,
  ClockArrowUp,
  Cpu,
  Gauge,
  HardDrive,
  LayoutDashboard,
  ListTree,
  Network,
  Server,
  Usb,
  Users,
  type LucideIcon,
} from 'lucide-react';

/**
 * Every destination in the application.
 *
 * A closed union rather than free-form strings: the route is persisted and
 * restored across restarts, so a value that no longer exists must be a
 * compile error here and a validated rejection at load time — not a blank
 * window the user can only escape by clearing their settings.
 */
export const routeIds = [
  'dashboard',
  'performance',
  'processes',
  'startup',
  'services',
  'appHistory',
  'users',
  'network',
  'storage',
  'installedApps',
  'devices',
  'benchmarks',
] as const;

export type RouteId = (typeof routeIds)[number];

export const defaultRoute: RouteId = 'dashboard';

export function isRouteId(value: unknown): value is RouteId {
  return typeof value === 'string' && (routeIds as readonly string[]).includes(value);
}

export interface NavItem {
  readonly id: RouteId;
  /** Key into the `nav.*` namespace. Resolved at render time, never here. */
  readonly labelKey: string;
  readonly icon: LucideIcon;
}

/**
 * Sidebar order.
 *
 * Grouped by how often the item is opened rather than alphabetically: the
 * first four cover the reason almost anyone opens a task manager, and burying
 * Processes halfway down an alphabetised list would be actively hostile.
 */
export const navItems: readonly NavItem[] = [
  { id: 'dashboard', labelKey: 'nav.dashboard', icon: LayoutDashboard },
  { id: 'performance', labelKey: 'nav.performance', icon: Activity },
  { id: 'processes', labelKey: 'nav.processes', icon: ListTree },
  { id: 'startup', labelKey: 'nav.startup', icon: ClockArrowUp },
  { id: 'services', labelKey: 'nav.services', icon: Server },
  { id: 'appHistory', labelKey: 'nav.appHistory', icon: Gauge },
  { id: 'users', labelKey: 'nav.users', icon: Users },
  { id: 'network', labelKey: 'nav.network', icon: Network },
  { id: 'storage', labelKey: 'nav.storage', icon: HardDrive },
  { id: 'installedApps', labelKey: 'nav.installedApps', icon: Boxes },
  { id: 'devices', labelKey: 'nav.devices', icon: Usb },
  { id: 'benchmarks', labelKey: 'nav.benchmarks', icon: Cpu },
];
