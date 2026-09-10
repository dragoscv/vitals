/**
 * The control surface: what a client may ask the host to do to a process.
 *
 * These are hand-written rather than generated because they live in
 * `vitals-server`, not `vitals-core`, and the ts-rs export covers only the
 * domain model. The shapes mirror `crates/vitals-server/src/control.rs`
 * exactly — `action`/`kind` tags, kebab-case — and the wire tests on the Rust
 * side are the contract that keeps them honest.
 */

import type { ProcessKey } from '@vitals/protocol';

/**
 * Scheduling priority as the control endpoint spells it.
 *
 * Deliberately not the protocol's `Priority` (which is camelCase): the server
 * parses these exact kebab-case strings, the same set the desktop's context
 * menu uses, and accepting the other spelling here would only produce an
 * `unsupported` refusal at runtime.
 */
export type ControlPriority =
  'idle' | 'below-normal' | 'normal' | 'above-normal' | 'high' | 'realtime';

/**
 * A command for the host. Every variant carries a full {@link ProcessKey},
 * never a bare PID, because a phone that took a second to tap "End" must not
 * kill whatever recycled the PID in the meantime.
 */
export type ControlRequest =
  | { action: 'terminate'; key: ProcessKey }
  | { action: 'suspend'; key: ProcessKey }
  | { action: 'resume'; key: ProcessKey }
  | { action: 'set-priority'; key: ProcessKey; priority: ControlPriority };

/** Why the host refused a {@link ControlRequest}. */
export type ControlError =
  /** The token can read but not act. */
  | { kind: 'forbidden' }
  /** The process is gone, or the key no longer matches (PID recycled). */
  | { kind: 'not-found' }
  /** Windows refused. Elevation might help, or the process may be protected. */
  | { kind: 'access-denied' }
  /** The host has no way to do this (no backend, unknown priority…). */
  | { kind: 'unsupported'; message: string }
  | { kind: 'internal'; message: string };

/** The WebSocket reply to a control request. Over HTTP the equivalent is a status code. */
export interface ControlReply {
  ok: boolean;
  error?: ControlError;
}

/** What `GET /api/v1/health` returns. */
export interface Health {
  ok: boolean;
  version: string;
  /**
   * The frame-shape version the server speaks, so a client can refuse to
   * parse a model it does not know rather than rendering garbage.
   */
  modelVersion: number;
}
