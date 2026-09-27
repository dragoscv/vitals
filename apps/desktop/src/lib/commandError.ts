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
 * A human-readable message for anything a command or promise rejected with.
 *
 * Tauri's own argument errors arrive as a bare string; those are returned
 * as-is, which is also why `String` is the last resort rather than the first.
 */
export function errorMessage(error: unknown): string {
  if (isCommandError(error)) return error.message;
  if (error instanceof Error) return error.message;
  if (typeof error === 'string') return error;
  try {
    return JSON.stringify(error) ?? String(error);
  } catch {
    return String(error);
  }
}
