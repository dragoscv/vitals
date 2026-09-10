/**
 * `@vitals/client` — a dependency-free TypeScript client for the Vitals
 * local API. Types come from `@vitals/protocol`; nothing here redefines a
 * frame.
 */

export {
  VitalsClient,
  type VitalsClientOptions,
  type StreamHandler,
  type Unsubscribe,
  type VitalsConnection,
} from './client';
export {
  VitalsError,
  isVitalsError,
  type VitalsErrorKind,
  type VitalsErrorDetails,
} from './errors';
export type {
  ControlRequest,
  ControlReply,
  ControlError,
  ControlPriority,
  Health,
} from './control';
export {
  backoffDelay,
  DEFAULT_BACKOFF,
  Reconnector,
  type BackoffOptions,
  type Attempt,
  type Cancel,
} from './backoff';
