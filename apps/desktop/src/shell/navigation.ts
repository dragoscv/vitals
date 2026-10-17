/**
 * Fired on `window` by the command palette after navigating to the dashboard;
 * the dashboard opens the "Why is my PC slow?" dialog in response.
 *
 * Lives here rather than in the palette module because the dashboard is in
 * the entry chunk and the palette is lazy — importing the palette from the
 * dashboard would drag it into the initial load.
 */
export const DIAGNOSE_EVENT = 'vitals:diagnose';

/**
 * Fired on `window` by a screen that needs the Settings dialog open — the
 * History widget, when recording is off. The dialog is owned by the shell,
 * and threading an opener through the route tree for one button is more
 * plumbing than the event it replaces.
 */
export const OPEN_SETTINGS_EVENT = 'vitals:open-settings';
import {
  Activity,
  Bot,
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
  'agents',
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
  readonly group: NavGroupId;
}

/**
 * The sidebar's three groups (DES-25). Twelve undifferentiated items read as
 * a wall; three short runs under quiet headings let the eye skip straight to
 * "what is happening now" versus "what is on this machine".
 */
export const navGroupIds = ['now', 'machine', 'more'] as const;
export type NavGroupId = (typeof navGroupIds)[number];

/**
 * Sidebar order.
 *
 * Grouped by how often the item is opened rather than alphabetically: the
 * first group covers the reason almost anyone opens a task manager, and
 * burying Processes halfway down an alphabetised list would be actively
 * hostile. The flat order is also the Ctrl+1…9 order, so it follows the
 * groups exactly.
 */
export const navItems: readonly NavItem[] = [
  { id: 'dashboard', labelKey: 'nav.dashboard', icon: LayoutDashboard, group: 'now' },
  { id: 'performance', labelKey: 'nav.performance', icon: Activity, group: 'now' },
  { id: 'processes', labelKey: 'nav.processes', icon: ListTree, group: 'now' },
  { id: 'agents', labelKey: 'nav.agents', icon: Bot, group: 'now' },
  { id: 'startup', labelKey: 'nav.startup', icon: ClockArrowUp, group: 'machine' },
  { id: 'services', labelKey: 'nav.services', icon: Server, group: 'machine' },
  { id: 'storage', labelKey: 'nav.storage', icon: HardDrive, group: 'machine' },
  { id: 'installedApps', labelKey: 'nav.installedApps', icon: Boxes, group: 'machine' },
  { id: 'devices', labelKey: 'nav.devices', icon: Usb, group: 'machine' },
  { id: 'network', labelKey: 'nav.network', icon: Network, group: 'machine' },
  { id: 'appHistory', labelKey: 'nav.appHistory', icon: Gauge, group: 'more' },
  { id: 'users', labelKey: 'nav.users', icon: Users, group: 'more' },
  { id: 'benchmarks', labelKey: 'nav.benchmarks', icon: Cpu, group: 'more' },
];

/**
 * Asks the Processes screen to select one process, by PID.
 *
 * Set by a screen that names a process — the diagnosis dialog's "Show in
 * Processes" — immediately before navigating there. A module slot plus an
 * event rather than URL state: the Processes screen may not be mounted yet
 * (it then reads the slot on mount), or may be mounted and hidden in an
 * `<Activity>` (it then hears the event).
 */
export const SELECT_PROCESS_EVENT = 'vitals:select-process';

let pendingProcessPid: number | null = null;

export function requestProcessSelection(pid: number): void {
  pendingProcessPid = pid;
  window.dispatchEvent(new Event(SELECT_PROCESS_EVENT));
}

/** Returns and clears the pending request, so it is honoured exactly once. */
export function takeProcessSelection(): number | null {
  const pid = pendingProcessPid;
  pendingProcessPid = null;
  return pid;
}
