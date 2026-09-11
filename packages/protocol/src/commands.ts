/**
 * Shapes returned by desktop-only Tauri commands that have no `vitals-core`
 * type behind them.
 *
 * Hand-written rather than generated because they describe the desktop
 * shell's own state (registry hooks, launch flags), not a measurement, and
 * `vitals-core` must stay free of Windows-shell concepts so it keeps
 * compiling on Linux. The Rust side is `apps/desktop/src-tauri/src/commands.rs`;
 * both carry `camelCase` and the drift gate checks the field names.
 */

/** What Ctrl+Shift+Esc and the taskbar's "Task Manager" entry open. */
export interface TaskManagerReplacement {
  /** Vitals is registered as the Task Manager replacement. */
  readonly enabled: boolean;
  /**
   * Another program (Process Explorer, System Informer) owns the hook. When
   * set, `enabled` is `false` and the switch must be disabled and say why.
   */
  readonly replacedBy: string | null;
  /** The executable the hook points at when `enabled`. */
  readonly path: string | null;
}
