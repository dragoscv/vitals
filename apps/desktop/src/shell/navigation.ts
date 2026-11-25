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

let pendingSettingsTab: string | null = null;

/**
 * Opens Settings on a given tab — the Health page's "Pair again" lands on
 * the Health section rather than on General. Same slot-plus-event shape as
 * the process selection below; the dialog reads the slot when it mounts.
 */
export function openSettingsAt(tab: string): void {
  pendingSettingsTab = tab;
  window.dispatchEvent(new Event(OPEN_SETTINGS_EVENT));
}

/** Returns and clears the requested tab, so it is honoured exactly once. */
export function takeSettingsTab(): string | null {
  const tab = pendingSettingsTab;
  pendingSettingsTab = null;
  return tab;
}
import {
  Activity,
  Bot,
  Boxes,
  ClockArrowUp,
  Cpu,
  Gauge,
  HardDrive,
  HeartPulse,
  LayoutDashboard,
  ListTree,
  MessagesSquare,
  Network,
  Server,
  Usb,
  Users,
  type LucideIcon,
} from 'lucide-react';
import { useMemo } from 'react';
import { create } from 'zustand';

import type { Platform } from '@vitals/protocol';

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
  'collab',
  'startup',
  'services',
  'appHistory',
  'users',
  'network',
  'storage',
  'installedApps',
  'devices',
  'benchmarks',
  'health',
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
  { id: 'collab', labelKey: 'nav.collab', icon: MessagesSquare, group: 'now' },
  { id: 'startup', labelKey: 'nav.startup', icon: ClockArrowUp, group: 'machine' },
  { id: 'services', labelKey: 'nav.services', icon: Server, group: 'machine' },
  { id: 'storage', labelKey: 'nav.storage', icon: HardDrive, group: 'machine' },
  { id: 'installedApps', labelKey: 'nav.installedApps', icon: Boxes, group: 'machine' },
  { id: 'devices', labelKey: 'nav.devices', icon: Usb, group: 'machine' },
  { id: 'network', labelKey: 'nav.network', icon: Network, group: 'machine' },
  { id: 'appHistory', labelKey: 'nav.appHistory', icon: Gauge, group: 'more' },
  { id: 'users', labelKey: 'nav.users', icon: Users, group: 'more' },
  { id: 'benchmarks', labelKey: 'nav.benchmarks', icon: Cpu, group: 'more' },
  // Last, after the first nine, so Ctrl+1…9 never shifts when it appears.
  // Hidden until a vmui connection exists — see `useNavVisibility`.
  { id: 'health', labelKey: 'nav.health', icon: HeartPulse, group: 'more' },
];

/**
 * Which optional destinations are shown.
 *
 * The single source of truth for "is this route reachable": the sidebar, the
 * palette, the shortcuts, route restore and the hidden-mount preload all read
 * it, so a page cannot be hidden in one place and reachable from another.
 * `null` means not yet known — treated as hidden, so a launch never flashes
 * an entry that then disappears.
 */
interface NavVisibility {
  /** A vmui connection is stored (health_status `configured`). */
  readonly health: boolean | null;
  /**
   * Whether agentq is installed and has state here, from
   * `get_agentq_status`. `null` until the probe answers, and deliberately
   * treated as visible rather than hidden: the probe is three `stat` calls,
   * and hiding a page on every launch only to flash it back a moment later
   * is what the platform rule above refuses to do too.
   *
   * `false` on Windows means the AI page would be empty, so it is not
   * offered. On another platform the page still lists AI sessions, so the
   * probe is not consulted at all there.
   */
  readonly agentq: boolean | null;
  /**
   * What this build runs on, from `get_capabilities`.
   *
   * `null` until the report arrives, and deliberately treated as "show
   * everything" rather than as hidden: the report answers in milliseconds,
   * and hiding eleven destinations on a Windows launch would flash the
   * sidebar back to full a moment later. Nothing can be clicked in that
   * window anyway.
   */
  readonly platform: Platform | null;
  readonly setHealth: (configured: boolean) => void;
  readonly setAgentq: (present: boolean) => void;
  readonly setPlatform: (platform: Platform | null) => void;
}

export const useNavVisibility = create<NavVisibility>((set) => ({
  health: null,
  agentq: null,
  platform: null,
  setHealth: (configured) => {
    set({ health: configured });
  },
  setAgentq: (present) => {
    set({ agentq: present });
  },
  setPlatform: (platform) => {
    set({ platform });
  },
}));

/**
 * The visibility facts, every one optional.
 *
 * A caller that knows one fact passes one — the Sidebar's tests pass only
 * `health` — and a missing fact means "not known yet", which is also what the
 * store's `null` means. Kept as a Partial rather than a second interface so
 * the store's own state satisfies it unchanged.
 */
export type VisibilityState = Partial<Pick<NavVisibility, 'health' | 'platform' | 'agentq'>>;

/**
 * Destinations whose backend commands are compiled only on Windows
 * (`apps/desktop/src-tauri/src/lib.rs`: `#[cfg(windows)]` on the
 * `inventory::*` and `commands::*` entries that serve them). Asking the
 * platform is cheaper and more truthful than calling each command and
 * catching `Unsupported`, and it is what `Capabilities.platform` exists for —
 * its own comment says the question is "can this build show a Windows-only
 * page?".
 *
 * A route that is merely *partly* Windows-specific stays here: the Processes
 * screen, for instance, works on macOS with its Windows-only controls absent,
 * because those controls are gated per command instead.
 */
export const windowsOnlyRoutes: readonly RouteId[] = [
  'network',
  'startup',
  'services',
  'installedApps',
  'storage',
  'devices',
];

/** Whether this build can show the destination at all. */
export function routeSupported(id: RouteId, platform: Platform | null | undefined): boolean {
  // Falsy covers both "the report has not answered yet" and a caller that
  // passed only `{ health }`; either way nothing is hidden.
  if (!platform) return true;
  return platform === 'windows' || !windowsOnlyRoutes.includes(id);
}

/** Whether a route is shown, given the visibility state. */
export function isRouteVisible(
  id: RouteId,
  state: VisibilityState = useNavVisibility.getState(),
): boolean {
  if (id === 'health') return state.health === true;
  // Only a Windows build with no agentq has nothing to show here, and only
  // once the probe has said so: `agentq === false` is that answer, `null` is
  // "not asked yet". The collaboration page reads the same coordination
  // directory, so it is shown on exactly the same fact.
  if (id === 'agents' || id === 'collab') return state.agentq !== false;
  return routeSupported(id, state.platform);
}

/** The nav items currently shown, in sidebar order. */
export function visibleNavItems(
  state: VisibilityState = useNavVisibility.getState(),
): readonly NavItem[] {
  return navItems.filter((item) => isRouteVisible(item.id, state));
}

/**
 * React binding for `visibleNavItems`; re-renders when visibility changes.
 *
 * Derived through `useMemo` from the raw facts rather than from a table of
 * every combination: health, platform and agentq are independent, so a table
 * would need eight hand-written entries and one of them would eventually
 * disagree with `isRouteVisible`, which is the rule that actually counts.
 */
export function useVisibleNavItems(): readonly NavItem[] {
  const health = useNavVisibility((state) => state.health);
  const platform = useNavVisibility((state) => state.platform);
  const agentq = useNavVisibility((state) => state.agentq);
  return useMemo(() => visibleNavItems({ health, platform, agentq }), [health, platform, agentq]);
}

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

/**
 * Asks the Storage screen to open the drive clean-up wizard for one mount.
 *
 * Set by the "Clean up this drive" action on a low-space alert immediately
 * before navigating to Storage; same slot-plus-event shape as
 * `requestProcessSelection`, for the same reason.
 */
export const OPEN_DRIVE_CLEANUP_EVENT = 'vitals:open-drive-cleanup';

let pendingDriveCleanup: string | null = null;

export function requestDriveCleanup(mount: string): void {
  pendingDriveCleanup = mount;
  window.dispatchEvent(new Event(OPEN_DRIVE_CLEANUP_EVENT));
}

/** Returns and clears the pending request, so it is honoured exactly once. */
export function takeDriveCleanup(): string | null {
  const mount = pendingDriveCleanup;
  pendingDriveCleanup = null;
  return mount;
}
