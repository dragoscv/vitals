/**
 * Every failure the client can surface, as one class with a discriminant.
 *
 * Callers switch on `kind`, not on the message: the server deliberately sends
 * the same body for a missing token and a wrong token, so the message carries
 * no information a UI could branch on. The status code and the body are kept
 * for logging only.
 */

import type { ControlError } from './control';

/** The categories a caller can meaningfully react to differently. */
export type VitalsErrorKind =
  /** The request never reached a server, or the connection dropped mid-way. */
  | 'network'
  /** 401 — no token, or a token the server does not know (indistinguishable by design). */
  | 'unauthorised'
  /** 403 — the token is valid but read-only, or Windows refused the action. */
  | 'forbidden'
  /** Any other non-2xx status. `status` and `detail` say which. */
  | 'http'
  /** A 2xx reply whose body was not what the contract promises. */
  | 'protocol'
  /** A request was pending on a socket that closed before the reply arrived. */
  | 'closed';

/** Fields a {@link VitalsError} may carry beyond its `kind`. */
export interface VitalsErrorDetails {
  /** The HTTP status, when the failure was an HTTP reply. */
  status?: number;
  /** The server's structured refusal, when it sent one. */
  detail?: ControlError;
  /** The underlying exception, so a log line can keep the original stack. */
  cause?: unknown;
}

/** A failure talking to a Vitals server. */
export class VitalsError extends Error {
  override readonly name = 'VitalsError';
  readonly kind: VitalsErrorKind;
  readonly status: number | undefined;
  readonly detail: ControlError | undefined;

  constructor(kind: VitalsErrorKind, message: string, details: VitalsErrorDetails = {}) {
    // `cause` is passed through `ErrorOptions` rather than assigned afterwards
    // so that `error.cause` is the native, non-enumerable slot every logger
    // already knows how to print.
    super(message, ...(details.cause !== undefined ? [{ cause: details.cause }] : []));
    this.kind = kind;
    this.status = details.status;
    this.detail = details.detail;
  }
}

/** Narrowing helper for `catch (error: unknown)` blocks. */
export function isVitalsError(error: unknown): error is VitalsError {
  return error instanceof VitalsError;
}
