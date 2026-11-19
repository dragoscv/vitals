/**
 * The error every Tauri command rejects with, and how to read it.
 *
 * `CommandError` serialises as `{ kind, message }` — a plain object, not an
 * `Error`. The screens outside Processes each wrote
 * `cause instanceof Error ? cause.message : String(cause)`, which turns that
 * object into the literal text "[object Object]": a failed uninstall, scan or
 * clear told the user nothing about why. One reader, used everywhere, is what
 * stops the next screen from repeating it.
 */

/** The error shape `CommandError` serialises to. */
export interface CommandErrorShape {
  readonly kind: 'access-denied' | 'not-found' | 'unsupported' | 'refused' | 'internal';
  readonly message: string;
}

export function isCommandError(value: unknown): value is CommandErrorShape {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as { kind?: unknown; message?: unknown };
  return typeof candidate.kind === 'string' && typeof candidate.message === 'string';
}

/**
 * Tauri's own rejection for a command that is not in `generate_handler!`:
 * the bare string `Command <name> not found`.
 *
 * Distinct from `CommandErrorShape` because it never reaches the handler — it
 * is raised by the IPC dispatcher before any Rust runs, so it cannot carry a
 * `kind`. A whole screen can be compiled for one platform and its commands
 * left out of the other build, which is exactly how a working-looking button
 * rejects with a string no user can act on.
 */
const MISSING_COMMAND = /^command .+ not found$/i;

export function isMissingCommand(error: unknown): boolean {
  const text = typeof error === 'string' ? error : error instanceof Error ? error.message : null;
  return text !== null && MISSING_COMMAND.test(text.trim());
}

export interface ErrorMessageOptions {
  /**
   * Shown in place of a missing-command rejection — the localised sentence
   * for "this build does not have that feature". Callers that can offer a
   * platform-specific action pass it; when they do not, a rejection that is
   * not a missing command is returned unchanged, so nothing is hidden.
   */
  readonly unsupported?: string;
}

/**
 * A human-readable message for anything a command or promise rejected with.
 *
 * Tauri's own argument errors arrive as a bare string; those are returned
 * as-is, which is also why `String` is the last resort rather than the first.
 */
export function errorMessage(error: unknown, options?: ErrorMessageOptions): string {
  if (options?.unsupported !== undefined && isMissingCommand(error)) {
    return options.unsupported;
  }
  if (isCommandError(error)) return error.message;
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  try {
    return JSON.stringify(error) ?? String(error);
  } catch {
    return String(error);
  }
}
