/**
 * Human text for a capability's unavailability reason.
 *
 * The keys live in the global namespace (`capability.*`) because more than one
 * feature names the same reasons, and they are mapped here — rather than
 * inline at each call site — so a new `Unavailable` variant is a compile error
 * in one place instead of a raw key path on someone's screen.
 */

import type { Unavailable } from '@vitals/protocol';

export const REASON_KEYS = {
  notSupportedOnPlatform: 'capability.notSupported',
  noSuchHardware: 'capability.noSuchHardware',
  needsElevation: 'capability.needsElevation',
  needsHelper: 'capability.needsHelper',
  needsPlugin: 'capability.needsPlugin',
  disabledByUser: 'capability.disabledByUser',
  appSandbox: 'capability.appSandbox',
  notImplemented: 'capability.notImplemented',
} as const satisfies Record<Unavailable, string>;
