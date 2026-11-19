/**
 * Feeds the nav visibility store from `get_capabilities`.
 *
 * Its own small module so the shell can import it without pulling a screen
 * into the entry chunk; the command is a dynamic import, exactly as the
 * Health visibility module next door does it for `health_status`.
 *
 * Why the platform and not one probe per command: every consumer asks the
 * same question — "can this build show a Windows-only page?" — and
 * `Capabilities.platform` is answered from `cfg!`, so it cannot disagree with
 * what was compiled (the type's own comment says so). Calling six commands and
 * catching `Unsupported` would answer the same thing more slowly, and would
 * call commands that do not exist on this platform.
 */

import { useEffect } from 'react';

import type { Capabilities, Capability, Platform, Unavailable } from '@vitals/protocol';

import { useNavVisibility } from './navigation';

/** Injectable so tests supply a platform without a Tauri host. */
export type PlatformReader = () => Promise<Platform | null>;

async function tauriPlatform(): Promise<Platform | null> {
  const { invoke } = await import('@tauri-apps/api/core');
  const report = await invoke<Capabilities>('get_capabilities');
  return report.platform;
}

/**
 * Publishes the platform, or `null` when it could not be read.
 *
 * `null` fails open — every destination stays visible — which is the right
 * direction for this particular fact: a report that could not be read must
 * never hide a page the user can actually use.
 */
export async function refreshPlatformVisibility(
  read: PlatformReader = tauriPlatform,
): Promise<void> {
  let platform: Platform | null;
  try {
    platform = await read();
  } catch {
    // Read once, assigned once: an unreadable report is the same fact as an
    // absent one, and both mean "hide nothing".
    platform = null;
  }
  useNavVisibility.getState().setPlatform(platform);
}

/** Once per shell mount; nothing changes the platform while the app runs. */
export function usePlatformVisibility(read?: PlatformReader): void {
  useEffect(() => {
    void refreshPlatformVisibility(read);
  }, [read]);
}

/**
 * Whether this build can run something the backend compiles for one platform
 * only — the menu items and buttons whose commands are `#[cfg(windows)]`.
 *
 * An unknown platform fails open, as everywhere else in this module: a fact
 * that could not be read must never take away a control that works, and the
 * commands themselves refuse with a reason the screen already renders.
 */
export function runsOn(platform: Platform | null, wanted: Platform): boolean {
  return platform === null || platform === wanted;
}

/**
 * Whether the host can actually exercise `capability`.
 *
 * The finer question behind `runsOn`: "may this build raise a window" is a
 * platform fact, but "can it end this process" is not — the Mac App Store
 * build is macOS and still cannot, and a Linux build has the command compiled
 * out entirely. Asking the report's own availability list is the only answer
 * that stays true for all three, and it is what `Capabilities.available`
 * exists for.
 *
 * Fails OPEN when there is no report, on the same rule as `runsOn`: a fact
 * that could not be read must never take away a control that works. It does
 * NOT fail open for a report that was read — an empty `available` list means
 * the host can do none of this, which is exactly what a platform without a
 * backend reports.
 */
export function supports(report: Capabilities | null | undefined, capability: Capability): boolean {
  if (!report) return true;
  return report.available.includes(capability);
}

/**
 * Why `capability` cannot be exercised here, or `null` when it can — or when
 * the report could not be read, in which case there is nothing honest to say.
 */
export function unavailableReason(
  report: Capabilities | null | undefined,
  capability: Capability,
): Unavailable | null {
  if (!report) return null;
  return report.unavailable.find(([name]) => name === capability)?.[1] ?? null;
}

/**
 * The first reason any of `wanted` is unavailable, for a surface that asks
 * one question about a group of actions ("can this build act on a process at
 * all?"). `fallback` is returned when none of them carries a reason — which
 * includes a report that was read but lists nothing, the shape a platform
 * without a backend reports.
 */
export function firstUnavailableReason(
  report: Capabilities | null | undefined,
  wanted: readonly Capability[],
  fallback: Unavailable,
): Unavailable {
  for (const capability of wanted) {
    const why = unavailableReason(report, capability);
    if (why !== null) return why;
  }
  return fallback;
}

/** Whether the host can perform any one of `wanted`. */
export function supportsAny(
  report: Capabilities | null | undefined,
  wanted: readonly Capability[],
): boolean {
  return wanted.some((capability) => supports(report, capability));
}
