/**
 * The platform rule behind the sidebar, the palette, the shortcuts, route
 * restore and the preload — all five read `isRouteVisible`, so they are
 * exercised once here rather than five times.
 *
 * The row this covers (S24-04) asks for exactly one observable thing: no
 * Windows-only route is visible when the reported platform is not Windows.
 */

import { afterEach, describe, expect, it } from 'vitest';

import type { Platform } from '@vitals/protocol';

import {
  isRouteVisible,
  navItems,
  routeIds,
  routeSupported,
  useNavVisibility,
  visibleNavItems,
  windowsOnlyRoutes,
} from './navigation';

/** The store is module-global: every test starts from "nothing known yet". */
function set(
  health: boolean | null,
  platform: Platform | null,
  agentq: boolean | null = null,
): void {
  useNavVisibility.setState({ health, platform, agentq });
}

afterEach(() => {
  set(null, null);
});

const crossPlatform = ['dashboard', 'performance', 'processes', 'agents'] as const;

describe('destination visibility', () => {
  it('hides nothing but Health until the platform report arrives', () => {
    set(null, null);
    const shown = visibleNavItems().map((item) => item.id);
    // Fail-open: a report that has not been read must not hide a page the
    // user can use, and Windows is where nearly every launch happens.
    for (const id of windowsOnlyRoutes) expect(shown).toContain(id);
    expect(shown).not.toContain('health');
  });

  it('hides every Windows-only destination on macOS', () => {
    set(false, 'macos');
    const shown = visibleNavItems().map((item) => item.id);
    for (const id of windowsOnlyRoutes) expect(shown).not.toContain(id);
    for (const id of crossPlatform) expect(shown).toContain(id);
  });

  it('hides every Windows-only destination on Linux', () => {
    set(false, 'linux');
    const shown = visibleNavItems().map((item) => item.id);
    for (const id of windowsOnlyRoutes) expect(shown).not.toContain(id);
  });

  it('keeps Health on macOS once a vmui is connected, because the two rules compose', () => {
    set(true, 'macos');
    const shown = visibleNavItems().map((item) => item.id);
    expect(shown).toContain('health');
    for (const id of windowsOnlyRoutes) expect(shown).not.toContain(id);
  });

  it('shows every destination on Windows, Health excepted', () => {
    set(false, 'windows');
    expect(visibleNavItems()).toHaveLength(navItems.length - 1);
    set(true, 'windows');
    expect(visibleNavItems()).toHaveLength(navItems.length);
  });

  it('reads the store when no state is passed, which is what the shortcuts and the route restore do', () => {
    set(false, 'macos');
    expect(isRouteVisible('network')).toBe(false);
    expect(isRouteVisible('processes')).toBe(true);
    expect(isRouteVisible('health')).toBe(false);
    set(true, 'macos');
    expect(isRouteVisible('health')).toBe(true);
  });

  it('names only destinations that exist, so a rename cannot silently stop hiding one', () => {
    for (const id of windowsOnlyRoutes) expect(routeIds).toContain(id);
  });

  it('leaves the cross-platform destinations supported on every platform', () => {
    for (const platform of ['windows', 'macos', 'linux'] as const) {
      for (const id of crossPlatform) expect(routeSupported(id, platform)).toBe(true);
    }
  });

  it('asks the platform about a Windows-only destination and nothing else', () => {
    expect(routeSupported('storage', 'windows')).toBe(true);
    expect(routeSupported('storage', 'macos')).toBe(false);
    expect(routeSupported('storage', null)).toBe(true);
  });
});

/**
 * The AI page is the one destination backed by a separate program rather than
 * by a command, so it is the one that can be missing on a machine that runs
 * Windows. The failure worth guarding is hiding it too eagerly: an unasked
 * probe (`null`) and an installed agentq (`true`) both keep it, and only a
 * definite "agentq is absent and no client has ever run" takes it away.
 */
describe('the AI page when agentq is not installed', () => {
  it('keeps the AI page until the probe has answered', () => {
    set(true, 'windows', null);
    expect(isRouteVisible('agents')).toBe(true);
  });

  it('keeps the AI page when agentq is installed', () => {
    set(true, 'windows', true);
    expect(isRouteVisible('agents')).toBe(true);
  });

  it('hides the AI page once the probe says agentq is absent', () => {
    set(true, 'windows', false);
    expect(isRouteVisible('agents')).toBe(false);
  });

  it('hides nothing else when agentq is absent, because the two facts are independent', () => {
    set(true, 'windows', false);
    const shown = visibleNavItems().map((item) => item.id);
    for (const id of windowsOnlyRoutes) expect(shown).toContain(id);
    expect(shown).toContain('health');
    expect(shown).not.toContain('agents');
  });
});
